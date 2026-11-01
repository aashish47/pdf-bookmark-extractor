use anyhow::{bail, Context, Result};
use lopdf::{Bookmark as PdfBookmark, Document, Object, ObjectId};
use std::collections::HashSet;
use std::path::Path;

use crate::model::Bookmark;

const INHERITABLE_PAGE_ATTRIBUTES: [(&str, &[u8]); 4] = [
    ("Resources", b"Resources"),
    ("MediaBox", b"MediaBox"),
    ("CropBox", b"CropBox"),
    ("Rotate", b"Rotate"),
];

pub fn open_pdf(path: &Path) -> Result<Document> {
    Document::load(path).with_context(|| format!("Could not open PDF: {}", path.display()))
}

/// Read the source PDF's outline as a flat list. lopdf reports 1-based pages.
pub fn read_bookmarks(document: &Document) -> Result<Vec<Bookmark>> {
    let toc = match document.get_toc() {
        Ok(toc) => toc,
        Err(lopdf::Error::NoOutline) => return Ok(Vec::new()),
        Err(error) => return Err(error).context("Could not read the PDF bookmark outline"),
    };

    Ok(toc
        .toc
        .into_iter()
        .map(|entry| Bookmark {
            level: entry.level,
            title: entry.title,
            page: entry.page,
        })
        .collect())
}

/// Extract an inclusive, zero-based page range into a new PDF.
///
/// The source document is cloned, its page tree is rebuilt to contain only the
/// requested pages, and unreachable objects are discarded before saving.
pub fn extract_pdf(
    source: &Document,
    toc: &[Bookmark],
    bookmark_index: usize,
    start_page: usize,
    end_page: usize,
    output_path: &Path,
    preserve_toc: bool,
) -> Result<()> {
    let source_pages = source.get_pages();
    let page_count = source_pages.len();
    if page_count == 0 {
        bail!("The input PDF contains no pages.");
    }

    let start_page = start_page.min(page_count - 1);
    let end_page = end_page.max(start_page).min(page_count - 1);

    let selected_page_ids: Vec<ObjectId> = (start_page..=end_page)
        .map(|zero_based| {
            let page_number = u32::try_from(zero_based + 1)
                .context("PDF page number exceeds lopdf's supported range")?;
            source_pages
                .get(&page_number)
                .copied()
                .with_context(|| format!("Could not find source PDF page {page_number}"))
        })
        .collect::<Result<_>>()?;

    let mut extracted = source.clone();
    rebuild_page_tree(&mut extracted, source, &selected_page_ids)?;

    // The source outline points at the original page tree. Remove it and add
    // only the bookmark subtree that belongs to this extracted document.
    extracted
        .delete_outlines()
        .context("Could not remove the source PDF outline")?;

    if preserve_toc {
        let subtree_end = hierarchy_subtree_end(toc, bookmark_index);
        let subtree = &toc[bookmark_index..subtree_end];
        install_outline(&mut extracted, subtree, toc[bookmark_index].level, start_page)?;
    }

    discard_unreachable_objects(&mut extracted);

    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Could not create output directory: {}", parent.display()))?;
        }
    }

    extracted
        .save(output_path)
        .with_context(|| format!("Could not save extracted PDF: {}", output_path.display()))?;

    println!("Saved: {}", output_path.display());
    Ok(())
}

fn hierarchy_subtree_end(toc: &[Bookmark], index: usize) -> usize {
    let level = toc[index].level;
    (index + 1..toc.len())
        .find(|&i| toc[i].level <= level)
        .unwrap_or(toc.len())
}

