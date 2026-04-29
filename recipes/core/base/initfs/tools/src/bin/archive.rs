use std::path::Path;

use anyhow::Result;
use clap::{Arg, Command};

use redox_initfs_tools::{self as archive, Args, DEFAULT_MAX_SIZE};

fn main() -> Result<()> {
    let matches = Command::new("redox-initfs-ar")
        .about("create an initfs image from a directory")
        .version(clap::crate_version!())
        .author(clap::crate_authors!())
        // TODO: support non-utf8 paths (applies to other paths as well)
        .arg(
            Arg::new("SOURCE")
                .required(true)
                .help("Specify the source directory to build the image from."),
        )
        .arg(
            Arg::new("BOOTSTRAP_CODE")
                .required(true)
                .help("Specify the bootstrap ELF file to include in the image."),
        )
        .arg(
            Arg::new("OUTPUT")
                .required(true)
                .long("output")
                .short('o')
                .help("Specify the path of the new image file."),
        )
        .get_matches();

    env_logger::init();

    let source = matches
        .get_one::<String>("SOURCE")
        .expect("expected the required arg SOURCE to exist");

    let bootstrap_code = matches
        .get_one::<String>("BOOTSTRAP_CODE")
        .expect("expected the required arg BOOTSTRAP_CODE to exist");

    let destination = matches
        .get_one::<String>("OUTPUT")
        .expect("expected the required arg OUTPUT to exist");

    let args = Args {
        source: Path::new(source),
        bootstrap_code: Path::new(bootstrap_code),
        destination_path: Path::new(destination),
        max_size: DEFAULT_MAX_SIZE,
    };
    archive::archive(&args)
}
