import { Compartment, EditorState } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter } from '@codemirror/view';
import { defaultKeymap, indentWithTab } from '@codemirror/commands';
import { bracketMatching, indentOnInput, indentUnit, syntaxHighlighting, HighlightStyle } from '@codemirror/language';
import { yaml } from '@codemirror/lang-yaml';
import { linter, lintGutter, type Diagnostic } from '@codemirror/lint';
import { tags } from '@lezer/highlight';
import { parseDocument } from 'yaml';

export function yamlDiagnostics(source: string): Diagnostic[] {
  // Parse the syntax tree only. Do not expand aliases or evaluate tagged values.
  const document = parseDocument(source, { prettyErrors: false });
  return [...document.errors, ...document.warnings].map(error => ({
    from: Math.max(0, Math.min(source.length, error.pos[0])),
    to: Math.max(0, Math.min(source.length, error.pos[1])),
    severity: error.name === 'YAMLParseError' ? 'error' : 'warning',
    message: error.message,
  }));
}

/** An offline YAML code editor. Document history is owned by the VS Code host. */
export class FrontmatterEditor {
  private view: EditorView;
  private editable = new Compartment();
  private disabled = false;
  private loading = false;
  private composing = false;
  private compositionTimer?: ReturnType<typeof setTimeout>;

  constructor(parent: HTMLElement, status: HTMLElement, onChange: (text: string, composing: boolean) => void, onComposition: (active: boolean) => void) {
    this.view = new EditorView({
      parent,
      state: EditorState.create({ extensions: [
        yaml(), lineNumbers(), highlightActiveLine(), highlightActiveLineGutter(),
        bracketMatching(), indentOnInput(), indentUnit.of('  '),
        keymap.of([...defaultKeymap, indentWithTab]),
        this.editable.of([EditorState.readOnly.of(false), EditorView.editable.of(true)]),
        EditorView.contentAttributes.of({ 'aria-label': 'YAML frontmatter', spellcheck: 'false' }),
        syntaxHighlighting(HighlightStyle.define([
          { tag: tags.propertyName, color: 'var(--vscode-symbolIcon-propertyForeground, #9cdcfe)' },
          { tag: tags.string, color: 'var(--vscode-debugTokenExpression-string, #ce9178)' },
          { tag: [tags.number, tags.bool, tags.null], color: 'var(--vscode-debugTokenExpression-number, #b5cea8)' },
          { tag: tags.comment, color: 'var(--vscode-descriptionForeground, #8b949e)', fontStyle: 'italic' },
          { tag: [tags.meta, tags.keyword], color: 'var(--vscode-symbolIcon-keywordForeground, #c586c0)' },
        ])),
        linter(view => {
          const diagnostics = yamlDiagnostics(view.state.doc.toString());
          const first = diagnostics[0];
          status.textContent = first ? `Line ${view.state.doc.lineAt(first.from).number}: ${first.message}` : 'YAML · Tab to indent · ⌘/Ctrl S to save';
          status.classList.toggle('yaml-invalid', diagnostics.some(d => d.severity === 'error'));
          return diagnostics;
        }, { delay: 250 }), lintGutter(),
        EditorView.updateListener.of(update => {
          if (update.docChanged && !this.loading) onChange(update.state.doc.toString(), this.composing || update.view.composing);
        }),
        EditorView.domEventHandlers({
          compositionstart: () => { this.composing = true; onComposition(true); return false; },
          compositionend: () => {
            clearTimeout(this.compositionTimer);
            this.compositionTimer = setTimeout(() => {
              this.composing = false; onComposition(false);
              onChange(this.view.state.doc.toString(), false);
            }, 0);
            return false;
          },
        }),
      ] }),
    });
  }
  setText(text: string) {
    const normalized = text.replace(/\r\n/g, '\n');
    if (this.view.state.doc.toString() === normalized) return;
    this.loading = true;
    const selection = Math.min(this.view.state.selection.main.head, normalized.length);
    try { this.view.dispatch({ changes: { from: 0, to: this.view.state.doc.length, insert: normalized }, selection: { anchor: selection } }); }
    finally { this.loading = false; }
  }
  setDisabled(disabled: boolean) {
    if (disabled === this.disabled) return;
    this.disabled = disabled;
    this.view.dispatch({ effects: this.editable.reconfigure([EditorState.readOnly.of(disabled), EditorView.editable.of(!disabled)]) });
  }
  focus() { this.view.focus(); }
  destroy() { clearTimeout(this.compositionTimer); this.view.destroy(); }
}
