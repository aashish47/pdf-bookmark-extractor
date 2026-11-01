/// One entry from the PDF's flattened table of contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bookmark {
    /// Original outline depth, starting at one.
    pub level: usize,
    pub title: String,
    /// Original PDF page number, starting at one.
    pub page: usize,
}
