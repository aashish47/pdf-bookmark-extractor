# PDF Bookmark Extractor

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey.svg)]()

A fast, multithreaded command-line utility written in Rust to split large PDFs into organized, numbered folders and individual PDF files using the document's existing bookmark hierarchy (table of contents).

Unlike many PDF utilities, **PDF Bookmark Extractor** is built purely in Rust using [`lopdf`](https://crates.io/crates/lopdf)—requiring **no** MuPDF, `mupdf-sys`, C++ runtimes, or external C dependencies.

---

## ✨ Features

- **⚡ High-Performance Parallel Processing**: Extracts PDF segments concurrently using scoped threads (`std::thread::scope`) scaled to your available CPU cores.
- **🌲 Intelligent Hierarchy Splitting**: Configurable target bookmark depth (`level`) allowing you to split at parts, chapters, sections, or subsections.
- **🛡️ Adaptive Shallow Branch Handling**: If a branch is shallower than the requested target depth, the extractor flattens it to its deepest real bookmarks instead of creating useless empty folders.
- **📑 Preface & Parent Introduction Extraction**: Automatically detects and extracts introductory pages that exist directly under a parent bookmark before its first child section (e.g. `1_Part One.pdf` followed by chapter folders).
- **🔖 Preserves Subtree Outlines**: Extracted child PDFs keep their relevant bookmark outline subtree intact.
- **🎨 Attribute & Resource Inheritance**: Rebuilds the PDF page tree and correctly preserves inheritable attributes (`Resources`, `MediaBox`, `CropBox`, `Rotate`).
- **🧹 Cross-Platform Filename Sanitization**: Automatically cleans Windows and POSIX reserved characters in bookmark titles and prepends consistent numerical ordering (`1_...`, `2_...`).
- **📦 100% Native Rust**: Zero C/C++ bindings or system-level PDF library dependencies.

---

## 📋 Requirements

- **Rust**: 1.85 or later
- **Cargo**: Rust's package manager

---

## 🚀 Installation & Building

### Build from Source

Clone the repository and build the release binary:

```bash
git clone https://github.com/your-username/pdf-bookmark-extractor.git
cd pdf-bookmark-extractor
cargo build --release
```

The optimized binary will be located at:
- **Windows**: `target/release/pdf-bookmark-extractor.exe`
- **Linux / macOS**: `target/release/pdf-bookmark-extractor`

### Install Locally with Cargo

To install the binary globally into your Cargo bin path (`~/.cargo/bin`):

```bash
cargo install --path .
```

You can then run `pdf-bookmark-extractor` from any directory.

---

## 💻 Usage

```text
Usage: pdf-bookmark-extractor [OPTIONS] <PDF> [LEVEL]

Arguments:
  <PDF>    Path to the input PDF file
  [LEVEL]  Bookmark depth to extract as PDF files [default: 1]

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### Basic Commands

```bash
# Split at Level 1 bookmarks (default)
cargo run -- document.pdf

# Split at Level 2 bookmarks (e.g., chapters under parts)
cargo run -- document.pdf 2

# Split at Level 3 bookmarks (e.g., sections under chapters)
cargo run -- document.pdf 3
```

Using the compiled binary directly:

```bash
# Windows
target\release\pdf-bookmark-extractor.exe "My Book.pdf" 2

# Linux / macOS
./target/release/pdf-bookmark-extractor "My Book.pdf" 2
```

---

## 📂 Output Structure & Behavior

The tool generates an output folder in the current working directory named after the input PDF (e.g. `document.pdf` → `./document/`).

### Target Level 1 (`level = 1`)

Every top-level bookmark is extracted as an individual PDF inside the root output directory:

```text
My Book/
├── 1_Part One.pdf
├── 2_Part Two.pdf
└── 3_Appendix.pdf
```

### Target Level > 1 (e.g. `level = 2` or `3`)

- Top-level bookmarks (Level 1) become root subfolders.
- Intermediate levels become subfolders until the requested depth is reached.
- Bookmarks at the target level become individual PDF files.

```text
My Book/
├── 1_Part One/
│   ├── 1_Part One.pdf             <-- Parent introductory / preface pages
│   ├── 2_Chapter One/
│   │   ├── 1_Section A.pdf
│   │   └── 2_Section B.pdf
│   └── 3_Chapter Two/
│       └── 1_Section C.pdf
└── 2_Part Two/
    ├── 1_Chapter Three.pdf
    └── 2_Chapter Four.pdf
```

### Edge Case Handling

1. **Parent-Owned Introductory Pages**:
   If a parent bookmark contains pages before its first child bookmark starts (e.g., a Part divider with an intro), those pages are extracted as their own standalone PDF preceding the children.
2. **Shallow Branches**:
   If a branch does not reach the specified target depth (e.g., an "Appendix" with no sub-sections), the utility extracts the deepest existing bookmarks directly into the folder rather than generating empty directory trees.
3. **Subtree Outlines**:
   Extracted PDFs retain their nested sub-bookmarks so navigation within the extracted chapter or section remains functional.

---

## 🛠️ Development & Testing

Run unit tests:

```bash
cargo test
```

Check code formatting and run linter:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features
```

---

## 📄 License

This project is licensed under the [MIT License](LICENSE).
