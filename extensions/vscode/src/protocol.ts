export type Snapshot = { text: string; version: number };
export type Edit = { type: 'edit'; id: number; version: number; text: string };
export type ClientMessage = Edit | { type: 'ready' | 'source' | 'save' | 'undo' | 'redo' } | { type: 'link'; href: string };
export type HostMessage =
  | ({ type: 'document'; baseUri: string; name: string } & Snapshot)
  | ({ type: 'ack'; id: number; accepted: boolean } & Snapshot)
  | { type: 'anchor'; fragment: string }
  | { type: 'error'; message: string };

export function isClientMessage(value: unknown): value is ClientMessage {
  if (!value || typeof value !== 'object') return false;
  const m = value as Record<string, unknown>;
  if (m.type === 'edit') return Number.isSafeInteger(m.id) && Number.isSafeInteger(m.version) && typeof m.text === 'string';
  if (m.type === 'link') return typeof m.href === 'string';
  return ['ready', 'source', 'save', 'undo', 'redo'].includes(String(m.type));
}

/** One contiguous change, using UTF-16 offsets as required by VS Code. */
export function textChange(before: string, after: string) {
  let start = 0;
  while (start < before.length && start < after.length && before[start] === after[start]) start++;
  // Never split a surrogate pair or a CRLF at either boundary.
  if (start > 0 && (/^[\uDC00-\uDFFF]$/.test(before[start] ?? '') || (before[start - 1] === '\r' && before[start] === '\n'))) start--;
  let oldEnd = before.length, newEnd = after.length;
  while (oldEnd > start && newEnd > start && before[oldEnd - 1] === after[newEnd - 1]) { oldEnd--; newEnd--; }
  if (oldEnd < before.length && (/^[\uDC00-\uDFFF]$/.test(before[oldEnd]) || (before[oldEnd - 1] === '\r' && before[oldEnd] === '\n'))) { oldEnd++; newEnd++; }
  return { start, end: oldEnd, text: after.slice(start, newEnd) };
}
