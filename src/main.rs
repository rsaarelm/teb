use std::{fs, io, path::PathBuf};

use anyhow::bail;
use clap::Parser;
use teb::{Vm, parse};

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Unformatted input table source, stdin by default
    #[arg()]
    input: Option<PathBuf>,

    /// Formatted output table destination, stdout by default
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Clear all spreadsheet output cells.
    #[arg(long)]
    clear_outputs: bool,

    /// Treat all cells as text instead of numbers.
    #[arg(long)]
    no_number_parsing: bool,

    /// Convert input file in-place, input must be a file path not stdin, and
    /// output will be ignored.
    #[arg(short, long)]
    in_place: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let input = match &cli.input {
        Some(path) => fs::read_to_string(path)?,
        None => io::read_to_string(io::stdin())?,
    };

    let mut tables = parse::tables(&input, !cli.no_number_parsing)?;

    let mut vm = Vm::default();
    let mut output = String::new();
    let len = tables.len();
    for (i, table) in tables.iter_mut().enumerate() {
        table.eval(&mut vm)?;
        output.push_str(&table.to_string());
        if i < len - 1 {
            output.push('\n');
        }
    }

    // TODO: Construct spreadsheets from tables, evaluate formulas, update
    // tables.

    if cli.in_place {
        if let Some(path) = &cli.input {
            fs::write(path, output)?;
        } else {
            bail!("In-place conversion requires an input file path, not stdin.");
        }
    } else if let Some(path) = &cli.output {
        fs::write(path, output)?;
    } else {
        print!("{output}");
    }

    Ok(())
}
