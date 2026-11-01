use anyhow::{Context, Result};
use lopdf::Document;
use std::path::{Path, PathBuf};
use std::thread;

use crate::model::Bookmark;
use crate::naming::numbered_name;
use crate::pdf;

/// Return the first TOC index outside the bookmark's complete subtree.
pub fn get_subtree_end(toc: &[Bookmark], index: usize) -> usize {
    let level = toc[index].level;
    (index + 1..toc.len())
        .find(|&i| toc[i].level <= level)
        .unwrap_or(toc.len())
}

/// Return only direct children of a bookmark.
pub fn get_immediate_children(toc: &[Bookmark], index: usize, subtree_end: usize) -> Vec<usize> {
    let child_level = toc[index].level + 1;
    (index + 1..subtree_end)
        .filter(|&i| toc[i].level == child_level)
        .collect()
}

/// Return the first page range owned by a parent before its first child.
pub fn get_parent_owned_range(
    toc: &[Bookmark],
    index: usize,
    subtree_end: usize,
) -> Option<(usize, usize)> {
    let children = get_immediate_children(toc, index, subtree_end);
    let first_child = *children.first()?;
    let start_page = toc[index].page.checked_sub(1)?;
    let first_child_page = toc[first_child].page.checked_sub(1)?;

    if first_child_page <= start_page {
        return None;
    }

    Some((start_page, first_child_page - 1))
}

/// Determine the complete page range represented by a bookmark.
pub fn get_bookmark_page_range(
    toc: &[Bookmark],
    index: usize,
    document_page_count: usize,
) -> (usize, usize) {
    let subtree_end = get_subtree_end(toc, index);
    let start_page = toc[index].page.saturating_sub(1);

    let end_page = if subtree_end < toc.len() {
        toc[subtree_end].page.saturating_sub(2)
    } else {
        document_page_count.saturating_sub(1)
    };

    (start_page, end_page.max(start_page))
}

/// Whether this bookmark's subtree contains a bookmark at `target_level`.
pub fn reaches_target_level(toc: &[Bookmark], index: usize, target_level: usize) -> bool {
    let current_level = toc[index].level;

    if current_level == target_level {
        return true;
    }
    if current_level > target_level {
        return false;
    }

    let subtree_end = get_subtree_end(toc, index);
    toc[index + 1..subtree_end]
        .iter()
        .any(|bookmark| bookmark.level == target_level)
}

pub fn extract_bookmarks(
    document: &Document,
    toc: &[Bookmark],
    target_level: usize,
    output_root: &Path,
) -> Result<()> {
    let document_page_count = document.get_pages().len();
    let root_indices: Vec<usize> = toc
        .iter()
        .enumerate()
        .filter_map(|(index, bookmark)| (bookmark.level == 1).then_some(index))
        .collect();

    if root_indices.is_empty() {
        anyhow::bail!("No Level-1 bookmarks found.");
    }

    std::fs::create_dir_all(output_root).with_context(|| {
        format!(
            "Could not create output directory: {}",
            output_root.display()
        )
    })?;

    let mut jobs = Vec::new();
    if target_level == 1 {
        for (sequence, index) in root_indices.into_iter().enumerate() {
            queue_bookmark_as_pdf(
                toc,
                index,
                output_root,
                sequence + 1,
                document_page_count,
                &mut jobs,
            );
        }
    } else {
        // For target levels > 1, every top-level bookmark is a folder.
        for (root_sequence, root_index) in root_indices.into_iter().enumerate() {
            let root_folder =
                output_root.join(numbered_name(root_sequence + 1, &toc[root_index].title));
            std::fs::create_dir_all(&root_folder).with_context(|| {
                format!(
                    "Could not create output directory: {}",
                    root_folder.display()
                )
            })?;

            process_children(
                toc,
                root_index,
                target_level,
                &root_folder,
                document_page_count,
                &mut jobs,
            )?;
        }
    }

    extract_jobs_parallel(document, toc, &jobs)
}

struct ExtractionJob {
    bookmark_index: usize,
    start_page: usize,
    end_page: usize,
    output_path: PathBuf,
    preserve_toc: bool,
}

