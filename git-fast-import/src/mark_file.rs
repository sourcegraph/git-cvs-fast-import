use std::{
    io::{BufReader, Read, Seek},
    num::ParseIntError,
    str::FromStr,
};

use nom::{
    bytes::complete::tag,
    character::complete::{alphanumeric1, digit1, multispace1},
    combinator::map_res,
    sequence::{delimited, terminated},
    Finish, IResult,
};
use rev_lines::RevLines;

use crate::{Error, Mark};

pub(crate) fn get_last_mark<R>(reader: R) -> Result<Option<Mark>, Error>
where
    R: Read + Seek,
{
    if let Some(line) = RevLines::new(BufReader::new(reader))?
        .into_iter()
        .find(|line| !line.is_empty())
    {
        Ok(Some(
            Finish::finish(mark_line(&line))
                .map_err(|e| Error::MarkParsingError(e.code))?
                .1,
        ))
    } else {
        Ok(None)
    }
}

fn mark_line(input: &str) -> IResult<&str, Mark> {
    map_res(
        terminated(delimited(tag(":"), digit1, multispace1), alphanumeric1),
        |raw| -> Result<Mark, ParseIntError> { Mark::from_str(raw) },
    )(input)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    macro_rules! assert_get_last_mark_error {
        ($input:expr) => {
            assert!(get_last_mark(Cursor::new($input)).is_err());
        };
    }

    macro_rules! assert_get_last_mark_ok {
        ($input:expr, $want:expr) => {
            assert_eq!(get_last_mark(Cursor::new($input)).unwrap(), $want);
        };
    }

    #[test]
    fn test_get_last_mark() {
        assert_get_last_mark_ok!(b"", None);
        assert_get_last_mark_ok!(b"\n", None);
        assert_get_last_mark_ok!(
            b":25 0123456789012345678901234567890123456789",
            Some(Mark(25))
        );
        assert_get_last_mark_ok!(
            b":25 0123456789012345678901234567890123456789\n\n",
            Some(Mark(25))
        );

        assert_get_last_mark_error!(b"not a mark");
        assert_get_last_mark_error!(b":xx xx");
        assert_get_last_mark_error!(b":25");
        assert_get_last_mark_error!(b":25 \n");
        assert_get_last_mark_error!(b"25 xx");
    }
}
