use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

use rcs_ed::{File, Script};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(parse(from_os_str))]
    patch: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();

    let commands = Script::parse(fs::File::open(opt.patch)?).into_command_list()?;
    let file = File::new(io::stdin())?;

    let mut stdout = io::stdout();
    for line in file.apply(&commands)? {
        stdout.write_all(&line)?;
        stdout.write_all(b"\n")?;
    }

    Ok(())
}
