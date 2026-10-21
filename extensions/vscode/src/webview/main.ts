import { Editor, defaultValueCtx, editorViewCtx, rootCtx, serializerCtx, remarkStringifyOptionsCtx } from '@milkdown/kit/core';
import { commonmark, toggleStrongCommand, toggleEmphasisCommand, toggleInlineCodeCommand, wrapInHeadingCommand, turnIntoTextCommand, wrapInBulletListCommand, wrapInBlockquoteCommand, toggleLinkCommand } from '@milkdown/kit/preset/commonmark';
import { gfm, toggleStrikethroughCommand, insertTableCommand } from '@milkdown/kit/preset/gfm';
import { clipboard } from '@milkdown/kit/plugin/clipboard';
import { indent } from '@milkdown/kit/plugin/indent';
import { trailing } from '@milkdown/kit/plugin/trailing';
import { $prose, callCommand, getMarkdown, insert, replaceAll } from '@milkdown/kit/utils';
import { Plugin } from '@milkdown/kit/prose/state';
import { gapCursor } from '@milkdown/kit/prose/gapcursor';
import type { ClientMessage, HostMessage } from '../protocol';
import { DocumentSync } from './sync';
import { reconcileMarkdown, sameMeaning, unsupportedReason, splitFrontmatter, withFrontmatter, replaceYamlFrontmatter, addYamlFrontmatter } from './markdown';
import { FrontmatterEditor } from './frontmatter';
import { headingSlug } from '../links';
import { codeView, imageView, taskView } from './views';
import './style.css';

declare function acquireVsCodeApi(): { postMessage(message: ClientMessage): void; getState(): { draft?: string; base?: string; uri?: string; metadataDraft?: string } | undefined; setState(state: unknown): void };
const vscode = acquireVsCodeApi();
const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const post = (message: ClientMessage) => vscode.postMessage(message);
const sync = new DocumentSync(post);
let editor: Editor | undefined, suppress = false, baseline = '', original = '', baseUri = '', name = '';
let blocked = false, ready = false, composing = false, waitingAction: 'source' | 'save' | 'undo' | 'redo' | undefined;
let messages = Promise.resolve();
let pendingLink: string | undefined;
let metadataEditor: FrontmatterEditor | undefined, metadataDraft: string | undefined;
let metadataError = '', metadataEditable = false;

