import { unified } from 'unified';
import remarkParse from 'remark-parse';
import remarkGfm from 'remark-gfm';
import { diffArrays } from 'diff';

const parser = unified().use(remarkParse).use(remarkGfm);
type Node = { type: string; position?: { start: { offset?: number }; end: { offset?: number } }; children?: Node[]; [key: string]: unknown };
const supported = new Set(['root', 'paragraph', 'text', 'heading', 'emphasis', 'strong', 'delete', 'inlineCode', 'code', 'blockquote', 'list', 'listItem', 'link', 'image', 'thematicBreak', 'break', 'table', 'tableRow', 'tableCell']);
function nodes(source: string): Node[] { return parser.parse(source).children as Node[]; }
export function fingerprint(node: Node): string {
  return JSON.stringify(node, (key, value) => ['position', 'data', 'spread'].includes(key) ? undefined : value);
}
export function unsupportedReason(source: string): string | undefined {
  if (source.length > 1_000_000) return 'This file is over 1 MB. Use the source editor.';
  if (/^(?:\uFEFF)?(?:---|\+\+\+)\r?\n/.test(source)) return 'Front matter is preserved in the source editor.';
  let reason: string | undefined;
  function visit(node: Node) {
    if (!supported.has(node.type)) reason ??= `${node.type} syntax is preserved in the source editor.`;
    if (node.type === 'code' && node.meta) reason ??= 'Code fence metadata is preserved in the source editor.';
    if (node.type === 'code' && /\r/.test(String(node.value))) reason ??= 'Use the source editor for this code block.';
    node.children?.forEach(visit);
  }
  nodes(source).forEach(visit);
  return reason;
}
export function sameMeaning(a: string, b: string): boolean {
  return JSON.stringify(nodes(a).map(fingerprint)) === JSON.stringify(nodes(b).map(fingerprint));
}

/** Preserve the exact spelling of unchanged top-level blocks and their gaps. */
export function reconcileMarkdown(original: string, baseline: string, edited: string): string {
  if (baseline === edited) return original;
  const raw = nodes(original), before = nodes(baseline), after = nodes(edited);
  if (!sameMeaning(original, baseline)) throw new Error('This syntax needs the source editor to preserve its content.');
  const oldKeys = before.map(fingerprint), newKeys = after.map(fingerprint);
  const chunks: { text: string; originalIndex?: number }[] = [];
  let oldIndex = 0, newIndex = 0;
  for (const part of diffArrays(oldKeys, newKeys)) {
    for (let i = 0; i < part.value.length; i++) {
      if (part.removed) { oldIndex++; continue; }
      if (part.added) {
        const node = after[newIndex++];
        chunks.push({ text: edited.slice(node.position!.start.offset, node.position!.end.offset) });
      } else {
        const node = raw[oldIndex];
        chunks.push({ text: original.slice(node.position!.start.offset, node.position!.end.offset), originalIndex: oldIndex });
        oldIndex++; newIndex++;
      }
    }
  }
  const eol = original.includes('\r\n') ? '\r\n' : '\n';
  let result = raw.length ? original.slice(0, raw[0].position!.start.offset) : '';
  chunks.forEach((chunk, index) => {
    result += chunk.originalIndex === undefined ? chunk.text.replace(/\r?\n/g, eol) : chunk.text;
    const next = chunks[index + 1];
    if (next) {
      const n = chunk.originalIndex;
      result += n !== undefined && n + 1 < raw.length
        ? original.slice(raw[n].position!.end.offset, raw[n + 1].position!.start.offset)
        : eol + eol;
    }
  });
  const trailing = raw.length ? original.slice(raw.at(-1)!.position!.end.offset) : '';
  result += chunks.length ? trailing : '';
  // Boundary changes can alter Markdown meaning (e.g. adjacent lists). Never silently save that.
  if (!sameMeaning(result, edited)) throw new Error('This structural change needs the source editor. Your draft is available to copy.');
  return result;
}
