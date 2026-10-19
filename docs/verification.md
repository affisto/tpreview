# Verification and compatibility

Updated: 2026-10-20. Local host: macOS / Apple Silicon (arm64), Rust 1.92.0. This document distinguishes executable tests from protocol simulation and real-terminal visual checks.

## Navigation and frontmatter release (2026-10-20)

CLI 0.1.2 and MD Visual Editor 0.1.3 were verified locally with:

- Rust formatting, Clippy, all **30 core tests**, and an optimized release build. New checks cover repeated file selection with Vim keys, narrow panels, mouse decoding/hit ranges, Unicode/wrapped/table links, absolute paths, plain-text documents, stdin anchors, and YAML/TOML frontmatter.
- Real PTY and isolated tmux integration. PTY checks include file selection, mouse capture restoration, frontmatter, and browser-launch arguments through a fixture OS launcher; they do not launch a live external browser.
- TypeScript checking, **11 unit tests** and **17 Playwright UI tests**, including byte-preserved metadata with BOM/CRLF, body edits/undo, link clicks and waiting for pending edits before navigation.
- An isolated real VS Code host: metadata survives save, a relative encoded path opens another document, its heading below 70 paragraphs scrolls into view, and an absolute link returns to the original visual editor. Existing Mermaid, save/undo/redo, external changes and source switching checks also pass. The generated screenshot is `dist/vscode-editor-preview.png`.
- All **9 installer integration tests**, covering both downloaders and four target platforms with fixtures.

Tagged release workflows repeat the required checks before publishing. The historical graphics/SSH limitations below still apply.

## Initial release results (2026-10-12)

| Check | Evidence / result |
| --- | --- |
| Prototype | `cargo run --example probe`: parsed Markdown fences; flowchart branch/cycle and request/reply sequence rendered to Unicode and SVG |
| Static checks | `cargo fmt --all --check`; `cargo clippy --locked --all-targets -- -D warnings` |
| Core tests | `cargo test --locked --all-targets`: **24 tests passed**; Unicode width, typed Markdown extraction, supported/error grammar, model limits, large tables, ANSI safety, relative links, search, anchors, stale results, last-good results, unchanged-result cache, real PNG, non-TTY output, tiny viewports and special-file rejection |
| PTY integration | `python3 scripts/pty_smoke.py`: controlling-TTY keyboard, stdin pipe, no controlling terminal, fallback, atomic/burst saves, temporary syntax error, deletion/recreation, resize, normal/Ctrl+C/SIGTERM restoration |
| Simulated graphics endpoint | PTY test answers Kitty/cell-size queries and verifies PNG placement + cleanup bytes and detail controls. It does **not** emulate or visually validate a terminal's graphics compositor |
| Real tmux | `python3 scripts/tmux_smoke.py`: isolated tmux 3.6 server; text preview, relative link/fragment/back, search next, contents, preserve position across insertion, no forced navigation on another file's edit, shell restored |
| PNG visual inspection | Actual worker rasterization of Korean/Japanese flowchart and sequence. Labels, connecting arrows and request/reply messages visually checked in generated PNGs |
| Release build | `cargo build --locked --release`; Apple Silicon binary about 9.7 MiB, normal macOS system frameworks/libraries only (`otool -L`) |

PTY tests compare terminal attributes before/after, ignoring only Darwin's transient `PENDIN` bit. Testing exposed a Darwin `/dev/tty` readiness issue with `poll`; the input owner now uses `select`, verified by real PTY/tmux input. Renderer processes are spawned from the application executable, not from Python or Node.

