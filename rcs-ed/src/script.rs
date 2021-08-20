use std::{
    io::{BufRead, BufReader, Read, Split},
    iter::Enumerate,
};
use thiserror::Error;

use crate::command;

pub struct Script<R: Read> {
    reader: Enumerate<Split<BufReader<R>>>,
}

/// Command is the external representation of an ed command, including its
/// payload, if any.
#[derive(Debug)]
pub enum Command {
    Add {
        position: usize,
        content: Vec<Vec<u8>>,
    },
    Delete {
        position: usize,
        lines: usize,
    },
}

pub type CommandList = Vec<Command>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("command parsing error on line {line}: {error}")]
    Command {
        #[source]
        error: command::Error,
        line: usize,
    },

    #[error("unexpected end of file: wanted {want} line(s) and only got {have}")]
    EndOfFile { have: usize, want: usize },

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl<R: Read> Script<R> {
    pub fn parse(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader).split(b'\n').enumerate(),
        }
    }

    pub fn into_command_list(self) -> Result<CommandList, Error> {
        self.into_iter().collect()
    }
}

impl<R: Read> Iterator for Script<R> {
    type Item = Result<Command, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        // We need to get the next line, which should be a command line.
        match self.reader.next() {
            Some((line, Ok(raw))) => match command::Command::parse(&raw) {
                // We got an Add command: this means that we need to read the
                // next chunk of lines to get the actual content to be added.
                Ok(command::Command::Add { position, lines }) => {
                    match (&mut self.reader)
                        .take(lines)
                        .map(|(_line, content)| content)
                        .collect::<Result<Vec<Vec<u8>>, std::io::Error>>()
                    {
                        Ok(content) if content.len() == lines => {
                            Some(Ok(Command::Add { position, content }))
                        }
                        Ok(content) if content.len() < lines => Some(Err(Error::EndOfFile {
                            have: content.len(),
                            want: lines,
                        })),
                        Ok(content) => panic!(
                            "read {} lines when only expected a maximum of {}",
                            content.len(),
                            lines
                        ),
                        Err(e) => Some(Err(Error::Io(e))),
                    }
                }
                // We got a Delete command, which is simpler: we just need to
                // return the position and lines to be deleted.
                Ok(command::Command::Delete { position, lines }) => {
                    Some(Ok(Command::Delete { position, lines }))
                }
                // The command couldn't be parsed, so let's return the command
                // error annotated with the 1-indexed line number.
                Err(e) => Some(Err(Error::Command {
                    error: e,
                    line: line + 1,
                })),
            },
            Some((_line, Err(e))) => Some(Err(e.into())),
            None => None,
        }
    }
}
