#[derive(Debug, Clone, Default)]
pub struct ProcessingStats {
    pub processed_files: usize,
    pub skipped_files: usize,
    pub total_chars: usize,
    pub total_tokens: usize,
}
