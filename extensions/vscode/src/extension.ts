import * as vscode from 'vscode';
import { randomBytes } from 'node:crypto';
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
  constructor(private extensionUri: vscode.Uri) {}
  resolveCustomTextEditor(document: vscode.TextDocument, panel: vscode.WebviewPanel) {
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
      if (message.type === 'ready') { update(); return; }
      const key = document.uri.toString();
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
    panel.onDidDispose(() => { changes.dispose(); messages.dispose(); });
    webview.html = editorHtml(asset('main.js'), asset('main.css'), webview.cspSource, randomBytes(18).toString('hex'));
  }
  private async openLink(document: vscode.Uri, href: string) {
    if (/^https?:\/\//i.test(href)) { await vscode.env.openExternal(vscode.Uri.parse(href)); return; }
    if (!href || /^[a-z][a-z\d+.-]*:/i.test(href) || href.startsWith('//') || href.startsWith('#')) return;
    const [path] = href.split('#');
    if (!/\.(md|markdown)$/i.test(path)) return;
    const uri = vscode.Uri.joinPath(document, '..', decodeURIComponent(path));
    await vscode.commands.executeCommand('vscode.openWith', uri, viewType);
  }
}
