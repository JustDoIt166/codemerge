use std::collections::BTreeMap;
use std::path::Path;

const FILE_RANKING_LIMIT: usize = 5;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessingStats {
    pub processed_files: usize,
    pub skipped_files: usize,
    pub total_chars: usize,
    pub total_tokens: usize,
    pub breakdown: Box<ProcessingBreakdown>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessingBreakdown {
    pub by_extension: Vec<GroupStats>,
    pub by_folder: Vec<GroupStats>,
    pub largest_files: Vec<FileStats>,
    pub smallest_files: Vec<FileStats>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupStats {
    pub name: Option<String>,
    pub file_count: usize,
    pub total_chars: usize,
    pub total_tokens: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileStats {
    pub path: String,
    pub chars: usize,
    pub tokens: usize,
}

pub(crate) fn build_processing_breakdown<'a>(
    files: impl IntoIterator<Item = (&'a str, usize, usize)>,
) -> ProcessingBreakdown {
    let mut extensions = BTreeMap::<Option<String>, GroupStats>::new();
    let mut folders = BTreeMap::<Option<String>, GroupStats>::new();
    let mut ranked_files = Vec::new();

    for (path, chars, tokens) in files {
        let path = path.replace('\\', "/");
        let file_path = Path::new(&path);
        let extension = file_path
            .extension()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .map(|value| format!(".{}", value.to_lowercase()));
        let folder = file_path.parent().and_then(|parent| {
            let parent = parent.to_string_lossy().replace('\\', "/");
            (!parent.is_empty() && parent != ".").then_some(parent)
        });

        add_group_value(&mut extensions, extension, chars, tokens);
        add_group_value(&mut folders, folder, chars, tokens);
        ranked_files.push(FileStats {
            path,
            chars,
            tokens,
        });
    }

    let mut largest_files = ranked_files.clone();
    largest_files.sort_by(|left, right| {
        right
            .chars
            .cmp(&left.chars)
            .then_with(|| left.path.cmp(&right.path))
    });
    largest_files.truncate(FILE_RANKING_LIMIT);

    ranked_files.sort_by(|left, right| {
        left.chars
            .cmp(&right.chars)
            .then_with(|| left.path.cmp(&right.path))
    });
    ranked_files.truncate(FILE_RANKING_LIMIT);

    ProcessingBreakdown {
        by_extension: extensions.into_values().collect(),
        by_folder: folders.into_values().collect(),
        largest_files,
        smallest_files: ranked_files,
    }
}

fn add_group_value(
    groups: &mut BTreeMap<Option<String>, GroupStats>,
    name: Option<String>,
    chars: usize,
    tokens: usize,
) {
    let group = groups.entry(name.clone()).or_insert_with(|| GroupStats {
        name,
        ..GroupStats::default()
    });
    group.file_count += 1;
    group.total_chars += chars;
    group.total_tokens += tokens;
}

#[cfg(test)]
mod tests {
    use super::{FileStats, GroupStats, build_processing_breakdown};

    #[test]
    fn groups_files_by_normalized_extension_and_direct_parent() {
        let breakdown = build_processing_breakdown([
            ("README", 10, 2),
            ("src/lib.RS", 20, 4),
            ("src/core/mod.rs", 30, 6),
            ("bundle.zip/src/main.rs", 40, 8),
            ("archive.tar.gz", 50, 10),
        ]);

        assert_eq!(
            breakdown.by_extension,
            vec![
                GroupStats {
                    name: None,
                    file_count: 1,
                    total_chars: 10,
                    total_tokens: 2,
                },
                GroupStats {
                    name: Some(".gz".into()),
                    file_count: 1,
                    total_chars: 50,
                    total_tokens: 10,
                },
                GroupStats {
                    name: Some(".rs".into()),
                    file_count: 3,
                    total_chars: 90,
                    total_tokens: 18,
                },
            ]
        );
        assert_eq!(
            breakdown.by_folder,
            vec![
                GroupStats {
                    name: None,
                    file_count: 2,
                    total_chars: 60,
                    total_tokens: 12,
                },
                GroupStats {
                    name: Some("bundle.zip/src".into()),
                    file_count: 1,
                    total_chars: 40,
                    total_tokens: 8,
                },
                GroupStats {
                    name: Some("src".into()),
                    file_count: 1,
                    total_chars: 20,
                    total_tokens: 4,
                },
                GroupStats {
                    name: Some("src/core".into()),
                    file_count: 1,
                    total_chars: 30,
                    total_tokens: 6,
                },
            ]
        );
    }

    #[test]
    fn ranks_largest_and_smallest_files_by_chars_with_stable_ties() {
        let breakdown = build_processing_breakdown([
            ("f.rs", 3, 1),
            ("e.rs", 2, 1),
            ("d.rs", 2, 1),
            ("c.rs", 1, 1),
            ("b.rs", 0, 0),
            ("a.rs", 0, 0),
        ]);

        assert_eq!(
            breakdown.largest_files,
            vec![
                FileStats {
                    path: "f.rs".into(),
                    chars: 3,
                    tokens: 1
                },
                FileStats {
                    path: "d.rs".into(),
                    chars: 2,
                    tokens: 1
                },
                FileStats {
                    path: "e.rs".into(),
                    chars: 2,
                    tokens: 1
                },
                FileStats {
                    path: "c.rs".into(),
                    chars: 1,
                    tokens: 1
                },
                FileStats {
                    path: "a.rs".into(),
                    chars: 0,
                    tokens: 0
                },
            ]
        );
        assert_eq!(
            breakdown.smallest_files,
            vec![
                FileStats {
                    path: "a.rs".into(),
                    chars: 0,
                    tokens: 0
                },
                FileStats {
                    path: "b.rs".into(),
                    chars: 0,
                    tokens: 0
                },
                FileStats {
                    path: "c.rs".into(),
                    chars: 1,
                    tokens: 1
                },
                FileStats {
                    path: "d.rs".into(),
                    chars: 2,
                    tokens: 1
                },
                FileStats {
                    path: "e.rs".into(),
                    chars: 2,
                    tokens: 1
                },
            ]
        );
    }

    #[test]
    fn empty_breakdown_has_no_groups_or_rankings() {
        assert_eq!(
            build_processing_breakdown(std::iter::empty()),
            Default::default()
        );
    }
}
