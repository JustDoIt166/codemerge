#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "usage: $0 <version> <arch-label>" >&2
  exit 1
fi

version="$1"
arch_label="$2"

app_name="${APP_NAME:-CodeMerge}"
bundle_id="${BUNDLE_ID:-io.github.hellotime.codemerge}"
binary_path="${BINARY_PATH:-target/release/codemerge}"
dist_dir="${DIST_DIR:-dist}"
plist_template="${PLIST_TEMPLATE:-packaging/macos/Info.plist.template}"
executable_name="${EXECUTABLE_NAME:-codemerge}"

if [[ ! -f "$binary_path" ]]; then
  echo "missing macOS binary: $binary_path" >&2
  exit 1
fi

if [[ ! -f "$plist_template" ]]; then
  echo "missing plist template: $plist_template" >&2
  exit 1
fi

mkdir -p "$dist_dir"

staging_root="$(mktemp -d "${TMPDIR:-/tmp}/codemerge-macos.XXXXXX")"
cleanup() {
  rm -rf "$staging_root"
}
trap cleanup EXIT

app_dir="$staging_root/${app_name}.app"
contents_dir="$app_dir/Contents"
macos_dir="$contents_dir/MacOS"
resources_dir="$contents_dir/Resources"
dmg_dir="$staging_root/dmg"

mkdir -p "$macos_dir" "$resources_dir" "$dmg_dir"
cp "$binary_path" "$macos_dir/$executable_name"
chmod +x "$macos_dir/$executable_name"

sed \
  -e "s|__VERSION__|$version|g" \
  -e "s|__BUNDLE_ID__|$bundle_id|g" \
  -e "s|__EXECUTABLE__|$executable_name|g" \
  -e "s|__APP_NAME__|$app_name|g" \
  "$plist_template" > "$contents_dir/Info.plist"

zip_path="$dist_dir/codemerge-${version}-macos-${arch_label}.zip"
dmg_path="$dist_dir/codemerge-${version}-macos-${arch_label}.dmg"

ditto -c -k --sequesterRsrc --keepParent "$app_dir" "$zip_path"

cp -R "$app_dir" "$dmg_dir/"
ln -s /Applications "$dmg_dir/Applications"
hdiutil create \
  -volname "$app_name" \
  -srcfolder "$dmg_dir" \
  -ov \
  -format UDZO \
  "$dmg_path" >/dev/null

echo "created $zip_path"
echo "created $dmg_path"
