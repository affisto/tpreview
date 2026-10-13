import type { Edit, Snapshot } from '../protocol';

/** Serialize edits; never apply a draft to a document version it did not start from. */
export class DocumentSync {
  current: Snapshot = { text: '', version: 0 };
  draft = '';
  pending?: Edit;
  conflict = false;
  private sequence = 0;
  constructor(private send: (edit: Edit) => void) {}
  get settled() { return !this.pending && !this.conflict && this.draft === this.current.text; }
  edit(text: string) { this.draft = text; this.flush(); }
  flush() {
    if (this.conflict || this.pending || this.draft === this.current.text) return;
    this.pending = { type: 'edit', id: ++this.sequence, version: this.current.version, text: this.draft };
    this.send(this.pending);
  }
  receive(state: Snapshot): 'replace' | 'keep' | 'conflict' {
    if (state.version < this.current.version) return 'keep';
    if (state.version === this.current.version && state.text === this.current.text) return 'keep';
    const clean = this.settled;
    this.current = state;
    if (clean) { this.draft = state.text; return 'replace'; }
    if (state.text === this.draft) return 'keep';
    this.conflict = true;
    return 'conflict';
  }
  acknowledge(id: number, accepted: boolean, state: Snapshot) {
    if (this.pending?.id !== id) return;
    const expected = this.pending.text;
    this.pending = undefined;
    if (state.version >= this.current.version) this.current = state;
    if (!accepted || state.text !== expected) this.conflict = true;
    this.flush();
  }
  reload() {
    this.pending = undefined;
    this.conflict = false;
    this.draft = this.current.text;
  }
}
