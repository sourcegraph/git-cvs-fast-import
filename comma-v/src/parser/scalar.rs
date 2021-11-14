use std::{convert::TryFrom, fmt::Debug, str::FromStr, time::SystemTime};

use chrono::{DateTime, NaiveDate, Utc};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_till1, take_while, take_while1},
    character::complete::digit1,
    combinator::{map, map_res, value},
    multi::fold_many0,
    sequence::{delimited, terminated, tuple},
    IResult,
};
use thiserror::Error;

use super::char::*;
use crate::{num, types};

pub(super) fn integrity_string(input: &[u8]) -> IResult<&[u8], types::IntString> {
    // TODO: thirdp support
    map(
        delimited(tag(b"@"), take_while(is_intchar), tag(b"@")),
        |bytes| types::IntString(Vec::from(bytes)),
    )(input)
}

pub(super) fn id(input: &[u8]) -> IResult<&[u8], types::Id> {
    map(take_while(|c| is_idchar(c) || c == b'.'), |bytes| {
        types::Id(Vec::from(bytes))
    })(input)
}

pub(super) fn numlike(input: &[u8]) -> IResult<&[u8], &[u8]> {
    take_while1(|c| c == b'.' || (b'0'..=b'9').contains(&c))(input)
}

pub(super) fn num(input: &[u8]) -> IResult<&[u8], num::Num> {
    map_res(numlike, num::Num::try_from)(input)
}

pub(super) fn string_literal(input: &[u8]) -> IResult<&[u8], &[u8]> {
    take_till1(|c| c == b'@')(input)
}

pub(super) fn string_escape(input: &[u8]) -> IResult<&[u8], &[u8]> {
    value(&b"@"[..], tag(b"@@"))(input)
}

pub(super) fn string(input: &[u8]) -> IResult<&[u8], types::VString> {
    map(
        delimited(
            tag(b"@"),
            fold_many0(
                alt((string_literal, string_escape)),
                Vec::new,
                |mut v, fragment| {
                    v.extend_from_slice(fragment);
                    v
                },
            ),
            tag(b"@"),
        ),
        types::VString,
    )(input)
}

pub(super) fn sym(input: &[u8]) -> IResult<&[u8], types::Sym> {
    map(take_while(is_idchar), |bytes| types::Sym(Vec::from(bytes)))(input)
}

pub(super) fn date(input: &[u8]) -> IResult<&[u8], SystemTime> {
    map_res(
        tuple((
            terminated(digits, tag(b".")),
            terminated(digits, tag(b".")),
            terminated(digits, tag(b".")),
            terminated(digits, tag(b".")),
            terminated(digits, tag(b".")),
            digits,
        )),
        |(year, month, day, hour, minute, second)| -> Result<SystemTime, Error> {
            if let Some(date) =
                NaiveDate::from_ymd_opt(if year < 100 { year + 1900 } else { year }, month, day)
            {
                if let Some(dt) = date.and_hms_milli_opt(
                    hour,
                    minute,
                    if second >= 60 { 59 } else { second },
                    if second >= 60 {
                        (second - 59) * 1000
                    } else {
                        0
                    },
                ) {
                    Ok(DateTime::<Utc>::from_utc(dt, Utc).into())
                } else {
                    Err(Error::InvalidTime {
                        hour,
                        minute,
                        second,
                    })
                }
            } else {
                Err(Error::InvalidDate { year, month, day })
            }
        },
    )(input)
}

fn digits<T>(input: &[u8]) -> IResult<&[u8], T>
where
    T: FromStr,
    T::Err: Debug,
{
    map_res(digit1, |s| {
        T::from_str(unsafe { std::str::from_utf8_unchecked(s) })
    })(input)
}

#[derive(Debug, Error)]
enum Error {
    #[error("invalid date input: {year}-{month}-{day}")]
    InvalidDate { year: i32, month: u32, day: u32 },

    #[error("invalid time input: {hour}:{minute}:{second}")]
    InvalidTime { hour: u32, minute: u32, second: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        assert_eq!(*integrity_string(b"@@").unwrap().1, b"");
        assert_eq!(*integrity_string(b"@foo@").unwrap().1, b"foo");
        assert_eq!(*integrity_string(b"@foo\x0cbar@").unwrap().1, b"foo\x0cbar");

        assert_eq!(string(b"@foo bar@").unwrap().1 .0, b"foo bar");
        assert_eq!(string(b"@foo@@bar@").unwrap().1 .0, b"foo@bar");
    }

    #[test]
    fn test_date() {
        // Straight up parse errors.
        assert_parse_error(b"", date);
        assert_parse_error(b"not.a.digit.oh.my.word", date);
        assert_parse_error(b".....", date);

        // Range errors.
        assert_parse_error(&build_date_input(2021, 0, 1, 0, 0, 0), date);
        assert_parse_error(&build_date_input(2021, 13, 1, 0, 0, 0), date);
        assert_parse_error(&build_date_input(2021, 1, 0, 0, 0, 0), date);
        assert_parse_error(&build_date_input(2021, 1, 32, 0, 0, 0), date);
        assert_parse_error(&build_date_input(2021, 1, 1, 24, 0, 0), date);
        assert_parse_error(&build_date_input(2021, 1, 1, 0, 60, 0), date);
        assert_parse_error(&build_date_input(2021, 1, 1, 0, 0, 61), date);

        // Actually valid inputs.
        assert_eq!(
            date(b"2021.08.11.19.08.27").unwrap().1,
            DateTime::parse_from_rfc3339("2021-08-11T19:08:27+00:00")
                .unwrap()
                .into(),
        );
        assert_eq!(
            date(b"98.08.11.19.08.27").unwrap().1,
            DateTime::parse_from_rfc3339("1998-08-11T19:08:27+00:00")
                .unwrap()
                .into(),
        );
    }

    fn assert_parse_error<F, T>(input: &[u8], f: F)
    where
        F: Fn(&[u8]) -> IResult<&[u8], T>,
    {
        assert!(f(input).is_err());
    }

    fn build_date_input(
        year: u32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
    ) -> Vec<u8> {
        format!("{}.{}.{}.{}.{}.{}", year, month, day, hour, minute, second).into_bytes()
    }
}
