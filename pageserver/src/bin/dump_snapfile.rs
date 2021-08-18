//
// Main entry point for the dump_snapfile executable
//
// A handy tool for debugging, that's all.

use pageserver::layered_repository::dump_snapfile_from_path;

use anyhow::Result;
use std::path::PathBuf;
use clap::{App, Arg};

fn main() -> Result<()> {
    let arg_matches = App::new("Zenith dump_snapfile utilityr")
        .about("Dump contents of one snapshot file, for debugging")
        .arg(Arg::with_name("path")
             .help("Path to file to dump")
             .required(true)
             .index(1))
        .get_matches();

    let path = PathBuf::from(arg_matches.value_of("path").unwrap());

    dump_snapfile_from_path(&path)?;

    Ok(())
}
