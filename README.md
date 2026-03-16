# CodeMerge (Rust + GPUI Component)

Desktop file merge tool with a `gpui + gpui-component` UI and a framework-independent processing core.

## Features
- Select folder, files, and `.gitignore`
- Folder/ext blacklist editing
- Output formats: Default/XML/Plain/Markdown
- Full mode or Tree-only mode
- Optional compression
- Char/token estimation
- Copy tree/content and download merged output
- Config persistence (`language/options/blacklists`)
- Resizable three-pane workspace
- Virtualized result table, tree, and preview list
- Lazy preview indexing and range loading for large files

## Run
```bash
cargo run
```

## Test
```bash
cargo test
```

## Config path
- Windows: `%APPDATA%/codemerge/config.json`
- Linux: `~/.config/codemerge/config.json`

## Notes
- `.gitignore` negation rules (`!`) are not supported in this version.
- Windows is the primary target; Linux is kept compile-compatible.
