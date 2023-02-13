use clap::Parser as ClapParser;
use std::path::PathBuf;

#[derive(Debug, ClapParser)]
pub struct CliOptions {
    /// Use this config file instead of $XDG_CONFIG_HOME/backlightd/config
    #[clap(short = 'c', long = "config", env = "BACKLIGHTD_CONFIG")]
    pub config_file: Option<PathBuf>,
    /// Path for the server unix socket, defaults to $XDG_RUNTIME_DIR/backlight
    #[clap(short = 's', long = "socket", env = "BACKLIGHTD_SOCKET_PATH")]
    pub socket_path: Option<PathBuf>,
}

impl CliOptions {
    pub fn new() -> Self {
        ClapParser::parse()
    }
}