fn extract_jobs_parallel(
    document: &Document,
    toc: &[Bookmark],
    jobs: &[ExtractionJob],
) -> Result<()> {
    if jobs.is_empty() {
        return Ok(());
    }

    let worker_count = thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .min(jobs.len());
    let chunk_size = jobs.len().div_ceil(worker_count);

    let errors = thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .chunks(chunk_size)
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .filter_map(|job| {
                            pdf::extract_pdf(
                                document,
                                toc,
                                job.bookmark_index,
                                job.start_page,
                                job.end_page,
                                &job.output_path,
                                job.preserve_toc,
                            )
                            .err()
                            .map(|error| {
                                error.context(format!(
                                    "Failed to extract PDF: {}",
                                    job.output_path.display()
                                ))
                            })
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();

        handles
            .into_iter()
            .flat_map(|handle| {
                handle.join().unwrap_or_else(|_| {
                    vec![anyhow::anyhow!("An extraction worker thread panicked.")]
                })
            })
            .collect::<Vec<_>>()
    });

    if errors.is_empty() {
        Ok(())
    } else {
        let details = errors
            .iter()
            .map(|error| format!("{error:#}"))
            .collect::<Vec<_>>()
            .join("\n");
        anyhow::bail!("{} PDF extraction(s) failed:\n{details}", errors.len());
    }
}

fn process_children(
    toc: &[Bookmark],
    parent_index: usize,
    target_level: usize,
    output_dir: &Path,
    document_page_count: usize,
    jobs: &mut Vec<ExtractionJob>,
) -> Result<()> {
    let parent_subtree_end = get_subtree_end(toc, parent_index);
    let children = get_immediate_children(toc, parent_index, parent_subtree_end);

    if children.is_empty() {
        return Ok(());
    }

    let mut sequence = 1;

    // The parent's own introductory pages precede its first immediate child.
    if let Some((start_page, end_page)) =
        get_parent_owned_range(toc, parent_index, parent_subtree_end)
    {
        queue_parent_owned_pages(
            toc,
            parent_index,
            start_page,
            end_page,
            output_dir,
            sequence,
            jobs,
        );
        sequence += 1;
    }

    for child_index in children {
        let child_level = toc[child_index].level;
        let child_subtree_end = get_subtree_end(toc, child_index);
        let child_children = get_immediate_children(toc, child_index, child_subtree_end);

        // CASE A: The child is exactly the requested level: make it a PDF.
        if child_level == target_level {
            queue_bookmark_as_pdf(
                toc,
                child_index,
                output_dir,
                sequence,
                document_page_count,
                jobs,
            );
            sequence += 1;
            continue;
        }

        // CASE B: This branch never reaches the requested level. Flatten it
        // into its deepest real bookmarks instead of creating empty folders.
        if !reaches_target_level(toc, child_index, target_level) {
            if child_children.is_empty() {
                queue_bookmark_as_pdf(
                    toc,
                    child_index,
                    output_dir,
                    sequence,
                    document_page_count,
                    jobs,
                );
                sequence += 1;
            } else {
                let emitted = process_shallow_branch(
                    toc,
                    child_index,
                    output_dir,
                    sequence,
                    document_page_count,
                    jobs,
                )?;
                sequence += emitted;
            }
            continue;
        }

        // CASE C: The branch reaches the requested depth, so this bookmark
        // becomes a folder and its children are handled recursively.
        let folder_path = output_dir.join(numbered_name(sequence, &toc[child_index].title));
        std::fs::create_dir_all(&folder_path).with_context(|| {
            format!(
                "Could not create output directory: {}",
                folder_path.display()
            )
        })?;

        process_children(
            toc,
            child_index,
            target_level,
            &folder_path,
            document_page_count,
            jobs,
        )?;
        sequence += 1;
    }

    Ok(())
}

