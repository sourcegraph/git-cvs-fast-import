use derive_more::{Deref, From, Into};
use eq_macro::EqU8;
use std::{collections::HashMap, fmt::Display, io::Cursor, time::SystemTime};

#[derive(Debug, Clone)]
pub struct File {
    pub admin: Admin,
    pub delta: HashMap<Num, Delta>,
    pub desc: Desc,
    pub delta_text: HashMap<Num, DeltaText>,
}

impl File {
    pub fn head_delta(&self) -> Option<(&Num, &Delta)> {
        if let Some(head) = &self.admin.head {
            self.delta.get(head).map(|delta| (head, delta))
        } else {
            None
        }
    }

    pub fn head_delta_text(&self) -> Option<(&Num, &DeltaText)> {
        if let Some(head) = &self.admin.head {
            self.delta_text
                .get(head)
                .map(|delta_text| (head, delta_text))
        } else {
            None
        }
    }

    pub fn head(&self) -> Option<&Num> {
        self.admin.head.as_ref()
    }

    pub fn revision(&self, revision: &Num) -> Option<(&Delta, &DeltaText)> {
        if let Some(delta) = self.delta.get(revision) {
            if let Some(delta_text) = self.delta_text.get(revision) {
                return Some((delta, delta_text));
            }
        }

        None
    }
}

#[derive(Debug, Clone)]
pub struct Admin {
    pub head: Option<Num>,
    pub branch: Option<Num>,
    pub access: Vec<Id>,
    pub symbols: HashMap<Sym, Num>,
    pub locks: HashMap<Id, Num>,
    pub strict: bool,
    pub integrity: Option<IntString>,
    pub comment: Option<VString>,
    pub expand: Option<VString>,
}

#[derive(Debug, Clone)]
pub struct Delta {
    pub date: SystemTime,
    pub author: Id,
    pub state: Option<Id>,
    pub branches: Vec<Num>,
    pub next: Option<Num>,
    pub commit_id: Option<Sym>,
}

pub type Desc = VString;

#[derive(Debug, Clone)]
pub struct DeltaText {
    pub log: VString,
    pub text: VString,
}

#[derive(Debug, Clone, PartialEq, Eq, EqU8, Deref, From, Into, Hash)]
pub struct Num(pub Vec<u8>);

impl Display for Num {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.0))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, EqU8, Deref, From, Into, Hash)]
pub struct Id(pub Vec<u8>);

#[derive(Debug, Clone, PartialEq, Eq, EqU8, Deref, From, Into, Hash)]
pub struct Sym(pub Vec<u8>);

#[derive(Debug, Clone, PartialEq, Eq, EqU8, Deref, From, Into, Hash)]
pub struct VString(pub Vec<u8>);

impl VString {
    pub fn as_cursor(&self) -> Cursor<&Vec<u8>> {
        Cursor::new(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, EqU8, Deref, From, Into, Hash)]
pub struct IntString(pub Vec<u8>);
