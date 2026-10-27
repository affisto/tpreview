# Release bundles and installation

## VS Code extension releases

The extension uses independent `extension-vX.Y.Z` tags. Do not use a plain `v*` tag for it: those tags build the Rust CLI. An extension release is never marked GitHub's latest release, so the CLI's `/releases/latest/download/install.sh` URL keeps working.

1. Update `extensions/vscode/package.json` and its lockfile, plus versioned extension download links in both READMEs.
2. In `extensions/vscode`, run `npm ci`, `npm run check`, `npm test`, `npm run build`, `npm run test:ui`, `npm run test:host`, and `npm run package`.
3. Commit the extension changes and push them. Tag that commit with the matching version, for example `git tag extension-v0.1.0`, then `git push origin extension-v0.1.0`.
4. The **VS Code extension** workflow verifies the build, browser tests and real VS Code host test, then uploads the platform-independent VSIX, checksum and preview image to GitHub Releases. It publishes only after every verification passes.

The package filename is derived from `package.json`, and tag builds reject a version mismatch. Reruns can finish a draft release but never overwrite a published release. Users download the VSIX and choose **Extensions → … → Install from VSIX…**; Node.js and Rust are only development dependencies. Marketplace publishing is separate and requires a configured Marketplace publisher account.

## Terminal application releases

The `Build and verify` Actions workflow produces macOS/Linux bundles for ARM64 and x86_64. Main pushes and pull requests retain these as downloadable Actions artifacts. A `v*` tag additionally publishes them to GitHub Releases after all native builds and installer tests pass. The installer runs with POSIX sh; Python is used only for build/test automation.

## Maintainer procedure

1. Update the version in `Cargo.toml`, the `tpreview` entry in `Cargo.lock`, and `TPREVIEW_DEFAULT_VERSION` in `install.sh`. Version checks reject disagreement or a tag that does not match Cargo. Update version-pinned README examples for the new release.
2. Run `sh -n install.sh scripts/publish-release.sh`, `python3 scripts/test_install.py`, and the usual Rust checks. Commit only the intended changes.
3. Push the commit and version tag:

   ```sh
   git push origin main
   git tag v0.2.0
   git push origin v0.2.0
   ```

4. Watch the **tag** run in Actions. It checks formatting, Clippy, Rust tests, release builds, real PTY behavior, and Linux tmux integration. Installer tests run on Linux and macOS.
5. After all jobs pass, the release job downloads the four Actions artifacts and verifies their checksums, `VERSION` records and executable members. It creates a draft, uploads every asset, then publishes it. Published curl and wget installation paths are exercised against real GitHub URLs.

Publishing uses the workflow's `GITHUB_TOKEN` with `contents: write` only in the tag release job. Main/PR builds have read-only permissions and never publish. No personal access token or manual artifact upload is required.

A failed upload leaves a draft, which can be retried by re-running the failed job. Published bundles are not overwritten by retries; changes require a new version tag. A manual workflow run against an existing tag can also complete that tag's release. Do not move an already-published tag.

Tags containing `-` (for example `v0.2.0-rc.1`) publish as prereleases and do not become `latest`. Plain version tags such as `v0.2.0` publish as regular early 0.x releases and become `latest`; this does not expand the documented terminal/image compatibility claims.

## Release assets

- `tpreview-{target}.tar.gz` and the matching `.sha256`, for each of the four targets.
- `install.sh` and `install.sh.sha256`.
- `SHA256SUMS` covering the four bundles and installer.

Each archive contains `tpreview`, `VERSION`, `install.sh`, `README.md`, `docs/`, `LICENSE`, `FONT-LICENSE.txt` and `THIRD_PARTY_NOTICES.md`. Archive names remain stable so direct `/releases/latest/download/...` URLs work. The installer embedded in each release pins that release's version to avoid racing a future update of `latest` while downloading multiple files.

## Installer behavior

- Auto-detects `Darwin`/`Linux` and `x86_64`/`arm64`/`aarch64` (`amd64` is accepted as an alias).
- Installs to `$HOME/.local/bin` by default. `--bin-dir DIR` or `TPREVIEW_INSTALL_DIR` overrides it, including paths with spaces.
- `--version vX.Y.Z` / `TPREVIEW_VERSION` pins another compatible release. `--downloader curl|wget` / `TPREVIEW_DOWNLOADER` chooses the transport; auto prefers curl and falls back to wget.
- Downloads from GitHub's HTTPS release URLs, validates the archive's one-line SHA-256 manifest, and extracts only the named executable to a temporary file in the install directory.
- Verifies `tpreview --version` before replacing the destination using a same-directory rename. Failed downloads, checksums, missing executables and wrong versions preserve an existing installation.
- Does not modify shell profiles, invoke sudo, install compilers or change security/quarantine settings. It reports an unavailable platform or musl rather than installing an incompatible GNU bundle.

`--bin-dir` can name a system directory if the invoking account already has write permission. No privilege escalation is attempted. The bundled documentation/licenses remain available in the archive; the automatic installer installs only the executable.

## Validation

`python3 scripts/test_install.py` runs real shell/tar/hash/filesystem integration tests using fixture curl/wget responses. It covers all four OS/architecture selections through both downloaders, paths with spaces, version overrides, malformed checksums, failed downloads, wrong/missing executables, unsupported platforms and release bundle validation. This is complemented by the release job's real public-download installation smoke test using both curl and wget. Core TUI compatibility is tracked separately in [verification.md](verification.md).
