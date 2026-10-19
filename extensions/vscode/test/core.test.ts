import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reconcileMarkdown, sameMeaning, unsupportedReason, splitFrontmatter } from '../src/webview/markdown';
import { linkTarget, headingSlug } from '../src/links';
import { textChange, isClientMessage, type Edit } from '../src/protocol';
import { DocumentSync } from '../src/webview/sync';

test('editing one block preserves unrelated spelling, whitespace, and final newline', () => {
  const original = '# Title #\n\n\nA **bold** paragraph.\n\n* first\n* second\n';
  const canonical = '# Title\n\nA **bold** paragraph.\n\n- first\n- second\n';
  const edited = canonical.replace('A **bold**', 'An **important**');
  assert.equal(reconcileMarkdown(original, canonical, edited), original.replace('A **bold**', 'An **important**'));
});
test('selection-only updates preserve the entire document byte for byte', () => {
  const raw = 'Title\n=====\n\n* hello\n';
  const canonical = '# Title\n\n- hello\n';
  assert.equal(reconcileMarkdown(raw, canonical, canonical), raw);
});
test('CRLF, emoji and no final newline survive edits', () => {
  const raw = '# 안녕 😀\r\n\r\n* one\r\n* two';
  const canonical = '# 안녕 😀\n\n* one\n* two\n';
  assert.equal(reconcileMarkdown(raw, canonical, canonical.replace('안녕', '반가워')), raw.replace('안녕', '반가워'));
});
test('insertions, deletion, task lists, tables and repeated blocks preserve meaning', () => {
  for (const [before, after] of [
    ['# Hello\n\nA\n\nA\n', '# Hello\n\nA\n\nB\n\nA\n'],
    ['# Hello\n\nA\n', 'A\n'],
    ['- [ ] One\n', '- [x] One\n'],
    ['| A | B |\n| - | - |\n| C | D |\n', '| A | B |\n| - | - |\n| C | E |\n'],
    ['', '# New\n'], ['Hello\n', ''],
  ]) assert.ok(sameMeaning(reconcileMarkdown(before, before, after), after));
});
test('lossy and unsupported input is never silently rewritten', () => {
  for (const raw of ['---\ntitle: Unclosed\n', '<div>HTML</div>', '[x][id]\n\n[id]: https://example.com', '```js title=demo\ncode\n```', 'A[^1]\n\n[^1]: Footnote']) assert.ok(unsupportedReason(raw));
  assert.equal(unsupportedReason('# Title\n\n- [ ] Task\n\n```mermaid\nflowchart LR\nA-->B\n```'), undefined);
  assert.throws(() => reconcileMarkdown('<span>important</span>', 'important', 'changed'));
});
test('frontmatter remains byte exact through body edits, including BOM, CRLF and empty bodies', () => {
  for (const prefix of ['---\ntitle: "한글"\n# comment\nsummary: |\n  one\n  two\n---\n', '\uFEFF---\r\ntitle: Keep\r\n...\r\n', '+++\ntitle = "Keep"\n+++\n']) {
    const body = prefix.includes('\r\n') ? '\r\n# Hello #\r\n\r\nBody\r\n' : '\n# Hello #\n\nBody\n';
    const raw = prefix + body;
    const baseline = '# Hello\n\nBody\n';
    assert.equal(unsupportedReason(raw), undefined);
    assert.equal(splitFrontmatter(raw).prefix, prefix);
    assert.equal(reconcileMarkdown(raw, baseline, baseline), raw);
    assert.equal(reconcileMarkdown(raw, baseline, baseline.replace('Body', 'Edited')), raw.replace('Body', 'Edited'));
    assert.equal(reconcileMarkdown(prefix, '', '# New\n'), prefix + '# New');
  }
  assert.equal(reconcileMarkdown('---\ntitle: Keep\n---', '', '# New\n'), '---\ntitle: Keep\n---\n# New');
  assert.equal(splitFrontmatter('# Body\n\n---\n').format, undefined);
  assert.equal(unsupportedReason('---\ntitle: Keep\n---\n<div>HTML</div>'), 'html syntax is preserved in the source editor.');
});
test('link destinations distinguish browser URLs, absolute paths, encoded filenames and anchors', () => {
  assert.deepEqual(linkTarget('https://example.com/a?q=b#c'), { kind: 'web', url: 'https://example.com/a?q=b#c' });
  assert.deepEqual(linkTarget('//example.com/a'), { kind: 'web', url: 'https://example.com/a' });
  for (const [href, path] of [['../다음%20문서.md#제목', '../다음 문서.md'], ['/tmp/next.md#제목', '/tmp/next.md'], ['C:\\Docs\\next.md#제목', 'C:\\Docs\\next.md'], ['file:///tmp/next.md#제목', '/tmp/next.md']]) {
    assert.deepEqual(linkTarget(href), { kind: 'document', path, fragment: '제목' });
  }
  assert.deepEqual(linkTarget('#%ED%95%9C%EA%B8%80'), { kind: 'document', path: '', fragment: '한글' });
  assert.equal(headingSlug('한글 제목! Hello'), '한글-제목-hello');
  for (const href of ['javascript:alert(1)', 'command:workbench.action.closeWindow', 'data:text/html,x', 'vscode://extension', '%zz.md', 'a%00.md', 'file://remote/share.md']) assert.throws(() => linkTarget(href));
});
test('UTF-16 text edits reconstruct Unicode and CRLF safely', () => {
  for (const [a, b] of [['hello 😀 world', 'hello 😎 world'], ['a\r\nb', 'a\nb'], ['a\nb', 'a\r\nb'], ['', 'new'], ['old', ''], ['한글', '한국어']]) {
    const change = textChange(a, b);
    assert.equal(a.slice(0, change.start) + change.text + a.slice(change.end), b);
  }
  assert.equal(isClientMessage({ type: 'edit', id: 1, version: 2, text: 'ok' }), true);
  assert.equal(isClientMessage({ type: 'edit', id: 1, version: 'bad', text: 'ok' }), false);
});
test('rapid typing queues the latest draft against the acknowledged version', () => {
  const sent: Edit[] = [], sync = new DocumentSync(edit => sent.push(edit));
  sync.receive({ text: 'A', version: 1 });
  sync.edit('AB'); sync.edit('ABC'); sync.edit('ABCD');
  assert.equal(sent.length, 1);
  sync.acknowledge(sent[0].id, true, { text: 'AB', version: 2 });
  assert.equal(sent.length, 2); assert.equal(sent[1].text, 'ABCD'); assert.equal(sent[1].version, 2);
  sync.acknowledge(sent[1].id, true, { text: 'ABCD', version: 3 });
  assert.ok(sync.settled);
});
test('external edits and stale acknowledgments cannot overwrite a local draft', () => {
  const sent: Edit[] = [], sync = new DocumentSync(edit => sent.push(edit));
  sync.receive({ text: 'A', version: 1 }); sync.edit('AB');
  assert.equal(sync.receive({ text: 'external', version: 3 }), 'conflict');
  sync.acknowledge(sent[0].id, true, { text: 'AB', version: 2 });
  assert.equal(sync.current.text, 'external'); assert.equal(sync.draft, 'AB'); assert.ok(sync.conflict);
  sync.reload(); assert.equal(sync.draft, 'external'); assert.ok(sync.settled);
});
test('rejected edits retain the draft and clean external changes replace the view', () => {
  const sync = new DocumentSync(() => {});
  sync.receive({ text: 'A', version: 1 });
  assert.equal(sync.receive({ text: 'B', version: 2 }), 'replace');
  sync.edit('BC'); sync.acknowledge(sync.pending!.id, false, { text: 'other', version: 3 });
  assert.ok(sync.conflict); assert.equal(sync.draft, 'BC');
});
