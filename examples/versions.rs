use std::io::{self, BufReader, Read, Write};

use comma_v::Num;
use rcs_ed::{File, Script};

fn main() -> anyhow::Result<()> {
    let mut buf = Vec::new();
    BufReader::new(io::stdin()).read_to_end(&mut buf)?;

    let cv = comma_v::parse(&buf)?;

    // Start at the head and work our way down.
    let (mut num, mut delta_text) = cv.head_delta_text().unwrap();
    let mut file = File::new(delta_text.text.as_cursor())?;
    write_delta(num, &file)?;

    // For now, we'll ignore branches.
    loop {
        match cv.delta.get(num) {
            Some(delta) => match &delta.next {
                Some(next) => {
                    num = next;
                }
                None => {
                    break;
                }
            },
            None => {
                anyhow::bail!(
                    "cannot find delta {}, even though we got it from somewhere!",
                    num
                )
            }
        }

        delta_text = match cv.delta_text.get(num) {
            Some(dt) => dt,
            None => anyhow::bail!("cannot find delta text {}", num),
        };

        let commands = Script::parse(delta_text.text.as_cursor()).into_command_list()?;
        file.apply_in_place(&commands)?;
        write_delta(num, &file)?;
    }

    Ok(())
}

fn write_delta(num: &Num, file: &File) -> anyhow::Result<()> {
    let mut stdout = io::stdout();
    println!("@{}", num);
    stdout.write_all(&file.as_bytes())?;
    println!("\n-=-=-=-=-=-=-\n");

    Ok(())
}
