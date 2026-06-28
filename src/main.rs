use std::io;

fn main() -> anyhow::Result<()> {
    // Read from stdin and write to stdout.
    let input = io::read_to_string(io::stdin())?;
    // TODO: CLI options for parse numbers, wipe outputs.
    let mut table = teb::Table::new(&input, true)?;
    table.eval(false)?;
    print!("{table}");
    Ok(())
}
