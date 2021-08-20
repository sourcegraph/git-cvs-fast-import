use crate::{Command, Identity, Mark};

#[derive(Debug)]
pub struct Tag {
    name: String,
    from: Mark,
    tagger: Identity,
    message: String,
}

impl Tag {
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
    fn write(&self, writer: &mut impl std::io::Write, mark: Mark) -> anyhow::Result<()> {
        Ok(write!(
            writer,
            "tag {}\nmark {}\nfrom {}\ntagger {}\ndata {}\n{}\n",
            self.name,
            mark,
            self.from,
            self.tagger,
            self.message.len(),
            self.message
        )?)
    }
}