function notice(message: string) { $('notice').textContent = message; $('notice').hidden = !message; }
function persist() { vscode.setState(sync.settled && metadataDraft === undefined ? {} : { draft: sync.draft, base: sync.current.text, uri: baseUri + name, metadataDraft }); }
function state() {
  if (sync.conflict) { pendingLink = undefined; waitingAction = undefined; }
  $('status').textContent = sync.conflict ? 'Draft preserved · resolve conflict' : metadataError ? 'YAML draft kept · check delimiters' : sync.pending ? 'Updating document…' : blocked ? 'Source editing available' : 'Changes applied to document';
  $('conflict').hidden = !sync.conflict;
  ($('reload') as HTMLButtonElement).disabled = !!sync.pending;
  if (sync.conflict) { $('conflict-message').textContent = 'The file changed while you were editing. Your draft is kept below. Copy it before using the file version.'; $('draft').textContent = sync.draft; ($('draft') as HTMLTextAreaElement).value = sync.draft; }
  $('toolbar').querySelectorAll<HTMLButtonElement | HTMLSelectElement>('button, select').forEach(button => button.disabled = blocked || sync.conflict || !ready);
  editor?.action(ctx => ctx.get(editorViewCtx).setProps({ editable: () => !blocked && !sync.conflict }));
  metadataEditor?.setDisabled(!metadataEditable || sync.conflict || !ready);
  ($('add-frontmatter') as HTMLButtonElement).disabled = blocked || sync.conflict || !ready || composing;
  $('metadata-error').textContent = metadataError; $('metadata-error').hidden = !metadataError;
  persist();
  if (pendingLink !== undefined && sync.settled && !composing && !metadataError) { const href = pendingLink; pendingLink = undefined; post({ type: 'link', href }); }
  if (waitingAction && sync.settled && !composing && !metadataError) { const type = waitingAction; waitingAction = undefined; post({ type }); }
}
function metadataChanged(text: string, isComposing: boolean) {
  if (suppress || !ready || sync.conflict) return;
  metadataDraft = text;
  if (isComposing) { persist(); return; }
  try {
    const next = replaceYamlFrontmatter(original, text);
    metadataError = ''; metadataDraft = undefined; original = next;
    if (blocked) $('raw').textContent = next;
    sync.edit(next);
  } catch (error) { metadataError = error instanceof Error ? error.message : 'Unable to update YAML frontmatter.'; }
  state();
}
function localChanged(markdown: string) {
  if (suppress || !ready || blocked || sync.conflict || composing) return;
  try {
    const text = reconcileMarkdown(original, baseline, markdown);
    original = text; baseline = markdown;
    sync.edit(text); state();
  } catch (error) {
    sync.draft = withFrontmatter(original, markdown); sync.conflict = true;
    notice(error instanceof Error ? error.message : 'Unable to preserve Markdown. Copy your draft.'); state();
  }
}
const transactions = $prose(ctx => new Plugin({
  view: () => ({ update(view, previous) {
    const parent = view.state.selection.$from.parent;
    ($('block') as HTMLSelectElement).value = parent.type.name === 'heading' ? String(Math.min(3, parent.attrs.level)) : '0';
    for (const [command, mark] of Object.entries({ bold: 'strong', italic: 'emphasis', strike: 'strike_through', code: 'inlineCode' })) {
      const active = (view.state.storedMarks ?? view.state.selection.$from.marks()).some(m => m.type.name === mark);
      document.querySelector(`[data-command="${command}"]`)?.setAttribute('aria-pressed', String(active));
    }
    if (!view.state.doc.eq(previous.doc)) {
      const markdown = ctx.get(serializerCtx)(view.state.doc);
      if ((composing || view.composing) && ready && !suppress && !blocked && !sync.conflict) {
        // Track the IME draft without shipping intermediate syllables to the host.
        // This also makes an external update conflict instead of discarding composition.
        try { sync.draft = reconcileMarkdown(original, baseline, markdown); }
        catch { sync.draft = withFrontmatter(original, markdown); }
        persist();
      } else localChanged(markdown);
    }
  } }),
  props: { handleDOMEvents: {
    compositionstart: () => { composing = true; return false; },
    compositionend: () => { composing = false; setTimeout(() => { if (editor) localChanged(editor.action(getMarkdown())); }, 0); return false; },
  } },
}));

async function load(text: string) {
  suppress = true; blocked = false; ready = false; composing = false;
  original = text; metadataDraft = undefined; metadataError = '';
  notice('');
  const reason = unsupportedReason(text);
  const { body, metadata, format } = splitFrontmatter(text);
  metadataEditable = format === 'YAML' && text.length <= 1_000_000;
  $('frontmatter').hidden = !format;
  $('add-frontmatter').hidden = !!format || !!reason;
  $('frontmatter-label').textContent = `Frontmatter · ${format ?? ''}`;
  $('metadata').hidden = !metadataEditable; $('yaml-status').hidden = !metadataEditable;
  $('metadata-readonly').hidden = metadataEditable; $('metadata-readonly').textContent = metadata;
  if (metadataEditable) {
    metadataEditor ??= new FrontmatterEditor($('metadata'), $('yaml-status'), metadataChanged, active => { composing = active; });
    metadataEditor.setText(metadata);
  }
  $('fallback').hidden = !reason; $('editor').hidden = !!reason;
  if (reason) {
    blocked = true; $('fallback-reason').textContent = reason; $('raw').textContent = text;
  } else {
    try {
      if (!editor) {
        editor = await Editor.make().config(ctx => {
          ctx.set(rootCtx, $('editor')); ctx.set(defaultValueCtx, body);
          ctx.update(remarkStringifyOptionsCtx, options => ({ ...options, bullet: '-' as const, fences: true }));
        })
          .use(commonmark).use(gfm).use(clipboard).use(indent).use(trailing)
          .use($prose(() => gapCursor())).use(transactions).use(codeView).use(taskView).use(imageView(() => baseUri)).create();
      } else editor.action(replaceAll(body));
      baseline = editor.action(getMarkdown()); original = text;
      if (!sameMeaning(body, baseline)) {
        blocked = true; $('fallback').hidden = false; $('editor').hidden = true;
        $('fallback-reason').textContent = 'This document contains formatting the visual editor cannot safely round-trip. The source editor preserves it.'; $('raw').textContent = text;
      }
    } catch (error) {
      blocked = true; $('fallback').hidden = false; $('editor').hidden = true;
      $('fallback-reason').textContent = 'This document could not be opened visually. Continue editing its original source.'; $('raw').textContent = text;
      console.error(error);
    }
  }
  suppress = false; ready = true; state();
}

