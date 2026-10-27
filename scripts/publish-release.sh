#!/bin/sh
# Called only by the tag workflow after all native builds/tests pass.
set -eu
: "${GH_REPO:?GH_REPO is required}"
: "${RELEASE_TAG:?RELEASE_TAG is required}"
: "${GITHUB_SHA:?GITHUB_SHA is required}"
: "${GITHUB_RUN_ID:?GITHUB_RUN_ID is required}"
assets=${1:?asset directory is required}
state=$(gh release view "$RELEASE_TAG" --repo "$GH_REPO" --json isDraft --jq '.isDraft' 2>/dev/null || printf 'missing')
if [ "$state" = false ]; then
    printf '%s\n' 'Release already published. Keeping its immutable bundles; use a new tag for changes.'
    exit 0
fi
notes=$(mktemp)
trap 'rm -f "$notes"' 0
cat > "$notes" <<NOTES
Offline Markdown and Mermaid terminal reader. This release adds ER, class and state diagrams alongside flowcharts and sequence diagrams. Tables now wrap long cells to the available width, connect their borders, honor left/center/right alignment, and preserve Unicode text and links across line breaks.

ER output includes attributes, PK/FK/UK, comments and relationship cardinalities in both text and images. Invalid edits retain the previous valid diagram. Very narrow tables stack cells to keep every value readable. The application remains a single native executable with no runtime downloads.

Install with curl:
\`\`\`sh
curl -fsSL https://github.com/$GH_REPO/releases/download/$RELEASE_TAG/install.sh | sh
\`\`\`

Or wget:
\`\`\`sh
wget -qO- https://github.com/$GH_REPO/releases/download/$RELEASE_TAG/install.sh | sh -s -- --downloader wget
\`\`\`

The installer detects macOS/Linux and ARM64/x86_64, verifies SHA-256, and installs to \`~/.local/bin\` without sudo. Use \`--bin-dir DIR\` to change the destination. It leaves an existing executable intact if verification fails. No Node.js, Python, browser or Rust toolchain is needed to install or run the bundle.

All four bundles were built and tested by [GitHub Actions](https://github.com/$GH_REPO/actions/runs/$GITHUB_RUN_ID) at commit $GITHUB_SHA. Each archive includes the executable, version, documentation, installer and license notices; adjacent checksum files and SHA256SUMS cover the downloadable assets.

Rendering limits still apply: pie/Gantt and other unlisted Mermaid families are unsupported; unquoted non-ASCII ER attribute identifiers are unsupported, and cyclic ER relationships appear separately in text. tmux uses text, real Ghostty/Kitty compositor and SSH image behavior need manual validation, and CJK image labels need a local CJK font. Linux bundles require GNU libc. macOS binaries are not notarized.
NOTES
if [ "$state" = missing ]; then
    gh release create "$RELEASE_TAG" --repo "$GH_REPO" --verify-tag --draft \
        --title "tpreview $RELEASE_TAG" --notes-file "$notes"
fi
# Only a draft is mutable. Publishing happens after every upload succeeds.
gh release upload "$RELEASE_TAG" "$assets"/*.tar.gz "$assets"/*.sha256 "$assets/install.sh" "$assets/SHA256SUMS" --repo "$GH_REPO" --clobber
case "$RELEASE_TAG" in
    *-*) gh release edit "$RELEASE_TAG" --repo "$GH_REPO" --draft=false --prerelease --latest=false --notes-file "$notes" ;;
    *) gh release edit "$RELEASE_TAG" --repo "$GH_REPO" --draft=false --prerelease=false --latest --notes-file "$notes" ;;
esac
