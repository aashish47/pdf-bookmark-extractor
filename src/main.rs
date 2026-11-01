fn main() {
    if let Err(error) = pdf_bookmark_extractor::app::run() {
        eprintln!("\nERROR: {error:#}");
        std::process::exit(1);
    }
}