window.addEventListener('message', event => {
  const message = event.data as HostMessage;
  messages = messages.then(async () => {
    if (message.type === 'document') {
      const first = !sync.current.version;
      const recovery = first ? vscode.getState() : undefined;
      baseUri = message.baseUri; name = message.name; $('filename').textContent = name;
      if (metadataDraft !== undefined && message.version >= sync.current.version && message.text !== sync.current.text) {
        sync.draft = replaceYamlFrontmatter(original, metadataDraft, true); sync.conflict = true;
      }
      const action = sync.receive(message);
      if (first || action === 'replace') await load(message.text);
      if (recovery?.draft && recovery.uri === baseUri + name && recovery.draft !== message.text) {
        sync.draft = recovery.draft; sync.conflict = true;
      }
      if (recovery?.metadataDraft !== undefined && recovery.uri === baseUri + name) {
        sync.draft = replaceYamlFrontmatter(recovery.draft ?? message.text, recovery.metadataDraft, true); sync.conflict = true;
      }
      state();
    } else if (message.type === 'ack') {
      sync.acknowledge(message.id, message.accepted, message); state();
    } else if (message.type === 'error') {
      notice(message.message);
      if (sync.pending) { sync.pending = undefined; sync.conflict = true; state(); }
    } else if (message.type === 'anchor') {
      jumpToHeading(message.fragment);
    }
  }).catch(error => notice(String(error)));
});