fn rebuild_page_tree(
    output: &mut Document,
    source: &Document,
    selected_page_ids: &[ObjectId],
) -> Result<()> {
    let pages_root_id = output
        .catalog()
        .context("Could not read the PDF catalog")?
        .get(b"Pages")
        .context("PDF catalog has no Pages entry")?
        .as_reference()
        .context("PDF catalog Pages entry is not an indirect reference")?;

    // A page can inherit these values from an intermediate /Pages node.
    // Since the extracted pages become direct children of the root /Pages
    // node, copy inherited values onto each page before changing its parent.
    for &page_id in selected_page_ids {
        let inherited: Vec<(&'static str, Object)> = INHERITABLE_PAGE_ATTRIBUTES
            .iter()
            .filter_map(|(name, key)| {
                let value = inherited_page_attribute(source, page_id, key)?;
                Some((*name, value))
            })
            .collect();

        let page = output
            .get_dictionary_mut(page_id)
            .with_context(|| format!("Could not read page object {page_id:?}"))?;

        for (key, value) in inherited {
            if page.get(key.as_bytes()).is_err() {
                page.set(key, value);
            }
        }
        page.set("Parent", Object::Reference(pages_root_id));
    }

    let root = output
        .get_dictionary_mut(pages_root_id)
        .context("Could not update the PDF page tree root")?;
    root.set(
        "Kids",
        Object::Array(
            selected_page_ids
                .iter()
                .copied()
                .map(Object::Reference)
                .collect(),
        ),
    );
    root.set("Count", Object::Integer(selected_page_ids.len() as i64));

    Ok(())
}

fn inherited_page_attribute(
    document: &Document,
    page_id: ObjectId,
    key: &[u8],
) -> Option<Object> {
    let mut current_id = page_id;
    let mut visited = HashSet::new();

    loop {
        if !visited.insert(current_id) {
            return None;
        }

        let dictionary = document.get_dictionary(current_id).ok()?;
        if let Ok(value) = dictionary.get(key) {
            return Some(value.clone());
        }

        current_id = dictionary.get(b"Parent").ok()?.as_reference().ok()?;
    }
}

fn install_outline(
    document: &mut Document,
    subtree: &[Bookmark],
    base_level: usize,
    start_page_zero_based: usize,
) -> Result<()> {
    let pages = document.get_pages();
    let page_count = pages.len();
    if page_count == 0 || subtree.is_empty() {
        return Ok(());
    }

    let mut parent_stack: Vec<(usize, u32)> = Vec::new();

    for entry in subtree {
        let level = entry.level.saturating_sub(base_level) + 1;

        while parent_stack
            .last()
            .is_some_and(|(parent_level, _)| *parent_level >= level)
        {
            parent_stack.pop();
        }

        let parent_id = parent_stack.last().map(|(_, id)| *id);
        if level > 1 && parent_id.is_none() {
            bail!(
                "Cannot rebuild bookmark '{}': its parent is missing.",
                entry.title
            );
        }

        let relative_page = entry
            .page
            .saturating_sub(start_page_zero_based)
            .clamp(1, page_count);
        let relative_page = u32::try_from(relative_page)
            .context("Extracted PDF page number exceeds lopdf's supported range")?;
        let page_id = pages
            .get(&relative_page)
            .copied()
            .with_context(|| format!("Extracted PDF has no page {relative_page}"))?;

        let bookmark = PdfBookmark::new(entry.title.clone(), [0.0, 0.0, 0.0], 0, page_id);
        let bookmark_id = document.add_bookmark(bookmark, parent_id);
        parent_stack.push((level, bookmark_id));
    }

    if let Some(outline_root) = document.build_outline() {
        document
            .catalog_mut()
            .context("Could not update the extracted PDF catalog")?
            .set("Outlines", Object::Reference(outline_root));
    }

    Ok(())
}

/// Remove objects no longer reachable from the trailer after the page tree and
/// outline have been rewritten. This prevents extracted PDFs from retaining
/// every original page as unused data.
fn discard_unreachable_objects(document: &mut Document) {
    let reachable: HashSet<ObjectId> = document
        .traverse_objects(|_| {})
        .into_iter()
        .collect();
    document.objects.retain(|id, _| reachable.contains(id));
}
