#![allow(dead_code, unused_imports)]
mod clamped;
mod config;
mod options;
mod error;
mod scale;

use std::{
    env,
    ffi::{OsStr, OsString},
    fs::{self, read_to_string, write},
    io::{self, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
    time::Duration,
};
use log::{trace, debug, info, warn, error};

use backlightd::BacklightCommand;
use clamped::*;
use config::get_config;
use error::*;
use scale::*;

const RETRY_INTERVAL: Duration = Duration::from_secs(2);
const STEPS_IN_REFERENCE_RANGE: f32 = 9.0;
const DEFAULT_LEVEL: i8 = 4;

pub type Anything<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, PartialEq)]
pub enum ControlMethod {
    /// An existing path for the parameter, e.g. /sys/class/drm/card0-eDP-1/dpms
    SysFS(PathBuf),
    /// Display number reported by ddcutil detect
    DDCUtil(u8),
    /// Name of the display used by swaymsg (e.g. eDP-1). Only used for on/off
    /// via DPMS.
    SwayDPMS(String),
}

impl ControlMethod {
    /// Display name like "card0-DP-1", filepath like "brightness"
    pub fn new_sysfs(display_name: impl AsRef<Path>, filepath: impl AsRef<Path>) -> Option<Self> {
        let mut p = PathBuf::from("/sys/class/drm");
        p.push(display_name);
        p.push(filepath);
        if matches!(p.try_exists(), Ok(true)) {
            Some(Self::SysFS(p))
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Display {
    // must be sysfs type
    dpms_control: Option<ControlMethod>,
    brightness_control: Option<ControlMethod>,
    scale: BrightnessScale,
    name: OsString,
}

impl Display {
    pub fn is_on(&self) -> Result<bool, Error> {
        if let Some(ControlMethod::SysFS(ref p)) = self.dpms_control {
            let x = read_to_string(p)?;
            Ok(x == "0\n")
        } else {
            Err(Error::NoBacklightStatus)
        }
    }
    pub fn is_off(&self) -> Result<bool, Error> {
        if let Some(ControlMethod::SysFS(ref p)) = self.dpms_control {
            let x = read_to_string(p)?;
            Ok(x == "4\n")
        } else {
            Err(Error::NoBacklightStatus)
        }
    }
    pub fn get_brightness(&self) -> ClampedValue<usize> {
        self.scale.get_brightness()
    }
    fn set_brightness(&mut self, v: usize) -> Result<(), io::Error> {
        debug!("Setting brightness to {v}");
        match self.brightness_control {
            Some(ControlMethod::SysFS(ref p)) => fs::write(p, v.to_string()),
            Some(ControlMethod::DDCUtil(display)) => ddcutil_set_brightness(display, v),
            // cannot use sway to set brightness
            _ => {
                error!("Cannot use swaydpms to set brightness for {:?}", self.name);
                Ok(())
            },
        }
    }
    pub fn set_brightness_level(&mut self, level: i8) -> Result<ClampedValue<usize>, io::Error> {
        debug!("Setting brightness on {:?} to {level}", self.name);
        let v = self.scale.set_level(level);
        self.set_brightness(*v).map(|_| v)
    }
    pub fn brightness_up(&mut self) -> Result<ClampedValue<usize>, io::Error> {
        debug!("Brightness up on {:?}", self.name);
        let v = self.scale.up();
        self.set_brightness(*v).map(|_| v)
    }
    pub fn brightness_down(&mut self) -> Result<ClampedValue<usize>, io::Error> {
        debug!("Brightness down on {:?}", self.name);
        let v = self.scale.down();
        self.set_brightness(*v).map(|_| v)
    }
    pub fn turn_on(&mut self) -> Result<(), io::Error> {
        debug!("Turning on {:?}", self.name);
        match self.dpms_control {
            Some(ControlMethod::SysFS(ref p)) => fs::write(p, "0"),
            Some(ControlMethod::SwayDPMS(ref name)) => dpms_sway_turn_on(name),
            _ => {
                error!("Cannot use ddcutil to turn on {:?}", self.name);
                Ok(())
            },
        }
    }
    pub fn turn_off(&mut self) -> Result<(), io::Error> {
        debug!("Turning off {:?}", self.name);
        match self.dpms_control {
            Some(ControlMethod::SysFS(ref p)) => fs::write(p, "4"),
            Some(ControlMethod::SwayDPMS(ref name)) => dpms_sway_turn_off(name),
            _ => {
                error!("Cannot use ddcutil to turn off {:?}", self.name);
                Ok(())
            },
        }
    }
}

fn ddcutil_set_brightness(display: u8, v: usize) -> Result<(), io::Error> {
    let mut cmd = Command::new("/usr/bin/ddcutil");
    cmd.arg("setvcp")
        .arg("10")
        .arg(v.to_string())
        .arg("--noverify")
        .arg("--display")
        .arg(display.to_string());
    let mut child = cmd.spawn()?;
    let _ = child.wait();
    Ok(())
}

fn dpms_sway_turn_on(name: impl AsRef<str>) -> Result<(), io::Error> {
    let mut child = Command::new("/usr/bin/swaymsg")
        .arg("-q") //quiet
        .arg("output")
        .arg(name.as_ref())
        .arg("power")
        .arg("on")
        .spawn()?;
    child.wait()?;
    Ok(())
}

fn dpms_sway_turn_off(name: impl AsRef<str>) -> Result<(), io::Error> {
    let mut child = Command::new("/usr/bin/swaymsg")
        .arg("-q") //quiet
        .arg("output")
        .arg(name.as_ref())
        .arg("power")
        .arg("off")
        .spawn()?;
    child.wait()?;
    Ok(())
}

// Wrapper for Path::try_exists, maps the Ok(true) result to Ok(()), and
// maps Ok(false) and Err to the application error type
fn check_exists(p: &Path) -> Result<(), Error> {
    if !p.try_exists()? {
        Err(Error::BadPath(p.into()))
    } else {
        Ok(())
    }
}

/// Read utf8 data from a file
fn read_value_from_file<T: FromStr>(p: impl AsRef<Path>) -> Result<T, Error> {
    let b = fs::read_to_string(p)?;
    b.trim().parse().map_err(|_| Error::BadParse)
}

fn establish_socket(path: impl AsRef<Path>) -> Anything<UnixListener> {
    let path = path.as_ref();
    let exists = path.try_exists()?;
    if exists {
        warn!("Removing existing socket at {path:?}");
        // try to remove the socket, if it already exists
        let _ = std::fs::remove_file(path)?;
    }
    trace!("Binding socket");
    let listener = UnixListener::bind(path)?;
    Ok(listener)
}

fn run(listener: UnixListener, mut config: config::Config) -> Anything<()> {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let (mut client, _) = listener.accept()?;
        client.read_to_end(&mut buf)?;
        let cmd = BacklightCommand::try_from(buf.as_ref());
        match cmd {
            Ok(x) => {
                execute_command(x, config.mut_displays())?;
            }
            Err(e) => println!("Backlight command error {e:?}"),
        }
    }
}

fn main() -> Anything<()> {
    // parse command line options
    let cli_options = options::CliOptions::new();

    // read config file
    if let Some(config_path) = cli_options.config_file {
        if ! matches!(config_path.try_exists(), Ok(true)) {
            return Err(Box::new(Error::BadPath(config_path)));
        }
    }
    let mut config = get_config()?;

    // set up logging - assume systemd/journald is reading stderr
    let mut logging = env_logger::Builder::new();
    logging.filter_level(config.log_level);
    if ! config.log_timestamp {
        logging.format_timestamp(None);
    }
    logging.init();
    info!("Logging enabled. Level is {:?}", config.log_level);

    // set up the socket
    let socket_path: PathBuf = if let Some(config_path) = cli_options.socket_path {
        trace!("Socket path from config file: {config_path:?}");
        config_path
    } else {
        trace!("Attempting to use XDG_RUNTIME_DIR to construct socket path");
        let xdg_runtime_dir = env::var("XDG_RUNTIME_DIR")?;
        PathBuf::from(xdg_runtime_dir).join("backlight")
    };
    let listener = establish_socket(&socket_path)?;
    debug!("Made socket at {socket_path:?}");

    // set default brightness
    let default_level = config.default_level;
    for d in config.mut_displays() {
        let _ = d.set_brightness_level(default_level);
    }
    run(listener, config)
}

fn execute_command(cmd: BacklightCommand, displays: &mut [Display]) -> Anything<()> {
    use backlightd::TargetDisplay;
    match cmd {
        BacklightCommand::SwaySock(value) => {
            env::set_var("SWAYSOCK", value);
        }
        BacklightCommand::On(display) => match display {
            TargetDisplay::Display(name) => {
                turn_on_display(&name, displays);
            }
            TargetDisplay::All => turn_on_all_displays(displays),
        },
        BacklightCommand::Off(display) => match display {
            TargetDisplay::Display(name) => turn_off_display(&name, displays),
            TargetDisplay::All => turn_off_all_displays(displays),
        },
        BacklightCommand::Up(display) => match display {
            TargetDisplay::Display(name) => display_brightness_up(&name, displays),
            TargetDisplay::All => all_brightness_up(displays),
        },
        BacklightCommand::Down(display) => match display {
            TargetDisplay::Display(name) => display_brightness_down(&name, displays),
            TargetDisplay::All => all_brightness_down(displays),
        },
        BacklightCommand::Toggle(display) => match display {
            TargetDisplay::Display(name) => toggle_display(&name, displays),
            TargetDisplay::All => toggle_all_displays(displays),
        },
        BacklightCommand::Max(_) => todo!(),
        BacklightCommand::Min(_) => todo!(),
        BacklightCommand::Default(_) => todo!(),
    }
    Ok(())
}

fn turn_on_display(name: &OsStr, displays: &mut [Display]) {
    for d in displays {
        if d.name == name {
            let _ = d.turn_on();
        }
    }
}
fn turn_on_all_displays(displays: &mut [Display]) {
    for d in displays {
        let _ = d.turn_on();
    }
}
fn turn_off_display(name: &OsStr, displays: &mut [Display]) {
    for d in displays {
        if d.name == name {
            let _ = d.turn_off();
        }
    }
}
fn turn_off_all_displays(displays: &mut [Display]) {
    for d in displays {
        let _ = d.turn_off();
    }
}
fn toggle_display(name: &OsStr, displays: &mut [Display]) {
    for d in displays {
        if d.name == name {
            match d.is_on() {
                Ok(true) => {
                    let _ = d.turn_off();
                }
                Ok(false) => {
                    let _ = d.turn_on();
                }
                Err(e) => println!("Error getting state of {name:?}: {e:?}"),
            }
        }
    }
}
fn toggle_all_displays(displays: &mut [Display]) {
    if let Some(lead_display) = displays.first() {
        match lead_display.is_on() {
            Ok(true) => turn_off_all_displays(displays),
            Ok(false) => turn_on_all_displays(displays),
            Err(e) => println!("Error getting state of lead display: {e:?}"),
        }
    }
}

fn display_brightness_up(name: &OsStr, displays: &mut [Display]) {
    // Consider every display, as several displays may share the same name
    for d in displays {
        if d.name == name {
            if !d.get_brightness().is_max() {
                let _ = d.brightness_up();
            }
        }
    }
}
fn display_brightness_down(name: &OsStr, displays: &mut [Display]) {
    // Consider every display, as several displays may share the same name
    for d in displays {
        if d.name == name {
            if !d.get_brightness().is_min() {
                let _ = d.brightness_down();
            }
        }
    }
}

fn all_brightness_up(displays: &mut [Display]) {
    if displays.iter().any(|d| !d.get_brightness().is_max()) {
        for d in displays {
            let _ = d.brightness_up();
        }
    }
}
fn all_brightness_down(displays: &mut [Display]) {
    if displays.iter().any(|d| !d.get_brightness().is_min()) {
        for d in displays {
            let _ = d.brightness_down();
        }
    }
}