function hostAction(action: 'source' | 'save' | 'undo' | 'redo') {
  if (composing) { waitingAction = action; return; }
  if (metadataError) {
    if (action === 'undo') {
      metadataDraft = undefined; metadataError = ''; metadataEditor?.setText(splitFrontmatter(original).metadata); state();
    } else notice('Correct the frontmatter delimiters before continuing. Your YAML draft is kept in the editor.');
    return;
  }
  if (editor && !blocked && !sync.conflict) localChanged(editor.action(getMarkdown()));
  if (sync.conflict) { notice('Copy your draft and use the file version before continuing.'); return; }
  waitingAction = action; sync.flush(); state();
}
for (const action of ['source', 'save', 'undo', 'redo'] as const) $(action).addEventListener('click', () => hostAction(action));
$('fallback-source').addEventListener('click', () => hostAction('source'));
$('frontmatter-source').addEventListener('click', () => hostAction('source'));
$('add-frontmatter').addEventListener('click', async () => {
  if (blocked || sync.conflict || !ready || composing) return;
  if (editor) localChanged(editor.action(getMarkdown()));
  if (sync.conflict) return;
  const text = addYamlFrontmatter(original);
  sync.edit(text); await load(text); metadataEditor?.focus();
});
window.addEventListener('unload', () => metadataEditor?.destroy());
$('reload').addEventListener('click', async () => { sync.reload(); await load(sync.current.text); });
$('copy').addEventListener('click', () => {
  const draft = $('draft') as HTMLTextAreaElement; draft.focus(); draft.select();
  const copied = document.execCommand('copy'); notice(copied ? 'Draft copied.' : 'Draft selected. Press ⌘/Ctrl C to copy.');
});
document.addEventListener('keydown', event => {
  if (!(event.metaKey || event.ctrlKey) || event.isComposing) return;
  const key = event.key.toLowerCase();
  if (key === 's' || key === 'z' || key === 'y') {
    event.preventDefault(); event.stopPropagation();
    hostAction(key === 's' ? 'save' : key === 'y' || event.shiftKey ? 'redo' : 'undo');
  }
}, true);
// Keep the ProseMirror selection while using toolbar buttons.
$('toolbar').addEventListener('mousedown', event => { if ((event.target as HTMLElement).closest('button')) event.preventDefault(); });
$('toolbar').addEventListener('click', event => {
  const command = (event.target as HTMLElement).closest('button')?.dataset.command;
  if (!editor || blocked || sync.conflict) return;
  const commands = { bold: toggleStrongCommand.key, italic: toggleEmphasisCommand.key, strike: toggleStrikethroughCommand.key, code: toggleInlineCodeCommand.key, list: wrapInBulletListCommand.key, quote: wrapInBlockquoteCommand.key };
  if (command && command in commands) editor.action(callCommand(commands[command as keyof typeof commands]));
  if (command === 'table') editor.action(callCommand(insertTableCommand.key, { row: 3, col: 3 }));
  if (command === 'task') editor.action(insert('- [ ] Task'));
  if (command === 'mermaid') editor.action(insert('```mermaid\nflowchart LR\n    Draft --> Review\n    Review --> Publish\n```\n'));
  if (command === 'link') { ($('link-dialog') as HTMLDialogElement).showModal(); ($('link-url') as HTMLInputElement).focus(); return; }
  editor.action(ctx => ctx.get(editorViewCtx).focus());
});
$('block').addEventListener('change', event => {
  if (!editor || blocked || sync.conflict) return;
  const level = Number((event.target as HTMLSelectElement).value);
  editor.action(level ? callCommand(wrapInHeadingCommand.key, level) : callCommand(turnIntoTextCommand.key));
  editor.action(ctx => ctx.get(editorViewCtx).focus());
});
$('link-dialog').addEventListener('close', () => {
  if (($('link-dialog') as HTMLDialogElement).returnValue !== 'insert' || !editor) return;
  const href = ($('link-url') as HTMLInputElement).value.trim();
  if (/^(?:javascript|data|command|vscode):/i.test(href)) { notice('Use an HTTPS link or a relative Markdown file path.'); return; }
  editor.action(callCommand(toggleLinkCommand.key, { href })); editor.action(ctx => ctx.get(editorViewCtx).focus());
});
$('editor').addEventListener('click', event => {
  const link = (event.target as HTMLElement).closest('a');
  if (link) {
    event.preventDefault();
    event.stopPropagation();
    if (event.altKey) return;
    if (composing || sync.conflict || metadataError) { notice('Finish editing or resolve the draft conflict before opening a link.'); return; }
    if (editor && !blocked) localChanged(editor.action(getMarkdown()));
    sync.flush();
    if (!sync.settled) { pendingLink = link.getAttribute('href') ?? ''; return; }
    post({ type: 'link', href: link.getAttribute('href') ?? '' });
  }
}, true);
function jumpToHeading(fragment: string) {
  if (!fragment) { window.scrollTo(0, 0); return; }
  const counts = new Map<string, number>();
  for (const heading of $('editor').querySelectorAll<HTMLElement>('h1, h2, h3, h4, h5, h6')) {
    const base = headingSlug(heading.textContent ?? '');
    const count = counts.get(base) ?? 0; counts.set(base, count + 1);
    const slug = count ? `${base}-${count}` : base;
    if (slug === fragment) {
      heading.scrollIntoView({ block: 'start' });
      heading.setAttribute('tabindex', '-1'); heading.focus({ preventScroll: true });
      notice(''); return;
    }
  }
  notice(`Heading not found: ${fragment}`);
}
post({ type: 'ready' });