All four platform jobs passed in [CI run 38246934617](https://github.com/affisto/tpreview/actions/runs/38246934617), executable-source commit `337cdaa8294c958264af5d0baf10a2a38d4e2042`. Each ran fmt, Clippy, all 24 core tests, release build and PTY/protocol checks; both Linux jobs also ran real tmux integration. The downloaded macOS ARM executable was additionally extracted outside the checkout and successfully rendered a Mermaid document there. Artifact SHA-256 checksums were verified before packaging the release. The subsequent documentation-only commit records these results; executable sources are unchanged.

## Environment matrix

| Environment | Text | Image | Status / scope |
| --- | --- | --- | --- |
| macOS arm64, real PTY | Passed | Protocol simulated | Input, render, watch, navigation and terminal restoration executed locally |
| macOS arm64, tmux 3.6 | Passed | Intentionally text | Real isolated tmux server, no user configuration changes |
| macOS non-TTY stdout | Passed | Disabled | Readable text, no escape/query bytes |
| TTY stdout without controlling TTY | Passed | Disabled | Safely exits after plain output |
| Ghostty direct | Manual check pending | Manual check pending | Installed app access was refused by computer-control tooling; **not tested as a real Ghostty window** |
| Kitty direct | Manual check pending | Manual check pending | Protocol implementation and simulated replies tested, actual compositor not tested |
| Sixel/iTerm2-only terminals | Text fallback | Not implemented | No terminal-name heuristic enables unsupported image protocols |
| Ubuntu 24.04 x86_64 / arm64 | Passed in CI | Protocol simulation passed; GUI pending | Native builds, 24 tests, PTY, watch/restore and real tmux; no live GUI/SSH claim |
| macOS 15 Intel | Passed in CI | Protocol simulation passed; GUI pending | Native build, 24 tests, PTY, watch/restore |
| macOS 14 ARM64 | Passed in CI | Protocol simulation passed; GUI pending | Native build, 24 tests, PTY, watch/restore; downloaded executable tested locally |
| SSH to macOS/Linux | Manual end-to-end check pending | Manual end-to-end check pending | Inline payloads avoid shared filesystem assumptions; no live remote-host session was used |
| tmux image passthrough | Text fallback | Not enabled | Enabling `allow-passthrough` does not opt this release into image mode |

## Reproduce the core demo

From the checkout:

```sh
cargo build --locked --release
./target/release/tpreview --watch examples/demo.md
```

1. Read the flowchart and request/reply sequence. Use Tab/Enter for detail; Esc returns.
2. Press `/`, type `Preview`, Enter; press `n` and `N`.
3. Use `t` and Enter to jump to Reading tools. Select the next-document link with Tab, Enter, then `b`.
4. Use `f` to choose another example; use arrows horizontally on the long table and Rust code. Resize to 32 columns.
5. In a second shell, copy the example into a temporary directory and watch that copy. Change a diagram label; confirm the new label appears without manual reload.
6. Remove a closing bracket from a previously valid flowchart. Confirm the previous-result notice, current source/error and responsive keys. Restore the bracket.
7. Perform several rapid writes followed by rename-over replacement; confirm the last document wins. Delete/recreate the file; the TUI must remain open.
8. Quit with `q`, repeat with Ctrl+C, and verify shell echo/cursor behavior. The PTY script additionally compares termios and tests SIGTERM.

## Manual graphics checklist (still required)

Use a direct Ghostty or Kitty window, outside tmux:

```sh
./target/release/tpreview --doctor
./target/release/tpreview --render image --watch examples/demo.md
```

Record OS/terminal versions, terminal font, `--doctor` output and window size. `image / Kitty` requires a positive query result; fallback is not a passed image test.

- Verify flowchart and sequence PNGs are actually visible inline, with complete labels and arrows.
- Scroll them partly above/below the viewport. Ensure clipping stays inside the reading region and no old image persists.
- Resize repeatedly; toggle files/contents/help, enter/exit detail, zoom `+/-`, pan, navigate a relative document and return.
- Edit a visible diagram, then introduce/recover from syntax error. Check both the caption and visual image replacement.
- Quit via q/Ctrl+C, reopen, and inspect for stale placements. Repeat with SIGTERM from a second shell.
- Repeat inside tmux: expect text, **not** images. No automatic tmux configuration changes should occur.
- Repeat through an actual SSH session, including slow transport. Record it separately from local PTY tests.
- On Linux without CJK fonts, inspect `--doctor`; test `--font` using a local CJK font and document what glyphs appear. Never count tofu/missing glyphs as a passing raster check.

## Outstanding fidelity limits

No screenshot comparison to Mermaid.js has been run. Font-specific geometric parity is not promised. Image transport is intentionally limited to one protocol. Device-specific image remnants, nested multiplexers, SSH transport and unusual terminal Unicode-width settings need real-device testing. See [the complete limitations](mermaid.md).
