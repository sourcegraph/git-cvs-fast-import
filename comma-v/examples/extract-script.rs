use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
    str::FromStr,
};

use comma_v::Num;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(parse(from_os_str), help = "input ,v file")]
    file: PathBuf,

    #[structopt(help = "revision content to extract")]
    revision: String,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();

    let file = comma_v::parse(&fs::read(&opt.file)?)?;
    match file.delta_text.get(&Num::from_str(&opt.revision)?) {
        Some(dt) => {
            io::stdout().write_all(&dt.text.0)?;
        }
        None => {
            anyhow::bail!(
                "{}: cannot find revision {}",
                opt.file.to_string_lossy(),
                &opt.revision
            );
        }
    }

    Ok(())
}
