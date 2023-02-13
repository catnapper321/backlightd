use std::path::PathBuf;

#[derive(Debug)]
pub enum Error {
    BadPath(PathBuf),
    IOError(std::io::Error),
    BadParse,
    MaxBrightnessRequired,
    NoBacklightStatus,
    BadConfiguration(&'static str),
    NoConfigFile,
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ERROR")
    }
}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::IOError(value)
    }
}
