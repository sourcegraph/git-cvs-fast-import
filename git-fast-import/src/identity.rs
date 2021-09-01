use std::{
    fmt::Display,
    time::{SystemTime, SystemTimeError},
};

#[derive(Debug)]
pub struct Identity {
    name: Option<String>,
    email: String,
    when: u64,
}

impl Identity {
    pub fn new(
        name: Option<String>,
        email: String,
        when: SystemTime,
    ) -> Result<Self, SystemTimeError> {
        Ok(Self {
            name,
            email,
            when: when.duration_since(SystemTime::UNIX_EPOCH)?.as_secs(),
        })
    }
}

impl Display for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(name) = &self.name {
            write!(f, "{} ", name)?;
        }
        write!(f, "<{}> {} +0000", self.email, self.when)
    }
}
