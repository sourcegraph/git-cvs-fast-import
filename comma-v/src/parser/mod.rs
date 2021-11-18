use std::collections::HashMap;

use nom::{
    branch::permutation,
    bytes::complete::tag,
    character::complete::{multispace0, multispace1},
    combinator::{map, opt},
    multi::{fold_many0, many0},
    sequence::{delimited, preceded, separated_pair, terminated, tuple},
    IResult,
};

use crate::{num, types};

mod char;

mod scalar;
use self::scalar::*;

pub(crate) fn file(input: &[u8]) -> IResult<&[u8], types::File> {
    map(
        tuple((
            delimited(multispace0, admin, multispace0),
            many0(terminated(delta, multispace0)),
            terminated(desc, multispace0),
            many0(terminated(delta_text, multispace0)),
        )),
        |(admin, delta, desc, delta_text)| types::File {
            admin,
            delta: delta.into_iter().collect(),
            desc,
            delta_text: delta_text.into_iter().collect(),
        },
    )(input)
}

fn admin(input: &[u8]) -> IResult<&[u8], types::Admin> {
    map(
        permutation((
            delimited(
                tuple((tag(b"head"), multispace1)),
                opt(num),
                tuple((multispace0, tag(b";"), multispace0)),
            ),
            map(
                opt(delimited(
                    tuple((tag(b"branch"), multispace1)),
                    opt(num),
                    tuple((multispace0, tag(b";"), multispace0)),
                )),
                |branch| branch.map(|b| b.unwrap()),
            ),
            delimited(
                tag(b"access"),
                many0(preceded(multispace1, id)),
                tuple((multispace0, tag(b";"), multispace0)),
            ),
            delimited(
                tag(b"symbols"),
                fold_many0(
                    separated_pair(
                        delimited(multispace0, sym, multispace0),
                        tag(b":"),
                        delimited(multispace0, num, multispace0),
                    ),
                    HashMap::new,
                    |mut acc, (k, v)| {
                        acc.insert(k, v);
                        acc
                    },
                ),
                tuple((multispace0, tag(b";"), multispace0)),
            ),
            delimited(
                tag(b"locks"),
                fold_many0(
                    separated_pair(
                        delimited(multispace0, id, multispace0),
                        tag(b":"),
                        delimited(multispace0, num, multispace0),
                    ),
                    HashMap::new,
                    |mut acc, (k, v)| {
                        acc.insert(k, v);
                        acc
                    },
                ),
                tuple((multispace0, tag(b";"), multispace0)),
            ),
            map(
                opt(tuple((tag(b"strict"), multispace0, tag(b";"), multispace0))),
                |strict| strict.is_some(),
            ),
            opt(delimited(
                tuple((tag(b"integrity"), multispace1)),
                integrity_string,
                tuple((multispace0, tag(b";"), multispace0)),
            )),
            opt(delimited(
                tuple((tag(b"comment"), multispace1)),
                string,
                tuple((multispace0, tag(b";"), multispace0)),
            )),
            opt(delimited(
                tuple((tag(b"expand"), multispace1)),
                string,
                tuple((multispace0, tag(b";"), multispace0)),
            )),
        )),
        |(head, branch, access, symbols, locks, strict, integrity, comment, expand)| types::Admin {
            head,
            branch,
            access,
            symbols,
            locks,
            strict,
            integrity,
            comment,
            expand,
        },
    )(input)
}

fn delta(input: &[u8]) -> IResult<&[u8], (num::Num, types::Delta)> {
    map(
        tuple((
            terminated(num, multispace1),
            permutation((
                delimited(
                    tuple((tag(b"date"), multispace1)),
                    date,
                    tuple((multispace0, tag(b";"), multispace0)),
                ),
                delimited(
                    tuple((tag(b"author"), multispace1)),
                    id,
                    tuple((multispace0, tag(b";"), multispace0)),
                ),
                delimited(
                    tuple((tag(b"state"), multispace1)),
                    opt(id),
                    tuple((multispace0, tag(b";"), multispace0)),
                ),
                delimited(
                    tag(b"branches"),
                    many0(preceded(multispace1, num)),
                    tuple((multispace0, tag(b";"), multispace0)),
                ),
                delimited(
                    tuple((tag(b"next"), multispace1)),
                    opt(num),
                    tuple((multispace0, tag(b";"), multispace0)),
                ),
                opt(delimited(
                    tuple((tag(b"commitid"), multispace1)),
                    sym,
                    tuple((multispace0, tag(b";"), multispace0)),
                )),
            )),
        )),
        |(num, (date, author, state, branches, next, commit_id))| {
            (
                num,
                types::Delta {
                    date,
                    author,
                    state,
                    branches,
                    next,
                    commit_id,
                },
            )
        },
    )(input)
}

