use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};
use include_dir::{Dir, include_dir};

static ASSETS_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets");

pub(super) struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ASSETS_DIR
            .get_file(path)
            .map(|file| Cow::Borrowed(file.contents())))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let dir = if path.is_empty() {
            Some(&ASSETS_DIR)
        } else {
            ASSETS_DIR.get_dir(path)
        };

        Ok(dir
            .map(|dir| {
                dir.files()
                    .filter_map(|file| file.path().file_name())
                    .map(|name| SharedString::from(name.to_string_lossy().into_owned()))
                    .collect()
            })
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use std::str;

    use super::AppAssets;
    use gpui::AssetSource;

    #[test]
    fn window_control_icons_are_embedded_and_themeable() {
        let assets = AppAssets;

        for path in [
            "icons/window-minimize.svg",
            "icons/window-maximize.svg",
            "icons/window-restore.svg",
            "icons/window-close.svg",
        ] {
            let svg = AssetSource::load(&assets, path)
                .expect("asset lookup should succeed")
                .unwrap_or_else(|| panic!("missing embedded asset: {path}"));

            let svg = str::from_utf8(svg.as_ref()).expect("embedded asset should be valid utf-8");
            assert!(
                svg.contains("currentColor"),
                "window control icon should follow theme text color: {path}"
            );
        }
    }
}