fn process_shallow_branch(
    toc: &[Bookmark],
    index: usize,
    output_dir: &Path,
    starting_sequence: usize,
    document_page_count: usize,
    jobs: &mut Vec<ExtractionJob>,
) -> Result<usize> {
    let subtree_end = get_subtree_end(toc, index);
    let children = get_immediate_children(toc, index, subtree_end);

    if children.is_empty() {
        queue_bookmark_as_pdf(
            toc,
            index,
            output_dir,
            starting_sequence,
            document_page_count,
            jobs,
        );
        return Ok(1);
    }

    let mut sequence = starting_sequence;
    for child_index in children {
        let child_subtree_end = get_subtree_end(toc, child_index);
        let child_children = get_immediate_children(toc, child_index, child_subtree_end);

        if child_children.is_empty() {
            queue_bookmark_as_pdf(
                toc,
                child_index,
                output_dir,
                sequence,
                document_page_count,
                jobs,
            );
            sequence += 1;
        } else {
            let emitted = process_shallow_branch(
                toc,
                child_index,
                output_dir,
                sequence,
                document_page_count,
                jobs,
            )?;
            sequence += emitted;
        }
    }

    Ok(sequence - starting_sequence)
}

fn queue_bookmark_as_pdf(
    toc: &[Bookmark],
    index: usize,
    output_dir: &Path,
    sequence: usize,
    document_page_count: usize,
    jobs: &mut Vec<ExtractionJob>,
) {
    let (start_page, end_page) = get_bookmark_page_range(toc, index, document_page_count);
    let filename = format!("{}.pdf", numbered_name(sequence, &toc[index].title));
    let output_path: PathBuf = output_dir.join(filename);

    jobs.push(ExtractionJob {
        bookmark_index: index,
        start_page,
        end_page,
        output_path,
        preserve_toc: true,
    });
}

fn queue_parent_owned_pages(
    toc: &[Bookmark],
    index: usize,
    start_page: usize,
    end_page: usize,
    output_dir: &Path,
    sequence: usize,
    jobs: &mut Vec<ExtractionJob>,
) {
    let filename = format!("{}.pdf", numbered_name(sequence, &toc[index].title));
    let output_path = output_dir.join(filename);

    jobs.push(ExtractionJob {
        bookmark_index: index,
        start_page,
        end_page,
        output_path,
        preserve_toc: false,
    });
}

#[cfg(test)]
mod tests {
    use super::{
        get_bookmark_page_range, get_immediate_children, get_parent_owned_range, get_subtree_end,
        reaches_target_level,
    };
    use crate::model::Bookmark;

    fn sample_toc() -> Vec<Bookmark> {
        vec![
            Bookmark {
                level: 1,
                title: "Part One".into(),
                page: 1,
            },
            Bookmark {
                level: 2,
                title: "Introduction".into(),
                page: 2,
            },
            Bookmark {
                level: 3,
                title: "Section A".into(),
                page: 4,
            },
            Bookmark {
                level: 3,
                title: "Section B".into(),
                page: 6,
            },
            Bookmark {
                level: 2,
                title: "Chapter Two".into(),
                page: 8,
            },
            Bookmark {
                level: 1,
                title: "Part Two".into(),
                page: 10,
            },
        ]
    }

    #[test]
    fn identifies_subtrees_and_direct_children() {
        let toc = sample_toc();
        assert_eq!(get_subtree_end(&toc, 0), 5);
        assert_eq!(get_immediate_children(&toc, 0, 5), vec![1, 4]);
        assert_eq!(get_immediate_children(&toc, 1, 4), vec![2, 3]);
    }

    #[test]
    fn calculates_bookmark_and_parent_owned_ranges() {
        let toc = sample_toc();
        assert_eq!(get_bookmark_page_range(&toc, 0, 12), (0, 8));
        assert_eq!(get_parent_owned_range(&toc, 0, 5), Some((0, 0)));
        assert_eq!(get_parent_owned_range(&toc, 1, 4), Some((1, 2)));
    }

    #[test]
    fn detects_target_depth() {
        let toc = sample_toc();
        assert!(reaches_target_level(&toc, 0, 3));
        assert!(!reaches_target_level(&toc, 1, 4));
        assert!(reaches_target_level(&toc, 2, 3));
    }
}
