use std::path::Path;

use chrono::{Local, NaiveDate};

use crate::domain::OutputFormat;

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
    output_format: OutputFormat,
) -> String {
    suggested_merge_result_name_for_date(selected_folder, output_format, Local::now().date_naive())
}

fn suggested_merge_result_name_for_date(
    selected_folder: Option<&Path>,
    output_format: OutputFormat,
    date: NaiveDate,
) -> String {
    let folder_name = selected_folder
        .map(filename)
        .map(|name| sanitize_filename_component(&name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "selected_files".to_string());

    format!(
        "{}-{}.{}",
        folder_name,
        date.format("%Y%m%d"),
        output_extension(output_format)
    )
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
    use std::path::Path;

    use chrono::NaiveDate;

    use super::suggested_merge_result_name_for_date;
    use crate::domain::OutputFormat;

    #[test]
    fn suggested_merge_result_name_uses_folder_and_date() {
        let name = suggested_merge_result_name_for_date(
            Some(Path::new("D:/Code/codemerge")),
            OutputFormat::Markdown,
            NaiveDate::from_ymd_opt(2026, 3, 19).expect("valid date"),
        );

        assert_eq!(name, "codemerge-20260319.md");
    }

    #[test]
    fn suggested_merge_result_name_falls_back_when_folder_missing() {
        let name = suggested_merge_result_name_for_date(
            None,
            OutputFormat::Default,
            NaiveDate::from_ymd_opt(2026, 3, 19).expect("valid date"),
        );

        assert_eq!(name, "selected_files-20260319.txt");
    }

    #[test]
    fn suggested_merge_result_name_sanitizes_invalid_folder_chars() {
        let name = suggested_merge_result_name_for_date(
            Some(Path::new("bad:name*folder")),
            OutputFormat::Xml,
            NaiveDate::from_ymd_opt(2026, 3, 19).expect("valid date"),
        );

        assert_eq!(name, "bad-name-folder-20260319.xml");
    }
}
