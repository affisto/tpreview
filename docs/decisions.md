# Technical decisions

Recorded 2026-10-12. The starting directory was empty: no existing repository, technology stack, or applicable AGENTS.md. The project is named **tpreview**, published at `affisto/tpreview`.

## Selection evidence

Official repository/docs and the actual crates downloaded by Cargo were inspected before implementation. Versions below are the resolved versions in Cargo.lock, not a claim that every dependency is the newest release.

| Responsibility | Decision | License / maintenance evidence | Distribution and accuracy tradeoff |
| --- | --- | --- | --- |
| TUI | Ratatui 0.30.2 | MIT; maintained [Ratatui project](https://ratatui.rs/) | Native terminal buffer/layout; no GUI or daemon |
| Terminal mode/output | Crossterm 0.29.0 | MIT; [official event documentation](https://docs.rs/crossterm/0.29.0/crossterm/event/index.html) | Raw mode and alternate screen. One custom `/dev/tty` input owner prevents probe replies competing with stdin or keyboard readers |
| Markdown model | pulldown-cmark 0.13.4 | MIT; [CommonMark parser](https://github.com/pulldown-cmark/pulldown-cmark) | Parser event stream is transformed into typed blocks, links and heading identities. Mermaid fences come from parsed events, not Markdown regexes |
| Mermaid | mmdflux 2.6.1, default features off | MIT; [upstream implementation and tests](https://github.com/kevinswiber/mmdflux) | Shared parser/layout engine for text and SVG, including sequence timelines. Only flowchart and sequence are enabled by this application |
| Rasterization | resvg/usvg 0.45.1 | MIT or Apache-2.0; [Linebender repository](https://github.com/linebender/resvg) | Pure Rust SVG-to-PNG path; no Chrome/Cairo. Local font fallback. Image URL/data resolvers disabled |
| Images | Narrow direct implementation of [Kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/) | Protocol specification; application code MIT | Direct PNG payloads work without shared filesystem access. Capability ACK plus cell metrics required. Explicit crops/owned IDs. No Sixel/iTerm2 claims |
| File watching | notify 8.2.0 | CC0-1.0; [notify-rs](https://github.com/notify-rs/notify) | Native FSEvents/inotify, parent-directory watch for rename-over writes, plus periodic reconciliation |
| Display width | unicode-width 0.2.2 + unicode-segmentation 1.13.3 | MIT or Apache-2.0; [width](https://github.com/unicode-rs/unicode-width), [graphemes](https://github.com/unicode-rs/unicode-segmentation) | Grapheme-safe wrapping/cropping; actual terminal glyph behavior can differ for emoji/ambiguous characters |
| Syntax highlighting | syntect 5.3.0, `default-fancy` | MIT; [syntect](https://github.com/trishume/syntect) | Bundled syntax definitions; Rust regex backend avoids a native Oniguruma requirement. Large blocks fall back to plain code |

The selected upstreams had active public repositories, documentation, tests and released crates at inspection. mmdflux is still a fast-moving implementation, not the reference Mermaid JavaScript renderer: pinning exactly 2.6.1 and running conformance fixtures is intentional. Cargo.lock fixes the complete dependency graph. Upgrade the engine only together with the syntax and visual regression checks.

## Alternatives evaluated

- [mermaid-rs-renderer 0.3.1](https://github.com/1jehuang/mermaid-rs-renderer) offers browser-free SVG/PNG and broad diagram coverage. mmdflux was preferred because readable text is a first-class output and shares parsed semantics with SVG. Supporting multiple diagram engines would introduce compatibility differences and a larger executable.
- [ratatui-image 10.0.6](https://github.com/ratatui/ratatui-image) was inspected. Its picker supports multiple protocols and uses environment fallbacks; its standard query reads stdio, which needs care with piped documents. Default features can also involve chafa. A narrow Kitty transport with one dedicated input reader was chosen to make detection, clipping and cleanup auditable. Broader protocol support is deferred.
- Official Mermaid CLI/browser rendering can be higher fidelity, but violates the mandatory offline single-executable runtime experience. No optional external renderer is implemented or advertised.

## Prototype before full UI

`examples/probe.rs` extracts actual Mermaid fences from Markdown and produces two text renderings and SVG files under `target/probe/`. The flowchart contains a branch and cycle; the sequence contains request/reply arrows. This ran successfully before the application UI was built.

`examples/terminal_probe.rs` exercises rasterization, capability negotiation, image transmission or text fallback, and cleanup. Ghostty was installed but the session's computer-control tool refused access to the app. Therefore actual Ghostty pixels were not marked tested. PTY protocol tests provide automated evidence; a real terminal check remains separate.

CJK PNGs were subsequently generated through the real renderer worker and visually inspected: Korean/Japanese flowchart labels and sequence messages/arrows were present.

## Detection and tmux policy

Kitty query (`a=q`, ID 31) and cell-size query (`CSI 16 t`) run for at most 350 ms. Only a matching positive graphics response and valid pixel metrics enable images. `TERM` can prohibit an unsafe multiplexer path but cannot enable images. Keyboard input received during probing is queued; APC/OSC/DCS/unknown CSI replies are consumed by the same decoder, including late responses. Bracketed paste is ignored rather than interpreted as navigation commands.

The [tmux FAQ](https://github.com/tmux/tmux/wiki/FAQ) and graphics protocol were reviewed. DCS passthrough, outer-terminal capabilities, pane offsets, scrolling and image placement cleanup are separate concerns; `allow-passthrough` alone is insufficient evidence. Current tmux policy is **text**, with no settings mutation. The real tmux 3.6 test verifies that policy and document interaction, not image passthrough.

Direct SSH image transport uses inline PNG data rather than a local filename. End-to-end SSH graphics remains untested. Text over a normal UTF-8 PTY is the conservative path.

## Concurrency, cache and safety

The UI owns a versioned document model. A bounded job channel coalesces document changes. A worker thread calls the same executable as a disposable renderer process; each process has a wall-clock deadline and CPU/output limits. Superseded work is killed and waited for. Generation, block identity and source content are checked before applying results.

Successful diagrams are cached by full sanitized source (engine, image mode and font are fixed per worker instance), bounded to 24 entries / 32 MiB. Current-document unchanged diagrams retain their `Arc` result without another worker request. Last-good results attach to a heading plus block-kind ordinal, so renaming a heading or structurally moving diagrams can intentionally lose the fallback rather than promise a perfect identity match.

File reads are bounded and only accept regular UTF-8 files. The application executes its own renderer and, on explicit HTTP/HTTPS link activation, the OS browser launcher with a single URL argument. Document code and shell syntax are never executed. It has no network resource loader. Linux renderer address space is limited; macOS uses input/model/pixel/CPU/time/output bounds instead of imposing an unreliable Darwin virtual-memory ceiling.

Normal exit, input Ctrl+C, SIGINT, SIGTERM, SIGHUP and unwinding restore raw/alternate modes. SIGKILL and machine/terminal failure cannot be recovered by any user-space cleanup handler.

## Packaging

Rust 1.92, thin LTO, stripped release binary. macOS links normal OS libraries; Linux GNU builds require the platform's libc, not Node/Python/Chrome. Release archives have one executable per target plus license/docs files. CI uses [documented GitHub-hosted macOS/Linux runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners). Notarization, musl builds, Windows and package-manager formulas are outside this release.
