use std::ffi::{OsStr, OsString};
use std::os::unix::prelude::OsStrExt;
use std::path::PathBuf;
mod error;

#[derive(Debug, PartialEq)]
pub enum TargetDisplay {
    Display(OsString),
    All,
}

#[derive(Debug, PartialEq)]
pub enum BacklightCommand {
    SwaySock(PathBuf),
    On(TargetDisplay),
    Off(TargetDisplay),
    Up(TargetDisplay),
    Down(TargetDisplay),
    Toggle(TargetDisplay),
    Max(TargetDisplay),
    Min(TargetDisplay),
    Default(TargetDisplay),
}

/// Backlight commands are sent in verb-noun order: "on DP-3"
impl TryFrom<&[u8]> for BacklightCommand {
    type Error = error::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        use parsing::parse_command;
        parse_command(value).map_err(|_| error::Error::BadParse)
    }
}

mod parsing {
    use crate::*;
    // use std::{ffi::OsStr, os::unix::prelude::OsStrExt};
    use nom::{
        branch::alt,
        bytes::complete::{tag_no_case, take_till, take_while},
        combinator::{map, rest},
        sequence::separated_pair,
    };
    type ParseResult<'a, T> = nom::IResult<&'a [u8], T>;

    fn is_space(c: u8) -> bool {
        c == 32
    }

    fn space0(input: &[u8]) -> nom::IResult<&[u8], ()> {
        map(take_while(is_space), |_| ())(input)
    }

    fn not_space(input: &[u8]) -> nom::IResult<&[u8], &[u8]> {
        take_till(is_space)(input)
    }

    fn token(input: &[u8]) -> nom::IResult<&[u8], &[u8]> {
        let mut p = alt((not_space, rest));
        p(input)
    }

    // fn token_no_case<'a>(t: &'static [u8], input: &'a [u8]) -> nom::IResult<&'a [u8], &'a [u8]> {
    //    terminated(tag_no_case(t), space0)(input)
    // }

    fn all_displays(input: &[u8]) -> ParseResult<TargetDisplay> {
        let p = tag_no_case("all");
        map(p, |_| TargetDisplay::All)(input)
    }

    fn specific_display(input: &[u8]) -> ParseResult<TargetDisplay> {
        map(token, |t| TargetDisplay::Display(t.to_os_string()))(input)
    }

    fn display(input: &[u8]) -> ParseResult<TargetDisplay> {
        alt((all_displays, specific_display))(input)
    }

    // Consumes entire input as a PathBuf
    fn path(input: &[u8]) -> ParseResult<PathBuf> {
        let z = input.to_os_string();
        let p = PathBuf::from(z);
        Ok((&[], p))
    }

    fn on_command(input: &[u8]) -> ParseResult<BacklightCommand> {
        let p = separated_pair(tag_no_case("on"), space0, display);
        map(p, |(_, d)| BacklightCommand::On(d))(input)
    }

    fn off_command(input: &[u8]) -> ParseResult<BacklightCommand> {
        let p = separated_pair(tag_no_case("off"), space0, display);
        map(p, |(_, d)| BacklightCommand::Off(d))(input)
    }

    fn up_command(input: &[u8]) -> ParseResult<BacklightCommand> {
        let p = separated_pair(tag_no_case("up"), space0, display);
        map(p, |(_, d)| BacklightCommand::Up(d))(input)
    }

    fn down_command(input: &[u8]) -> ParseResult<BacklightCommand> {
        let p = separated_pair(tag_no_case("down"), space0, display);
        map(p, |(_, d)| BacklightCommand::Down(d))(input)
    }

    fn toggle_command(input: &[u8]) -> ParseResult<BacklightCommand> {
        let p = separated_pair(tag_no_case("toggle"), space0, display);
        map(p, |(_, d)| BacklightCommand::Toggle(d))(input)
    }

    fn swaysock_command(input: &[u8]) -> ParseResult<BacklightCommand> {
        let p = separated_pair(tag_no_case("swaysock"), space0, path);
        map(p, |(_, d)| BacklightCommand::SwaySock(d))(input)
    }

    fn max_command(input: &[u8]) -> ParseResult<BacklightCommand> {
        let p = separated_pair(tag_no_case("max"), space0, display);
        map(p, |(_, d)| BacklightCommand::Max(d))(input)
    }

    fn min_command(input: &[u8]) -> ParseResult<BacklightCommand> {
        let p = separated_pair(tag_no_case("min"), space0, display);
        map(p, |(_, d)| BacklightCommand::Min(d))(input)
    }

    fn reference_command(input: &[u8]) -> ParseResult<BacklightCommand> {
        let p = separated_pair(tag_no_case("default"), space0, display);
        map(p, |(_, d)| BacklightCommand::Default(d))(input)
    }

    pub fn parse_command(input: &[u8]) -> Result<BacklightCommand, ()> {
        let x = alt((
            swaysock_command,
            toggle_command,
            down_command,
            up_command,
            off_command,
            on_command,
            max_command,
            min_command,
            reference_command,
        ))(input);
        match x {
            Ok((_, y)) => Ok(y),
            Err(_) => Err(()),
        }
    }

    #[cfg(test)]
    mod testing {
        use super::*;

        fn ok_result<'a>(b: BacklightCommand) -> ParseResult<'a, BacklightCommand> {
            Ok(("".as_bytes(), b))
        }
        fn make_disp(name: &'static str) -> TargetDisplay {
            TargetDisplay::Display(OsString::from(name))
        }

        #[test]
        fn test_token() {
            let input = "this is a test".as_bytes();
            let result = Ok((" is a test".as_bytes(), "this".as_bytes()));
            assert_eq!(token(input), result);
        }
        #[test]
        fn test_on() {
            let input = "on DP-3".as_bytes();
            let r = ok_result(BacklightCommand::On(make_disp("DP-3")));
            assert_eq!(on_command(input), r);
        }
        #[test]
        fn test_all_on() {
            let input = "on all".as_bytes();
            let r = ok_result(BacklightCommand::On(TargetDisplay::All));
            assert_eq!(on_command(input), r);
        }
        #[test]
        fn test_swaysock() {
            use std::str::FromStr;
            let p_str = "/path/to/the/swaysock";
            let p = PathBuf::from_str(p_str).unwrap();
            let input = format!("swaysock {p_str}");
            let r = ok_result(BacklightCommand::SwaySock(p));
            assert_eq!(swaysock_command(input.as_bytes()), r);
        }
        #[test]
        fn test_parsing() {
            // let input = "DoWn SomeDisplay".as_bytes();
            // let d = make_disp("SomeDisplay");
            // let r = Ok(BacklightCommand::Down(d));
            // assert_eq!(input.try_into(), r);
        }
    }
}

pub trait ByteBagExt {
    fn to_os_string(&self) -> OsString;
}

impl ByteBagExt for [u8] {
    fn to_os_string(&self) -> OsString {
        OsStr::from_bytes(self).to_owned()
    }
}
