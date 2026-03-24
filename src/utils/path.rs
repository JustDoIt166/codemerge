use std::path::Path;

use chrono::{Local, NaiveDate};

use crate::domain::OutputFormat;
use crate::processor::archive::is_zip_path;

pub fn filename(path: &Path) -> String {
    path.file_name()
        .map(|v| v.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn ext(path: &Path) -> String {
    path.extension()
        .map(|v| format!(".{}", v.to_string_lossy().to_lowercase()))
        .unwrap_or_default()
}

pub fn suggested_merge_result_name(
    selected_folder: Option<&Path>,
    selected_files: &[impl AsRef<Path>],
    output_format: OutputFormat,
) -> String {
    suggested_merge_result_name_for_date(
        selected_folder,
        selected_files,
        output_format,
        Local::now().date_naive(),
    )
}

fn suggested_merge_result_name_for_date(
    selected_folder: Option<&Path>,
    selected_files: &[impl AsRef<Path>],
    output_format: OutputFormat,
    date: NaiveDate,
) -> String {
    let base_name = selected_folder
        .map(filename)
        .map(|name| sanitize_filename_component(&name))
        .filter(|name| !name.is_empty())
        .or_else(|| infer_name_from_files(selected_files))
        .unwrap_or_else(|| "selected_files".to_string());

    format!(
        "{}-{}.{}",
        base_name,
        date.format("%Y%m%d"),
        output_extension(output_format)
    )
}

fn infer_name_from_files(files: &[impl AsRef<Path>]) -> Option<String> {
    if files.len() != 1 {
        return None;
    }
    let path = files[0].as_ref();
    if !is_zip_path(path) {
        return None;
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .map(|name| sanitize_filename_component(&name))
        .filter(|name| !name.is_empty());
    stem
}

fn sanitize_filename_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut last_was_separator = false;

    for ch in value.trim().chars() {
        let mapped = if matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
            || ch.is_control()
        {
            '-'
        } else {
            ch
        };

        if mapped == '-' {
            if !last_was_separator {
                out.push(mapped);
            }
            last_was_separator = true;
            continue;
        }

        out.push(mapped);
        last_was_separator = false;
    }

    out.trim_matches([' ', '.', '-']).to_string()
}

fn output_extension(output_format: OutputFormat) -> &'static str {
    match output_format {
        OutputFormat::Xml => "xml",
        OutputFormat::Markdown => "md",
        OutputFormat::PlainText | OutputFormat::Default => "txt",
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use chrono::NaiveDate;

    use super::suggested_merge_result_name_for_date;
    use crate::domain::OutputFormat;

    const NO_FILES: &[PathBuf] = &[];

    #[test]
    fn suggested_merge_result_name_uses_folder_and_date() {
        let name = suggested_merge_result_name_for_date(
            Some(Path::new("D:/Code/codemerge")),
            NO_FILES,
            OutputFormat::Markdown,
            NaiveDate::from_ymd_opt(2026, 3, 19).expect("valid date"),
        );

        assert_eq!(name, "codemerge-20260319.md");
    }

    #[test]
    fn suggested_merge_result_name_falls_back_when_folder_missing() {
        let name = suggested_merge_result_name_for_date(
            None,
            NO_FILES,
            OutputFormat::Default,
            NaiveDate::from_ymd_opt(2026, 3, 19).expect("valid date"),
        );

        assert_eq!(name, "selected_files-20260319.txt");
    }

    #[test]
    fn suggested_merge_result_name_sanitizes_invalid_folder_chars() {
        let name = suggested_merge_result_name_for_date(
            Some(Path::new("bad:name*folder")),
            NO_FILES,
            OutputFormat::Xml,
            NaiveDate::from_ymd_opt(2026, 3, 19).expect("valid date"),
        );

        assert_eq!(name, "bad-name-folder-20260319.xml");
    }

    #[test]
    fn suggested_merge_result_name_uses_zip_stem_when_single_zip() {
        let files = vec![PathBuf::from("D:/Downloads/my-project.zip")];
        let name = suggested_merge_result_name_for_date(
            None,
            &files,
            OutputFormat::Default,
            NaiveDate::from_ymd_opt(2026, 3, 19).expect("valid date"),
        );

        assert_eq!(name, "my-project-20260319.txt");
    }

    #[test]
    fn suggested_merge_result_name_folder_takes_priority_over_zip() {
        let files = vec![PathBuf::from("bundle.zip")];
        let name = suggested_merge_result_name_for_date(
            Some(Path::new("D:/Code/codemerge")),
            &files,
            OutputFormat::Markdown,
            NaiveDate::from_ymd_opt(2026, 3, 19).expect("valid date"),
        );

        assert_eq!(name, "codemerge-20260319.md");
    }

    #[test]
    fn suggested_merge_result_name_ignores_non_zip_single_file() {
        let files = vec![PathBuf::from("readme.txt")];
        let name = suggested_merge_result_name_for_date(
            None,
            &files,
            OutputFormat::Default,
            NaiveDate::from_ymd_opt(2026, 3, 19).expect("valid date"),
        );

        assert_eq!(name, "selected_files-20260319.txt");
    }

    #[test]
    fn suggested_merge_result_name_ignores_multiple_zips() {
        let files = vec![
            PathBuf::from("a.zip"),
            PathBuf::from("b.zip"),
        ];
        let name = suggested_merge_result_name_for_date(
            None,
            &files,
            OutputFormat::Default,
            NaiveDate::from_ymd_opt(2026, 3, 19).expect("valid date"),
        );

        assert_eq!(name, "selected_files-20260319.txt");
    }
}
