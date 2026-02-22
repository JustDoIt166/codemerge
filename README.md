# CodeMerge (Rust + Iced)

Desktop file merge tool for Linux and Windows.

## Features
- Select folder, files, and `.gitignore`
- Folder/ext blacklist editing
- Output formats: Default/XML/Plain/Markdown
- Full mode or Tree-only mode
- Optional compression
- Char/token estimation
- Copy tree/content and download merged output
- Config persistence (`language/options/blacklists`)

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
- Large preview defaults to 1MB; full load requires confirmation.