fn delta_text(input: &[u8]) -> IResult<&[u8], (num::Num, types::DeltaText)> {
    map(
        tuple((
            num,
            preceded(multispace1, tag(b"log")),
            delimited(multispace1, string, multispace1),
            tag(b"text"),
            preceded(multispace1, string),
        )),
        |(num, _, log, _, text)| (num, types::DeltaText { log, text }),
    )(input)
}

fn desc(input: &[u8]) -> IResult<&[u8], types::Desc> {
    preceded(tuple((tag(b"desc"), multispace1)), string)(input)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use chrono::DateTime;

    use crate::num::Num;

    use super::*;

    #[test]
    fn test_admin() -> anyhow::Result<()> {
        let have = admin(include_bytes!("fixtures/admin/input"))?.1;
        assert_eq!(have.head.unwrap().to_string(), "1.1");
        assert!(have.branch.is_none());
        assert_eq!(have.access.len(), 0);
        assert_eq!(have.symbols.len(), 0);
        assert_eq!(have.locks.len(), 0);
        assert!(have.strict);
        assert!(have.integrity.is_none());
        assert_eq!(*have.comment.unwrap(), b"# ");
        assert!(have.expand.is_none());

        Ok(())
    }

    #[test]
    fn test_delta() -> anyhow::Result<()> {
        let (num, have) = delta(include_bytes!("fixtures/delta/input"))?.1;
        assert_eq!(num.to_string(), "1.2");
        assert_eq!(
            have.date,
            DateTime::parse_from_rfc3339("2021-08-20T17:34:26+00:00")?.into(),
        );
        assert_eq!(*have.author, b"adam");
        assert_eq!(*have.state.unwrap(), b"Exp");
        assert_eq!(
            have.branches,
            vec![Num::from_str("1.2.2.1")?, Num::from_str("1.2.4.1")?]
        );
        assert_eq!(have.next.unwrap().to_string(), "1.1");
        assert!(have.commit_id.is_none());

        Ok(())
    }

    #[test]
    fn test_delta_text() -> anyhow::Result<()> {
        let (num, have) = delta_text(include_bytes!("fixtures/delta_text/input"))?.1;
        assert_eq!(num.to_string(), "1.1");
        assert_eq!(*have.log, include_bytes!("fixtures/delta_text/log"),);
        assert_eq!(*have.text, include_bytes!("fixtures/delta_text/text"),);

        let (num, have) = delta_text(b"1.2 log @@ text @@")?.1;
        assert_eq!(num.to_string(), "1.2");
        assert_eq!(*have.log, b"");
        assert_eq!(*have.text, b"");

        Ok(())
    }

    #[test]
    fn test_desc() -> anyhow::Result<()> {
        assert_eq!(*desc(b"desc @@")?.1, b"");
        assert_eq!(*desc(b"desc @foo@@bar@")?.1, b"foo@bar");
        assert_eq!(*desc(b"desc   @foo@@bar@")?.1, b"foo@bar");

        Ok(())
    }

    #[test]
    fn test_file() -> anyhow::Result<()> {
        let have = file(include_bytes!("fixtures/file/input"))?.1;

        // We'll just spot check.
        assert_eq!(have.admin.head.unwrap().to_string(), "1.4");

        assert_eq!(have.delta.len(), 4);
        assert_eq!(
            have.delta.get(&num::Num::from_str("1.4")?).unwrap().date,
            DateTime::parse_from_rfc3339("2021-08-11T19:08:27+00:00")?.into(),
        );

        assert_eq!(*have.desc, b"");

        assert_eq!(have.delta_text.len(), 4);
        assert_eq!(
            *have
                .delta_text
                .get(&num::Num::from_str("1.1")?)
                .unwrap()
                .text,
            b"d5 3\n"
        );

        Ok(())
    }
}
