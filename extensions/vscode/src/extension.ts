import * as vscode from 'vscode';
import { randomBytes } from 'node:crypto';
import { posix } from 'node:path';
import { linkTarget } from './links';
import { editorHtml } from './html';
import { isClientMessage, textChange, type HostMessage, type Snapshot } from './protocol';

export const viewType = 'tpreview.markdown';
export function activate(context: vscode.ExtensionContext) {
  const provider = new MarkdownEditor(context.extensionUri);
  context.subscriptions.push(vscode.window.registerCustomEditorProvider(viewType, provider, {
    supportsMultipleEditorsPerDocument: true,
    webviewOptions: { retainContextWhenHidden: true },
  }), vscode.commands.registerCommand('tpreview.openEditor', async (uri?: vscode.Uri) => {
    const tab = vscode.window.tabGroups.activeTabGroup.activeTab?.input;
    const target = uri ?? vscode.window.activeTextEditor?.document.uri
      ?? (tab instanceof vscode.TabInputText || tab instanceof vscode.TabInputCustom ? tab.uri : undefined);
    if (target) await vscode.commands.executeCommand('vscode.openWith', target, viewType);
    else void vscode.window.showInformationMessage('Open a Markdown file first.');
  }));
}

class MarkdownEditor implements vscode.CustomTextEditorProvider {
  private queues = new Map<string, Promise<void>>();
  private previews = new Map<string, Set<{ panel: vscode.WebviewPanel; ready: boolean }>>();
  private anchors = new Map<string, string>();
  constructor(private extensionUri: vscode.Uri) {}
  resolveCustomTextEditor(document: vscode.TextDocument, panel: vscode.WebviewPanel) {
    const key = document.uri.toString();
    const preview = { panel, ready: false };
    const previews = this.previews.get(key) ?? new Set();
    previews.add(preview); this.previews.set(key, previews);
    const webview = panel.webview;
    const parent = vscode.Uri.joinPath(document.uri, '..');
    webview.options = { enableScripts: true, localResourceRoots: [vscode.Uri.joinPath(this.extensionUri, 'dist'), parent] };
    const asset = (name: string) => webview.asWebviewUri(vscode.Uri.joinPath(this.extensionUri, 'dist/webview', name)).toString();
    const snapshot = (): Snapshot => ({ text: document.getText(), version: document.version });
    const send = (message: HostMessage) => void webview.postMessage(message);
    const update = () => send({ type: 'document', ...snapshot(), name: document.uri.path.split('/').pop() ?? 'Untitled', baseUri: webview.asWebviewUri(parent).toString().replace(/\/?$/, '/') });
    let applying: string | undefined;
    const changes = vscode.workspace.onDidChangeTextDocument(event => {
      if (event.document.uri.toString() === document.uri.toString() && document.getText() !== applying) update();
    });
    const messages = webview.onDidReceiveMessage((message: unknown) => {
      if (!isClientMessage(message)) return;
      if (message.type === 'ready') { preview.ready = true; update(); this.revealAnchor(key); return; }
      const previous = this.queues.get(key) ?? Promise.resolve();
      const next = previous.then(async () => {
        if (message.type === 'edit') {
          if (message.version !== document.version || message.text.length > 2_000_000) {
            send({ type: 'ack', id: message.id, accepted: false, ...snapshot() }); return;
          }
          let accepted = true;
          if (message.text !== document.getText()) {
            const change = textChange(document.getText(), message.text);
            const edit = new vscode.WorkspaceEdit();
            edit.replace(document.uri, new vscode.Range(document.positionAt(change.start), document.positionAt(change.end)), change.text);
            applying = message.text;
            try { accepted = await vscode.workspace.applyEdit(edit); }
            finally { applying = undefined; }
          }
          send({ type: 'ack', id: message.id, accepted, ...snapshot() });
        } else if (message.type === 'source') {
          await vscode.commands.executeCommand('vscode.openWith', document.uri, 'default', panel.viewColumn);
        } else if (message.type === 'save') {
          await document.save();
        } else if (message.type === 'undo' || message.type === 'redo') {
          await vscode.commands.executeCommand(message.type);
        } else if (message.type === 'link') {
          await this.openLink(document.uri, message.href);
        }
      }).catch(error => send({ type: 'error', message: error instanceof Error ? error.message : 'Unable to update document.' }));
      this.queues.set(key, next);
      void next.finally(() => { if (this.queues.get(key) === next) this.queues.delete(key); });
    });
    const visibility = panel.onDidChangeViewState(() => this.revealAnchor(key));
    panel.onDidDispose(() => { changes.dispose(); messages.dispose(); visibility.dispose(); previews.delete(preview); if (!previews.size) this.previews.delete(key); });
    webview.html = editorHtml(asset('main.js'), asset('main.css'), webview.cspSource, randomBytes(18).toString('hex'));
  }
  private async openLink(document: vscode.Uri, href: string) {
    const target = linkTarget(href);
    if (target.kind === 'web') {
      if (!await vscode.env.openExternal(vscode.Uri.parse(target.url))) throw new Error('Unable to open the link in your browser.');
      return;
    }
    let path = target.path.replace(/\\/g, '/');
    if (/^[a-z]:\//i.test(path)) path = '/' + path;
    const uri = document.with({ path: path ? posix.resolve(posix.dirname(document.path), path) : document.path, query: '', fragment: '' });
    const key = uri.toString();
    if (key === document.toString() && target.fragment !== undefined) {
      this.anchors.set(key, target.fragment); this.revealAnchor(key); return;
    }
    const stat = await vscode.workspace.fs.stat(uri);
    if (stat.type & vscode.FileType.Directory) throw new Error('The link points to a directory. Choose a document file.');
    if (/\.(md|markdown)$/i.test(uri.path)) {
      if (target.fragment !== undefined) this.anchors.set(key, target.fragment);
      try {
        await vscode.commands.executeCommand('vscode.openWith', uri, viewType);
        this.revealAnchor(key);
      } catch (error) { this.anchors.delete(key); throw error; }
    } else {
      await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(uri));
    }
  }
  private revealAnchor(key: string) {
    const fragment = this.anchors.get(key);
    if (fragment === undefined) return;
    const previews = [...(this.previews.get(key) ?? [])].filter(p => p.ready && p.panel.visible);
    if (!previews.length) return;
    this.anchors.delete(key);
    for (const { panel } of previews) void panel.webview.postMessage({ type: 'anchor', fragment } satisfies HostMessage);
  }
}
