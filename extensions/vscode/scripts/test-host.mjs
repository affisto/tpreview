import { spawn } from 'node:child_process';
import { mkdtemp, writeFile, readFile, mkdir, rm, access } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { createServer } from 'node:net';
import { chromium, expect } from '@playwright/test';
import { downloadAndUnzipVSCode } from '@vscode/test-electron';

const scratch = await mkdtemp(join(tmpdir(), 'tpreview-vscode-'));
const capture = process.argv.includes('--capture');
const profile = join(scratch, 'profile');
const file = join(scratch, capture ? 'launch-notes.md' : 'notes.md');
const initial = capture ? await readFile('demo/launch-notes.md', 'utf8') : '# VS Code integration\n\nEdit this paragraph.\n\n- [ ] Verify save\n\n```mermaid\nflowchart LR\n  Edit --> Save\n```\n';
await mkdir(join(profile, 'User'), { recursive: true });
await writeFile(join(profile, 'User/settings.json'), JSON.stringify({ 'workbench.startupEditor': 'none', 'window.restoreWindows': 'none', 'telemetry.telemetryLevel': 'off', 'update.mode': 'none', 'extensions.autoCheckUpdates': false, 'chat.disableAIFeatures': true, ...(capture ? { 'breadcrumbs.enabled': false, 'editor.minimap.enabled': false, 'workbench.activityBar.location': 'hidden', 'workbench.statusBar.visible': false, 'window.commandCenter': false, 'window.zoomLevel': 0, 'editor.fontSize': 16, 'workbench.colorTheme': 'Default Dark Modern' } : {}) }));
await writeFile(file, initial);
let executable = process.env.VSCODE_EXECUTABLE;
if (!executable && process.platform === 'darwin') {
  const installed = '/Applications/Visual Studio Code.app/Contents/MacOS/Code';
  try { await access(installed); executable = installed; } catch {}
}
executable ??= await downloadAndUnzipVSCode('stable');
const server = createServer();
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const port = server.address().port;
await new Promise(resolve => server.close(resolve));
const child = spawn(executable, [
  '--no-sandbox', '--disable-gpu', '--disable-workspace-trust',
  '--skip-welcome', '--skip-release-notes', `--user-data-dir=${profile}`,
  `--extensions-dir=${join(scratch, 'extensions')}`, `--extensionDevelopmentPath=${resolve('.')}`,
  `--remote-debugging-port=${port}`, file,
], { env: { ...process.env, ELECTRON_RUN_AS_NODE: '' }, stdio: ['ignore', 'pipe', 'pipe'] });
let logs = '';
child.stdout.on('data', data => { logs += data; }); child.stderr.on('data', data => { logs += data; });
let browser;
try {
  await expect.poll(async () => {
    try { return (await fetch(`http://127.0.0.1:${port}/json/version`)).ok; } catch { return false; }
  }, { timeout: 45_000 }).toBe(true);
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  let page;
  await expect.poll(() => {
    page = browser.contexts().flatMap(c => c.pages()).find(p => p.url().includes('workbench'));
    return !!page;
  }, { timeout: 30_000 }).toBe(true);
  page.setDefaultTimeout(15_000);
  await page.bringToFront();
  await page.locator('.monaco-workbench').waitFor();
  await expect(page.locator('.monaco-editor .view-lines').first()).toContainText(capture ? 'Launch notes' : 'VS Code integration');
  await page.locator('.monaco-editor .view-lines').first().click();
  await page.keyboard.press('F1');
  await page.locator('.quick-input-widget input').fill('>MD Visual Editor: Open Visual Markdown Editor');
  await expect(page.locator('.quick-input-list')).toContainText('Open Visual Markdown Editor');
  await page.keyboard.press('Enter');
  let frame;
  await expect.poll(async () => {
    for (const candidate of page.frames()) {
      if (await candidate.locator('.ProseMirror').count()) { frame = candidate; return true; }
    }
    return false;
  }, { timeout: 30_000 }).toBe(true);
  await expect(frame.locator('.ProseMirror h1')).toHaveText(capture ? 'Launch notes' : 'VS Code integration');
  await expect(frame.locator('.diagram-preview svg')).toBeVisible({ timeout: 15_000 });
  await expect(frame.locator('#status')).toHaveText('Changes applied to document');
  if (await readFile(file, 'utf8') !== initial) throw new Error('Opening the editor modified the file');
  if (capture) {
    await (await import('./capture-walkthrough.mjs')).captureWalkthrough(page, frame, file);
  } else {
  await frame.locator('.ProseMirror p').first().click();
  await page.keyboard.press('End'); await page.keyboard.insertText(' Saved visually.');
  await frame.getByRole('button', { name: 'Save', exact: true }).click();
  await expect.poll(() => readFile(file, 'utf8')).toContain('Saved visually.');
  await frame.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(frame.locator('.ProseMirror p').first()).toHaveText('Edit this paragraph.');
  await frame.getByRole('button', { name: 'Redo', exact: true }).click();
  await expect(frame.locator('.ProseMirror p').first()).toContainText('Saved visually.');
  await frame.locator('.ProseMirror p').first().click(); await page.keyboard.press('End');
  await page.keyboard.type(' Rapid typing.');
  await page.keyboard.press(process.platform === 'darwin' ? 'Meta+s' : 'Control+s');
  await expect.poll(() => readFile(file, 'utf8')).toContain('Rapid typing.');
  await frame.getByRole('checkbox', { name: 'Task complete' }).check();
  await frame.getByRole('button', { name: 'Save', exact: true }).click();
  await expect.poll(() => readFile(file, 'utf8')).toContain('- [x] Verify save');
  await mkdir('../../dist', { recursive: true });
  await page.screenshot({ path: '../../dist/vscode-editor-preview.png' });
  await writeFile(file, '# Changed externally\n\nThe file watcher updates the editor.\n');
  await expect(frame.locator('.ProseMirror h1')).toHaveText('Changed externally', { timeout: 15_000 });
  await frame.getByRole('button', { name: '</> Source' }).click();
  await expect(page.locator('.monaco-editor textarea').first()).toBeAttached();
  console.log('PASS: real VS Code opens without writes; Mermaid, visual edits, save, undo, redo, tasks, external updates and source switching work.');
  }
} catch (error) {
  await mkdir('../../dist', { recursive: true });
  await writeFile('../../dist/vscode-host-test.log', logs);
  if (browser) for (const page of browser.contexts().flatMap(c => c.pages())) {
    try { await page.screenshot({ path: '../../dist/vscode-host-failure.png' }); } catch {}
  }
  throw error;
} finally {
  await browser?.close();
  child.kill('SIGTERM');
  await new Promise(resolve => { if (child.exitCode !== null) resolve(); else { child.once('exit', resolve); setTimeout(resolve, 3000).unref(); } });
  await rm(scratch, { recursive: true, force: true, maxRetries: 3 }).catch(() => {});
}
