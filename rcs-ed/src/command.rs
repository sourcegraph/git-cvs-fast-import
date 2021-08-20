use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::digit1,
    combinator::map,
    sequence::{delimited, tuple},
    Finish, IResult,
};
use thiserror::Error;

/// Command is the internal representation of an ed command.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Add { position: usize, lines: usize },
    Delete { position: usize, lines: usize },
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid ed command: {0}")]
    InvalidCommand(String),

    #[error("missing ed command")]
    NoCommand,
}

impl Command {
    pub(crate) fn parse(line: &[u8]) -> Result<Self, Error> {
        Ok(Finish::finish(command(line))
            .map_err(|e| {
                if e.input.len() == 0 {
                    Error::NoCommand
                } else {
                    Error::InvalidCommand(String::from_utf8_lossy(e.input).to_string())
                }
            })?
            .1)
    }
}

fn command(input: &[u8]) -> IResult<&[u8], Command> {
    alt((
        map(
            tuple((delimited(tag(b"a"), digit1, tag(b" ")), digit1)),
            |(position, lines): (&[u8], &[u8])| Command::Add {
                position: digits_to_usize(position),
                lines: digits_to_usize(lines),
            },
        ),
        map(
            tuple((delimited(tag(b"d"), digit1, tag(b" ")), digit1)),
            |(position, lines): (&[u8], &[u8])| Command::Delete {
                position: digits_to_usize(position),
                lines: digits_to_usize(lines),
            },
        ),
    ))(input)
}

fn digits_to_usize(digits: &[u8]) -> usize {
    // To state the obvious, this function is _wildly_ unsafe if the input is
    // anything other than a slice of ASCII digits.
    unsafe { std::str::from_utf8_unchecked(digits) }
        .parse::<usize>()
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        assert_eq!(
            Command::parse(b"a2 3").unwrap(),
            Command::Add {
                position: 2,
                lines: 3
            }
        );

        assert_eq!(
            Command::parse(b"d20 32121").unwrap(),
            Command::Delete {
                position: 20,
                lines: 32121
            }
        );

        assert!(matches!(Command::parse(b""), Err(Error::NoCommand)));

        assert!(matches!(
            Command::parse(b"a2 "),
            Err(Error::InvalidCommand(_))
        ));

        assert!(matches!(
            Command::parse(b"c1 2"),
            Err(Error::InvalidCommand(_))
        ));

        assert!(matches!(
            Command::parse(b"x"),
            Err(Error::InvalidCommand(_))
        ));
    }
}
