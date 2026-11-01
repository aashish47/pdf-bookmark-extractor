use anyhow::{bail, Context, Result};
use clap::Parser;
use std::path::PathBuf;

use crate::{cli::Args, hierarchy, naming, pdf};

pub fn run() -> Result<()> {
    let args = Args::parse();
    if !args.pdf.is_file() {
        bail!("File not found: {}", args.pdf.display());
    }
    if args.level < 1 {
        bail!("Bookmark level must be >= 1.");
    }

    let base_name = args
        .pdf
        .file_stem()
        .context("Input path does not have a valid file name")?
        .to_string_lossy();
    let output_root = PathBuf::from(naming::output_directory_name(base_name.as_ref()));

    println!("Input PDF : {}", args.pdf.display());
    println!("Target level: {}", args.level);

    let document = pdf::open_pdf(&args.pdf)?;
    let toc = pdf::read_bookmarks(&document)?;

    if toc.is_empty() {
        bail!("No usable bookmarks found in this PDF.");
    }

    let page_count = document.get_pages().len();
    println!("Pages     : {page_count}");
    println!("Bookmarks : {}", toc.len());
    println!("Output    : {}", output_root.display());

    hierarchy::extract_bookmarks(&document, &toc, args.level, &output_root)?;

    println!("\nDone! Output: '{}'", output_root.display());
    Ok(())
}

