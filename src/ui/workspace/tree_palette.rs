use gpui::Hsla;
use gpui_component::Theme;

use super::model::{FilterMatchKind, TreeIconKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TreeAccentRole {
    Neutral,
    Primary,
    Warning,
    Accent,
    Danger,
    Muted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TreeChipTone {
    Neutral,
    Accent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TreeRowPalette {
    selected: bool,
    icon_role: TreeAccentRole,
    badge_tone: TreeChipTone,
    extension_tone: TreeChipTone,
    highlight_tone: TreeChipTone,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ResolvedTreeRowPalette {
    pub label_fg: Hsla,
    pub secondary_fg: Hsla,
    pub chevron_fg: Hsla,
    pub selection_bar_bg: Hsla,
    pub icon_bg: Hsla,
    pub icon_fg: Hsla,
    pub badge_bg: Hsla,
    pub badge_fg: Hsla,
    pub extension_bg: Hsla,
    pub extension_fg: Hsla,
    pub match_bg: Hsla,
    pub match_fg: Hsla,
    guide_base: Hsla,
    guide_opacities: [f32; 4],
}

impl TreeRowPalette {
    pub(super) fn new(
        selected: bool,
        icon_kind: TreeIconKind,
        is_filter_match: bool,
        match_kind: Option<FilterMatchKind>,
    ) -> Self {
        let icon_role = if selected {
            TreeAccentRole::Neutral
        } else {
            match icon_kind {
                TreeIconKind::FolderOpen | TreeIconKind::Document => TreeAccentRole::Primary,
                TreeIconKind::FolderClosed | TreeIconKind::Config => TreeAccentRole::Warning,
                TreeIconKind::Code | TreeIconKind::Data => TreeAccentRole::Accent,
                TreeIconKind::Media => TreeAccentRole::Danger,
                TreeIconKind::Text => TreeAccentRole::Muted,
            }
        };

        let badge_tone = if selected {
            TreeChipTone::Neutral
        } else {
            TreeChipTone::Accent
        };

        let extension_tone = TreeChipTone::Neutral;
        let highlight_tone = if is_filter_match && match_kind.is_some() {
            TreeChipTone::Accent
        } else {
            TreeChipTone::Neutral
        };

        Self {
            selected,
            icon_role,
            badge_tone,
            extension_tone,
            highlight_tone,
        }
    }

    pub(super) fn resolve(self, theme: &Theme) -> ResolvedTreeRowPalette {
        let (icon_bg, icon_fg) = resolve_icon_colors(self.icon_role, self.selected, theme);
        let (badge_bg, badge_fg) = resolve_chip_colors(self.badge_tone, self.selected, theme);
        let (extension_bg, extension_fg) =
            resolve_extension_colors(self.extension_tone, self.selected, theme);
        let (match_bg, match_fg) = resolve_match_colors(self.highlight_tone, theme);

        ResolvedTreeRowPalette {
            label_fg: theme.foreground,
            secondary_fg: if self.selected {
                theme.muted_foreground.opacity(0.92)
            } else {
                theme.muted_foreground
            },
            chevron_fg: if self.selected {
                theme.muted_foreground.opacity(0.78)
            } else {
                theme.muted_foreground.opacity(0.7)
            },
            selection_bar_bg: if self.selected {
                theme.primary.opacity(0.72)
            } else {
                theme.transparent
            },
            icon_bg,
            icon_fg,
            badge_bg,
            badge_fg,
            extension_bg,
            extension_fg,
            match_bg,
            match_fg,
            guide_base: if self.selected {
                theme.list_active_border
            } else {
                theme.muted_foreground
            },
            guide_opacities: if self.selected {
                [0.38, 0.3, 0.22, 0.16]
            } else {
                [0.26, 0.2, 0.16, 0.12]
            },
        }
    }
}

impl ResolvedTreeRowPalette {
    pub(super) fn guide_color(&self, depth: usize) -> Hsla {
        self.guide_base.opacity(match depth {
            0 => self.guide_opacities[0],
            1 => self.guide_opacities[1],
            2 => self.guide_opacities[2],
            _ => self.guide_opacities[3],
        })
    }
}

fn resolve_icon_colors(role: TreeAccentRole, selected: bool, theme: &Theme) -> (Hsla, Hsla) {
    if selected {
        return (
            theme.secondary.opacity(0.72),
            theme.foreground.opacity(0.86),
        );
    }

    match role {
        TreeAccentRole::Neutral => (
            theme.secondary.opacity(0.6),
            theme.muted_foreground.opacity(0.9),
        ),
        TreeAccentRole::Primary => (theme.primary.opacity(0.09), theme.primary.opacity(0.88)),
        TreeAccentRole::Warning => (theme.warning.opacity(0.09), theme.warning.opacity(0.86)),
        TreeAccentRole::Accent => (theme.accent.opacity(0.09), theme.accent.opacity(0.88)),
        TreeAccentRole::Danger => (theme.danger.opacity(0.08), theme.danger.opacity(0.84)),
        TreeAccentRole::Muted => (
            theme.secondary.opacity(0.5),
            theme.muted_foreground.opacity(0.86),
        ),
    }
}

fn resolve_chip_colors(tone: TreeChipTone, selected: bool, theme: &Theme) -> (Hsla, Hsla) {
    if selected {
        return (
            theme.secondary.opacity(0.86),
            theme.muted_foreground.opacity(0.92),
        );
    }

    match tone {
        TreeChipTone::Neutral => (
            theme.secondary.opacity(0.7),
            theme.muted_foreground.opacity(0.9),
        ),
        TreeChipTone::Accent => (theme.accent.opacity(0.12), theme.accent.opacity(0.9)),
    }
}

fn resolve_extension_colors(tone: TreeChipTone, selected: bool, theme: &Theme) -> (Hsla, Hsla) {
    if selected {
        return (
            theme.secondary.opacity(0.92),
            theme.muted_foreground.opacity(0.92),
        );
    }

    match tone {
        TreeChipTone::Neutral => (
            theme.secondary.opacity(0.58),
            theme.muted_foreground.opacity(0.9),
        ),
        TreeChipTone::Accent => (theme.accent.opacity(0.12), theme.accent.opacity(0.9)),
    }
}

fn resolve_match_colors(tone: TreeChipTone, theme: &Theme) -> (Hsla, Hsla) {
    match tone {
        TreeChipTone::Neutral => (
            theme.secondary.opacity(0.58),
            theme.muted_foreground.opacity(0.9),
        ),
        TreeChipTone::Accent => (theme.primary.opacity(0.14), theme.primary),
    }
}

#[cfg(test)]
mod tests {
    use super::{TreeAccentRole, TreeChipTone, TreeRowPalette};
    use gpui::hsla;
    use gpui_component::{Theme, ThemeColor};

    use super::super::model::{FilterMatchKind, TreeIconKind};

    fn sample_theme() -> Theme {
        Theme::from(&ThemeColor {
            foreground: hsla(0.00, 0.00, 0.12, 1.0),
            muted_foreground: hsla(0.00, 0.00, 0.36, 1.0),
            primary: hsla(0.60, 0.75, 0.52, 1.0),
            primary_foreground: hsla(0.00, 0.00, 0.98, 1.0),
            accent: hsla(0.45, 0.70, 0.46, 1.0),
            warning: hsla(0.11, 0.82, 0.55, 1.0),
            danger: hsla(0.98, 0.72, 0.56, 1.0),
            secondary: hsla(0.00, 0.00, 0.84, 1.0),
            list_active_border: hsla(0.60, 0.45, 0.48, 1.0),
            ..ThemeColor::default()
        })
    }

    #[test]
    fn selected_palette_keeps_text_and_supporting_elements_neutral() {
        let theme = sample_theme();
        let resolved =
            TreeRowPalette::new(true, TreeIconKind::Code, true, Some(FilterMatchKind::Label))
                .resolve(&theme);

        assert_eq!(resolved.label_fg, theme.foreground);
        assert_eq!(resolved.secondary_fg, theme.muted_foreground.opacity(0.92));
        assert_eq!(resolved.chevron_fg, theme.muted_foreground.opacity(0.78));
        assert_eq!(resolved.icon_fg, theme.foreground.opacity(0.86));
        assert_eq!(resolved.badge_fg, theme.muted_foreground.opacity(0.92));
        assert_eq!(resolved.extension_fg, theme.muted_foreground.opacity(0.92));
        assert_ne!(resolved.label_fg, theme.primary_foreground);
        assert_ne!(resolved.secondary_fg, theme.primary_foreground);
        assert_ne!(resolved.chevron_fg, theme.primary_foreground);
        assert_eq!(resolved.match_fg, theme.primary);
    }

    #[test]
    fn selected_filter_match_only_keeps_primary_on_match_highlight() {
        let theme = sample_theme();
        let palette = TreeRowPalette::new(
            true,
            TreeIconKind::FolderOpen,
            true,
            Some(FilterMatchKind::Path),
        );
        let resolved = palette.resolve(&theme);

        assert_eq!(resolved.match_bg, theme.primary.opacity(0.14));
        assert_eq!(resolved.match_fg, theme.primary);
        assert_ne!(resolved.badge_fg, theme.primary);
        assert_ne!(resolved.icon_fg, theme.primary);
        assert_ne!(resolved.secondary_fg, theme.primary);
    }

    #[test]
    fn unselected_icons_keep_stable_semantic_roles() {
        assert_eq!(
            TreeRowPalette::new(false, TreeIconKind::FolderOpen, false, None).icon_role,
            TreeAccentRole::Primary
        );
        assert_eq!(
            TreeRowPalette::new(false, TreeIconKind::FolderClosed, false, None).icon_role,
            TreeAccentRole::Warning
        );
        assert_eq!(
            TreeRowPalette::new(false, TreeIconKind::Code, false, None).icon_role,
            TreeAccentRole::Accent
        );
        assert_eq!(
            TreeRowPalette::new(false, TreeIconKind::Media, false, None).icon_role,
            TreeAccentRole::Danger
        );
        assert_eq!(
            TreeRowPalette::new(false, TreeIconKind::Text, false, None).icon_role,
            TreeAccentRole::Muted
        );
    }

    #[test]
    fn selected_rows_force_neutral_roles_except_match_highlight() {
        let palette = TreeRowPalette::new(
            true,
            TreeIconKind::Media,
            true,
            Some(FilterMatchKind::Label),
        );

        assert_eq!(palette.icon_role, TreeAccentRole::Neutral);
        assert_eq!(palette.badge_tone, TreeChipTone::Neutral);
        assert_eq!(palette.extension_tone, TreeChipTone::Neutral);
        assert_eq!(palette.highlight_tone, TreeChipTone::Accent);
    }
}
