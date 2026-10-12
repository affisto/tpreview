# Mermaid support and known limits

## Supported families

`tpreview` enables **flowchart** (`graph` / `flowchart`) and **sequenceDiagram** using mmdflux 2.6.1. It does not claim full Mermaid.js conformance or identical geometry/fonts. Every block is validated before either text or SVG output. Engine warnings, including skipped sequence lines and permissive flowchart constructs, become visible errors.

| Family | Verified syntax |
| --- | --- |
| Flowchart | LR/TD examples; identifiers and bracket labels; decision diamonds; directed `-->` edges; edge labels `-->|text|`; branching/cycles; Korean/Japanese labels |
| Sequence | `participant` / `actor`, `as` aliases, `->>` request and `-->>` reply, Unicode messages, `Note over A,B:`, `autonumber`, `alt` / `else` / `end` |

These are the tested fixtures, not an exhaustive grammar specification. The engine can accept additional constructs in these two families, but they are not added to the verified list until covered by tests. The application does not implement an independent Mermaid regex parser.

Unsupported families (including pie/class/state/ER/Gantt), malformed fences' contents, unknown statements and engine warnings show a reason and source. `click` statements are rejected. Neither callbacks, JavaScript, shell commands nor browser interactions run.

Text output is a real relation/timeline rendering from the engine, not an image-to-character approximation. Wide diagrams pan horizontally. `--render image` uses the same validated semantics rendered to SVG, then PNG; missing image capabilities or raster errors retain readable text. Colors, routing, layout and typography may differ from the official renderer.

## Editing and identity

A valid diagram changed to an unfinished draft keeps its previous result, shows the old-result notice and current error, and includes the new source. With no valid prior result, source and error are shown alone. During background work, the caption reads `rendering…`.

Last-good identity is heading + diagram ordinal within that section. It is best effort: rename/move headings or reorder diagrams and the matching may reset. The content cache is independent of placement identity. On navigation, old document state is cleared; history retains document text and position, then watch reconciles the file.

## Resource bounds

| Resource | Limit |
| --- | --- |
| Document input | 2 MiB, 30,000 lines, UTF-8 regular file or stdin |
| Mermaid source | 16 KiB, 160 lines per block |
| Rendered diagrams | First 32 blocks per document; further blocks show source and reason |
| Flowchart model | 120 nodes / 240 edges |
| Sequence model | 40 participants / 160 events |
| Worker runtime | 5 seconds wall time; 4 seconds CPU |
| Worker output | 32 MiB |
| Linux worker address space | 768 MiB |
| PNG raster | Maximum side 4096 px and maximum 4 megapixels |
| Worker success cache | 24 entries / 32 MiB |
| Syntax highlighting | First 2,000 lines of a block, under 8,192 bytes per line; remaining code is readable plain text |
| Table padding | Above 8 MiB projected padding, show complete unaligned cell contents with a notice |
| Directory traversal | 16 levels, 5,000 Markdown entries, no followed symlinks |
| Back history | 32 documents |

Directory exclusions: `.git`, `node_modules`, `target`, `.venv`, `vendor`, `dist`. User tmux configuration is never changed. A file deleted or temporarily unreadable keeps the last document and reports the read error.

## Other reader limitations

- Sixel/iTerm2 image protocols, Windows, external high-fidelity renderers and remote document/image fetching are not implemented.
- Raw HTML is shown as inert text. Markdown images show URL/alt text; local raster images are not yet embedded.
- Inline emphasis/strong/strike delimiters are parsed away; content remains readable, without separate styling for those inline marks. Code blocks are highlighted; inline code is plain text.
- Table alignment uses display widths and preserves cell contents, but Markdown left/center/right alignment hints are not yet reflected.
- Search is case-insensitive, literal, per displayed row (plus source text for image diagrams), not regex or cross-wrap phrase search. Matches highlight whole rows.
- No mouse navigation, shell/editor launching, OSC hyperlinks, file editing, persistent settings, bookmarks or disk cache.
- CJK raster labels depend on available local CJK fonts; only Noto Sans Latin/general coverage is embedded. Text width assumes normal narrow ambiguous characters; some terminal emoji policies differ.
- Graphics updates re-transmit visible PNGs and delete prior owned IDs. Large diagrams over slow SSH can be expensive; use text mode there. Real terminal cleanup/placement needs the manual checklist.
