use crate::{Command, Error, Identity, Mark};

/// A `tag` fast-import command.
#[derive(Debug)]
pub struct Tag {
    name: String,
    from: Mark,
    tagger: Identity,
    message: String,
}

impl Tag {
    /// Constructs a new tag from the given mark and metadata.
    pub fn new(name: String, from: Mark, tagger: Identity, message: String) -> Self {
        Self {
            name,
            from,
            tagger,
            message,
        }
    }
}

impl Command for Tag {
    fn write(&self, writer: &mut impl std::io::Write, mark: Mark) -> Result<(), Error> {
        Ok(writeln!(
            writer,
            "tag {}\nmark {}\nfrom {}\ntagger {}\ndata {}\n{}",
            self.name,
            mark,
            self.from,
            self.tagger,
            self.message.len(),
            self.message
        )?)
    }
}
