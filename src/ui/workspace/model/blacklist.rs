use gpui::SharedString;

use super::super::BlacklistItemKind;
use crate::domain::Language;
use crate::utils::i18n::tr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct BlacklistTagViewModel {
    pub kind: BlacklistItemKind,
    pub value: String,
    pub display_label: SharedString,
    pub deletable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::workspace) struct BlacklistSectionViewModel {
    pub kind: BlacklistItemKind,
    pub title: SharedString,
    pub count: usize,
    pub items: Vec<BlacklistTagViewModel>,
}

pub(in crate::ui::workspace) fn build_blacklist_sections(
    folder_blacklist: &[String],
    ext_blacklist: &[String],
    filter: &str,
    language: Language,
) -> Vec<BlacklistSectionViewModel> {
    let filter = filter.trim().to_ascii_lowercase();
    let mut sections = Vec::new();

    let folder_items =
        build_blacklist_section_items(folder_blacklist, BlacklistItemKind::Folder, &filter);
    if !folder_items.is_empty() {
        sections.push(BlacklistSectionViewModel {
            kind: BlacklistItemKind::Folder,
            title: SharedString::from(tr(language, "rules_group_folders")),
            count: folder_items.len(),
            items: folder_items,
        });
    }

    let ext_items = build_blacklist_section_items(ext_blacklist, BlacklistItemKind::Ext, &filter);
    if !ext_items.is_empty() {
        sections.push(BlacklistSectionViewModel {
            kind: BlacklistItemKind::Ext,
            title: SharedString::from(tr(language, "rules_group_extensions")),
            count: ext_items.len(),
            items: ext_items,
        });
    }

    sections
}

fn build_blacklist_section_items(
    items: &[String],
    kind: BlacklistItemKind,
    filter: &str,
) -> Vec<BlacklistTagViewModel> {
    items
        .iter()
        .filter(|item| filter.is_empty() || item.to_ascii_lowercase().contains(filter))
        .map(|item| BlacklistTagViewModel {
            kind,
            value: item.clone(),
            display_label: SharedString::from(item.clone()),
            deletable: true,
        })
        .collect()
}
