use crate::domain::ArchiveEntrySource;

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct PreviewRowViewModel {
    pub id: u32,
    pub display_path: String,
    pub chars: usize,
    pub tokens: usize,
    pub archive: Option<ArchiveEntrySource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum ResultTab {
    #[default]
    Tree,
    Content,
}
