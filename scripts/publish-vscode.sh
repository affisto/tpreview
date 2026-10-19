#!/bin/sh
# Publish the verified extension without replacing the CLI's latest release.
set -eu
: "${GH_REPO:?GH_REPO is required}"
: "${RELEASE_TAG:?RELEASE_TAG is required}"
assets=${1:?asset directory is required}
version=$(node -p 'require("./extensions/vscode/package.json").version')
test "$RELEASE_TAG" = "extension-v$version"
filename="tpreview-markdown-$version.vsix"
(cd "$assets" && sha256sum -c "$filename.sha256")
state=$(gh release view "$RELEASE_TAG" --repo "$GH_REPO" --json isDraft --jq '.isDraft' 2>/dev/null || printf 'missing')
if [ "$state" = false ]; then
    printf '%s\n' 'Release already published. Use a new version tag for changes.'
    exit 0
fi
notes=$(mktemp)
trap 'rm -f "$notes"' 0
cat > "$notes" <<NOTES
Visual Markdown editing inside VS Code, with bundled offline Mermaid previews.

Display name: **MD Visual Editor**. The existing extension ID is retained so this package updates previous installations. See the [한국어 사용 가이드](https://github.com/$GH_REPO/blob/$RELEASE_TAG/docs/vscode-usage.ko.md) for screenshots and a walkthrough.

### Install on another computer

1. Download [$filename](https://github.com/$GH_REPO/releases/download/$RELEASE_TAG/$filename) below.
2. In VS Code, open **Extensions → … → Install from VSIX…** and select the downloaded file.
3. Open a Markdown file and run **MD Visual Editor: Open Visual Markdown Editor** in the Command Palette.

Or run this command from your Downloads directory:

\`\`\`sh
code --install-extension $filename
\`\`\`

Requires VS Code **1.96 or later**. The VSIX is platform-independent; Node.js and Rust are not needed on the installation computer. Source build and package details are in the [extension guide](https://github.com/$GH_REPO/blob/$RELEASE_TAG/extensions/vscode/README.md).

### Included

- Visual editing of headings, formatting, lists, tasks and tables.
- Readable YAML/TOML frontmatter, preserved exactly while editing the document body.
- Clickable browser URLs, relative/absolute document paths and heading anchors.
- Offline Mermaid previews with code editing and last-valid-result fallback.
- Source editor switching, normal file saving and VS Code undo/redo.
- Original spelling of unchanged Markdown blocks and draft preservation on external-edit conflicts.

Metadata editing, unclosed frontmatter, raw HTML, reference links, footnotes and unsupported syntax use the source editor. Edited blocks may be normalized. Real OS IME candidate-window behavior and remote hosts still need manual verification.

The package passed type checking, unit tests, browser interaction tests and an actual VS Code host test in GitHub Actions. The adjacent SHA-256 file verifies the download. This release is distributed through GitHub, not the VS Code Marketplace; install updates from their new VSIX files.
NOTES
if [ "$state" = missing ]; then
    gh release create "$RELEASE_TAG" --repo "$GH_REPO" --verify-tag --draft --latest=false \
        --title "MD Visual Editor $version" --notes-file "$notes"
fi
gh release upload "$RELEASE_TAG" "$assets/$filename" "$assets/$filename.sha256" "$assets/vscode-editor-preview.png" --repo "$GH_REPO" --clobber
gh release edit "$RELEASE_TAG" --repo "$GH_REPO" --draft=false --latest=false --notes-file "$notes"
