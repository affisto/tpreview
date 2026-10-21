function escape(value: string) { return value.replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!); }
export function editorHtml(script: string, style: string, cspSource: string, nonce: string) {
  return `<!doctype html><html lang="en"><head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src ${escape(cspSource)} 'nonce-${escape(nonce)}'; style-src ${escape(cspSource)} 'unsafe-inline'; img-src ${escape(cspSource)} data:; font-src ${escape(cspSource)}; connect-src 'none';">
<link rel="stylesheet" href="${escape(style)}"><title>MD Visual Editor</title></head><body>
<header><div class="identity"><span class="brand">MD Visual Editor</span><span id="filename"></span></div>
<div class="actions"><button id="undo" title="Undo (⌘/Ctrl Z)" aria-label="Undo">↶</button><button id="redo" title="Redo (⌘/Ctrl Shift Z)" aria-label="Redo">↷</button><button id="source" title="Open Markdown source">&lt;/&gt; Source</button><button id="save">Save</button></div></header>
<nav id="toolbar" aria-label="Markdown formatting">
<select id="block" aria-label="Text style"><option value="0">Paragraph</option><option value="1">Heading 1</option><option value="2">Heading 2</option><option value="3">Heading 3</option></select>
<button data-command="bold" title="Bold (⌘/Ctrl B)" aria-label="Bold"><strong>B</strong></button>
<button data-command="italic" title="Italic (⌘/Ctrl I)" aria-label="Italic"><em>I</em></button>
<button data-command="strike" title="Strikethrough" aria-label="Strikethrough"><s>S</s></button>
<button data-command="code" title="Inline code" aria-label="Inline code">&lt;/&gt;</button>
<span class="divider"></span><button data-command="list">List</button><button data-command="task">Task</button><button data-command="quote">Quote</button><button data-command="link">Link</button>
<span class="divider"></span><button data-command="table">Table</button><button data-command="mermaid">Diagram</button>
</nav>
<div id="notice" role="status" hidden></div>
<section id="conflict" aria-label="Unsaved draft" hidden><p id="conflict-message"></p><div class="actions"><button id="copy">Copy my draft</button><button id="reload">Use file version</button></div><textarea id="draft" aria-label="Your unsaved Markdown draft" readonly></textarea></section>
<main><button id="add-frontmatter" hidden>Add YAML frontmatter</button><details id="frontmatter" open hidden><summary id="frontmatter-label">Frontmatter</summary><div id="metadata"></div><pre id="metadata-readonly" hidden></pre><p id="yaml-status" role="status">YAML · Tab to indent · ⌘/Ctrl S to save</p><p id="metadata-error" role="alert" hidden></p><button id="frontmatter-source">Open metadata source</button></details><div id="editor" aria-label="Visual Markdown editor"></div><section id="fallback" hidden><h1>Continue in the source editor</h1><p id="fallback-reason"></p><button id="fallback-source">Open source</button><pre id="raw"></pre></section></main>
<footer><span id="status" role="status">Opening document…</span><span>Markdown · Visual editor</span></footer>
<dialog id="link-dialog"><form method="dialog"><label for="link-url">Link URL</label><input id="link-url" type="text" placeholder="https://… or ./document.md" required><div class="actions"><button value="cancel" formnovalidate>Cancel</button><button value="insert">Insert link</button></div></form></dialog>
<script type="module" nonce="${escape(nonce)}" src="${escape(script)}"></script></body></html>`;
}
