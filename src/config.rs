use crate::{Anything, BrightnessScale, ControlMethod, Display, Error, ScaleBuilder};
use std::path::{Path, PathBuf};
use toml::*;

pub struct Config {
    pub steps_in_reference_range: f32,
    pub default_level: i8,
    pub displays: Vec<Display>,
    pub socket_path: Option<PathBuf>,
}

impl Config {
    pub fn mut_displays(&mut self) -> &mut [Display] {
        self.displays.as_mut_slice()
    }
}

mod parser {
    use crate::error::Error;
    use nom::{
        branch::alt,
        bytes::complete::tag_no_case,
        character::complete::{char, digit1},
        combinator::{map, map_res, opt, recognize, rest},
        sequence::{preceded, tuple},
    };

    use crate::{scale::ScaleKind, ControlMethod};
    type ParseResult<'a, T> = nom::IResult<&'a str, T>;

    fn number_p<T: std::str::FromStr>(input: &str) -> ParseResult<T> {
        let dot_p = preceded(char('.'), digit1);
        let p = recognize(tuple((digit1, opt(dot_p))));
        map_res(p, |x: &str| x.parse::<T>())(input)
    }
    fn sysfs(input: &str) -> ParseResult<ControlMethod> {
        let p = preceded(tag_no_case("sysfs:"), rest);
        map(p, |x: &str| ControlMethod::SysFS(x.into()))(input)
    }
    fn ddcutil(input: &str) -> ParseResult<ControlMethod> {
        let p = preceded(tag_no_case("ddcutil:"), number_p);
        map(p, |x: u8| ControlMethod::DDCUtil(x))(input)
    }
    fn swaydpms(input: &str) -> ParseResult<ControlMethod> {
        let p = preceded(tag_no_case("swaydpms:"), rest);
        map(p, |x: &str| ControlMethod::SwayDPMS(x.into()))(input)
    }
    pub fn parse_control_method(input: &str) -> Result<ControlMethod, Error> {
        match alt((sysfs, ddcutil, swaydpms))(input) {
            Ok((_, v)) => Ok(v),
            Err(_) => Err(Error::BadConfiguration("Could not parse control method")),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::ControlMethod;
        #[test]
        fn test_swaydpms_parsing() {
            let (_, v) = swaydpms("swaydpms:DP-3").unwrap();
            assert_eq!(v, ControlMethod::SwayDPMS("DP-3".into()));
        }
        #[test]
        fn test_ddcutil_parsing() {
            let (_, v) = ddcutil("ddcutil:1").unwrap();
            let expected = ControlMethod::DDCUtil(1);
            assert_eq!(v, expected);
        }
        #[test]
        fn test_sysfs_parsing() {
            let (_, v) = sysfs("sysfs:/path/to/file").unwrap();
            let expected = ControlMethod::SysFS("/path/to/file".into());
            assert_eq!(v, expected);
        }
    }
}

use parser::*;

fn get_usize(table: &Table, key: &str) -> Result<Option<usize>, Error> {
    let x = table.get(key);
    if x.is_none() {
        return Ok(None);
    }
    let x = x.unwrap();
    if let Some(y) = x.as_integer() {
        Ok(Some(y as usize))
    } else {
        Err(Error::BadConfiguration("Could not parse integer value"))
    }
}

fn toml_to_display(t: &Table) -> Result<Display, Error> {
    let Some(name) = t.get("name").and_then(|v| v.as_str()) else {
        return Err(Error::BadConfiguration("Display name is required"));
    };
    // parse controls
    let mut brightness_control: Option<ControlMethod> = None;
    if let Some(v) = t.get("brightness_control") {
        let s = v.as_str().ok_or(Error::BadConfiguration(
            "Could not parse brightness control configuration",
        ))?;
        let cm = parser::parse_control_method(s)?;
        brightness_control = Some(cm);
    }
    let mut onoff_control: Option<ControlMethod> = None;
    if let Some(v) = t.get("onoff_control") {
        let s = v.as_str().ok_or(Error::BadConfiguration(
            "Could not parse onoff control configuration",
        ))?;
        let cm = parser::parse_control_method(s)?;
        onoff_control = Some(cm);
    }
    // build the scale
    let gamma = t.get("gamma").and_then(|v| v.as_float());
    let min_value = get_usize(t, "min")?;
    let max_value = get_usize(t, "max")?;
    let ref_max = get_usize(t, "ref_max")?;
    let ref_min = get_usize(t, "ref_min")?;
    let mut scalebuilder = ScaleBuilder::new();
    if let Some(g) = gamma {
        if g == 1.0 {
            scalebuilder.kind(crate::scale::ScaleKind::Linear);
            // defaults for linear scale
            scalebuilder.max_value(100);
            scalebuilder.min_value(0);
        } else {
            scalebuilder.kind(crate::scale::ScaleKind::Exp2(g as f32));
        }
    }
    if let Some(v) = min_value {
        scalebuilder.min_value(v);
    }
    if let Some(v) = max_value {
        scalebuilder.max_value(v);
    }
    if let Some(v) = ref_max {
        scalebuilder.ref_max_value(v);
    }
    if let Some(v) = ref_min {
        scalebuilder.ref_min_value(v);
    }
    let scale = scalebuilder.make()?;
    Ok(Display {
        dpms_control: onoff_control,
        brightness_control,
        scale,
        name: name.into(),
    })
}

fn parse_config_document(document: impl AsRef<str>) -> Result<Config, Error> {
    let doc = document
        .as_ref()
        .parse::<Table>()
        .map_err(|_| Error::BadConfiguration("Could not parse the configuration document"))?;
    let display_config = doc.get("display").ok_or(Error::BadConfiguration(
        "Could not find a display array in the configuration document",
    ))?;
    let displays_array = display_config.as_array().ok_or(Error::BadConfiguration(
        "Could not parse the display array in the configuration document",
    ))?;
    let mut displays = Vec::new();
    for display_config in displays_array {
        let display_toml_table = display_config.as_table().ok_or(Error::BadConfiguration(
            "Could not parse toml display table",
        ))?;
        let display = toml_to_display(display_toml_table)?;
        displays.push(display);
    }
    let steps = doc
        .get("steps")
        .and_then(|v| v.as_integer())
        .map(|v| v as f32)
        .unwrap_or(9.0);
    let default_level = doc
        .get("default_level")
        .and_then(|v| v.as_integer())
        .map(|v| v as i8)
        .unwrap_or(4);
    let socket_path = doc
        .get("socket_path")
        .and_then(|v| v.as_str())
        .map(|v| PathBuf::from(v));
    Ok(Config {
        steps_in_reference_range: steps,
        default_level,
        displays,
        socket_path,
    })
}

fn get_config_file_contents(p: impl AsRef<Path>) -> Result<String, Error> {
    let path: &Path = p.as_ref();
    let contents = std::fs::read_to_string(path)?;
    Ok(contents)
}

fn get_config_file_path() -> Result<PathBuf, Error> {
    let mut targets: Vec<PathBuf> = Vec::new();
    if let Ok(xdg_user_config) = std::env::var("XDG_CONFIG_HOME") {
        let mut p = PathBuf::from(xdg_user_config);
        p.push("backlightd");
        p.push("config");
        targets.push(p);
    }
    targets.push(PathBuf::from("/etc/backlightd/config"));
    for target in targets {
        if matches!(target.try_exists(), Ok(true)) {
            return Ok(target);
        }
    }
    Err(Error::NoConfigFile)
}

pub fn get_config() -> Result<Config, Error> {
    let config_path = get_config_file_path()?;
    let contents = get_config_file_contents(config_path)?;
    parse_config_document(contents)
}
