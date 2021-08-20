use nom::{
    branch::alt,
    bytes::complete::{tag, take_till1, take_while, take_while1},
    combinator::{map, value},
    multi::fold_many0,
    sequence::delimited,
    IResult,
};

use super::char::*;
use crate::types;

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
    take_while1(|c| c == b'.' || (c >= b'0' && c <= b'9'))(input)
}

pub(super) fn date(input: &[u8]) -> IResult<&[u8], types::Date> {
    map(numlike, |bytes| types::Date(Vec::from(bytes)))(input)
}

pub(super) fn num(input: &[u8]) -> IResult<&[u8], types::Num> {
    map(numlike, |bytes| types::Num(Vec::from(bytes)))(input)
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
                Vec::new(),
                |mut v, fragment| {
                    v.extend_from_slice(fragment);
                    v
                },
            ),
            tag(b"@"),
        ),
        |bytes| types::VString(bytes),
    )(input)
}

pub(super) fn sym(input: &[u8]) -> IResult<&[u8], types::Sym> {
    map(take_while(is_idchar), |bytes| types::Sym(Vec::from(bytes)))(input)
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
}
