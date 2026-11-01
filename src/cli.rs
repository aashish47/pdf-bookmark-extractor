use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "pdf-bookmark-extractor",
    version,
    about = "Extract PDF bookmark branches into numbered folders and PDF files"
)]
pub struct Args {
    /// Input PDF file
    pub pdf: PathBuf,

    /// Bookmark depth to extract as PDF files (defaults to 1)
    #[arg(default_value_t = 1)]
    pub level: usize,
}
