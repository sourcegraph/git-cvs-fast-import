use std::io::{BufRead, BufReader, Read};
use thiserror::Error;

mod command;

mod script;
pub use script::{Command, CommandList, Script};

#[derive(Debug, Clone)]
pub struct File {
    lines: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
enum Line<'a> {
    Add(Vec<&'a Vec<Vec<u8>>>),
    Delete,
    Keep,
    Replace(Vec<&'a Vec<Vec<u8>>>),
}

impl File {
    pub fn new<R: Read>(reader: R) -> anyhow::Result<Self> {
        // In theory, you'd think BufReader::split() would be sufficient here,
        // but it doesn't allow you to distinguish between a file with a
        // trailing newline and one without. So, let's use read_until() to find
        // out what's really going on.

        let mut r = BufReader::new(reader);
        let mut lines = Vec::new();

        loop {
            let mut line = Vec::new();
            r.read_until(b'\n', &mut line)?;

            if line.len() == 0 {
                // Special case: last line of the file, and it's empty.
                lines.push(b"".to_vec());
                break;
            }

            if line[line.len() - 1] != b'\n' {
                // Also the last line of the file, but it's not empty.
                lines.push(line);
                break;
            }

            line.pop();
            lines.push(line);
        }

        Ok(Self { lines })
    }

    pub fn apply(&self, commands: &CommandList) -> anyhow::Result<Vec<Vec<u8>>> {
        let line_commands = calculate_line_commands(self.lines.len(), commands)?;

        let mut output = Vec::with_capacity(line_commands.output_capacity());
        output.extend(line_commands.prepend.into_iter());
        for (orig, line) in self.lines.iter().zip(line_commands.lines.into_iter()) {
            match line {
                Line::Add(contents) => {
                    output.push(orig.clone());
                    output.extend(contents.iter().flat_map(|content| content.iter()).cloned());
                }
                Line::Delete => {}
                Line::Keep => {
                    output.push(orig.clone());
                }
                Line::Replace(contents) => {
                    output.extend(contents.iter().flat_map(|content| content.iter()).cloned());
                }
            }
        }

        Ok(output)
    }

    pub fn apply_in_place(&mut self, commands: &CommandList) -> anyhow::Result<()> {
        let line_commands = calculate_line_commands(self.lines.len(), commands)?;

        let mut output = Vec::with_capacity(line_commands.output_capacity());
        output.extend(line_commands.prepend.into_iter());
        for (orig, line) in self.lines.drain(..).zip(line_commands.lines.into_iter()) {
            match line {
                Line::Add(contents) => {
                    output.push(orig);
                    output.extend(contents.iter().flat_map(|content| content.iter()).cloned());
                }
                Line::Delete => {}
                Line::Keep => {
                    output.push(orig);
                }
                Line::Replace(contents) => {
                    output.extend(contents.iter().flat_map(|content| content.iter()).cloned());
                }
            }
        }
        self.lines = output;

        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = &Vec<u8>> {
        self.lines.iter()
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        self.lines.join(&b'\n')
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.lines.join(&b'\n')
    }
}

struct LineCommands<'a> {
    lines: Vec<Line<'a>>,
    prepend: Vec<Vec<u8>>,
}

impl LineCommands<'_> {
    fn output_capacity(&self) -> usize {
        self.lines.len() + self.prepend.len()
    }
}

#[derive(Debug, Error)]
enum LineCommandError {
    #[error("multiple a0 commands were found, but a valid script can have only one")]
    ConflictingPrepends,
}

fn calculate_line_commands(
    n: usize,
    commands: &CommandList,
) -> Result<LineCommands, LineCommandError> {
    let mut line_commands = LineCommands {
        lines: vec![Line::Keep; n],
        prepend: Vec::new(),
    };

    for command in commands {
        match command {
            Command::Add { position, content } if *position > 0 => {
                match &mut line_commands.lines[position - 1] {
                    Line::Add(commands) => {
                        // FIXME: I don't really know if this is the right
                        // behaviour when there are multiple a commands on the
                        // same line. Shrug.
                        commands.push(content);
                    }
                    Line::Delete => {
                        line_commands.lines[position - 1] = Line::Replace(vec![content]);
                    }
                    Line::Keep => {
                        line_commands.lines[position - 1] = Line::Add(vec![content]);
                    }
                    Line::Replace(commands) => {
                        commands.push(content);
                    }
                }
            }
            Command::Add {
                position: _,
                content,
            } => {
                // Special case: insert at the start of the commands.
                if line_commands.prepend.len() > 0 {
                    return Err(LineCommandError::ConflictingPrepends);
                }

                line_commands.prepend.extend(content.iter().cloned());
            }
            Command::Delete { position, lines } => {
                line_commands.lines.splice(
                    position - 1..position + lines - 1,
                    vec![Line::Delete; *lines],
                );
            }
        }
    }

    Ok(line_commands)
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::{Path, PathBuf},
    };

    use super::*;

    #[test]
    fn test_apply() {
        assert_eq!(
            File::new(include_bytes!("fixtures/lao").as_ref())
                .unwrap()
                .apply(
                    &Script::parse(include_bytes!("fixtures/script.ed").as_ref())
                        .into_command_list()
                        .unwrap()
                )
                .unwrap()
                .join(&b'\n'),
            include_bytes!("fixtures/tzu")
        );
    }

    #[test]
    fn test_apply_in_place() {
        let mut file = File::new(include_bytes!("fixtures/lao").as_ref()).unwrap();

        file.apply_in_place(
            &Script::parse(include_bytes!("fixtures/script.ed").as_ref())
                .into_command_list()
                .unwrap(),
        )
        .unwrap();

        assert_eq!(file.into_bytes(), include_bytes!("fixtures/tzu"));
    }

    #[test]
    fn test_add_first_line() {
        let mut file = File::new(include_bytes!("fixtures/a0/1.15").as_ref()).unwrap();

        for i in (1..15).rev() {
            // Read and apply the script.
            file.apply_in_place(
                &Script::parse(fs::File::open(fixture_path(format!("a0/1.{}.ed", i))).unwrap())
                    .into_command_list()
                    .unwrap(),
            )
            .unwrap();

            // Compare to the expected output.
            let expected = fs::read(fixture_path(format!("a0/1.{}", i))).unwrap();
            assert_eq!(&file.as_bytes(), &expected);
        }
    }

    // We can't always hardcode the path for fixtures, so this will resolve them
    // at runtime.
    fn fixture_path<P>(path: P) -> PathBuf
    where
        P: AsRef<Path>,
    {
        let mut buf = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        buf.extend(["src", "fixtures"]);
        buf.push(path);

        buf
    }
}
