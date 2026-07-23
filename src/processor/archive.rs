use std::fs::File;
use std::io::Read;
use std::path::Path;

use zip::ZipArchive;

use crate::processor::error::{ProcessorError, ProcessorResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipFileEntry {
    pub archive_name: String,
    pub display_name: String,
}

pub fn is_zip_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("zip"))
}

pub fn list_zip_file_entries(path: &Path) -> ProcessorResult<Vec<ZipFileEntry>> {
    let file = File::open(path).map_err(|error| ProcessorError::io("open zip", error))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| ProcessorError::archive("open zip", error))?;
    let mut entries = Vec::new();

    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| ProcessorError::archive("read zip entry", error))?;
        if entry.is_dir() {
            continue;
        }
        let archive_name = entry.name().to_string();
        let Some(display_name) = normalize_entry_name(&archive_name) else {
            continue;
        };
        entries.push(ZipFileEntry {
            archive_name,
            display_name,
        });
    }

    Ok(entries)
}

pub fn read_zip_entry_text(path: &Path, entry_name: &str) -> ProcessorResult<String> {
    let file = File::open(path).map_err(|error| ProcessorError::io("open zip", error))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| ProcessorError::archive("open zip", error))?;
    let mut entry = archive
        .by_name(entry_name)
        .map_err(|error| ProcessorError::archive("open zip entry", error))?;
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .map_err(|error| ProcessorError::io("read zip entry", error))?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

fn normalize_entry_name(name: &str) -> Option<String> {
    let normalized = name.replace('\\', "/");
    let trimmed = normalized.trim_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    let mut parts = Vec::new();
    for part in trimmed.split('/') {
        if part.is_empty() || matches!(part, "." | "..") {
            return None;
        }
        parts.push(part);
    }

    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::{is_zip_path, normalize_entry_name};
    use std::path::Path;

    #[test]
    fn normalize_entry_name_rejects_empty_and_parent_segments() {
        assert_eq!(
            normalize_entry_name("src\\main.rs"),
            Some("src/main.rs".to_string())
        );
        assert_eq!(
            normalize_entry_name("/nested/lib.rs/"),
            Some("nested/lib.rs".to_string())
        );
        assert_eq!(normalize_entry_name("../secret.txt"), None);
        assert_eq!(normalize_entry_name("folder//file.txt"), None);
        assert_eq!(normalize_entry_name(""), None);
    }

    #[test]
    fn detects_zip_path_case_insensitively() {
        assert!(is_zip_path(Path::new("bundle.zip")));
        assert!(is_zip_path(Path::new("bundle.ZIP")));
        assert!(!is_zip_path(Path::new("bundle.tar.gz")));
    }
}
