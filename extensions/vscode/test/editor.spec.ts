import { test, expect, type Page } from '@playwright/test';
declare global { interface Window { __host: { text: string; version: number; edits: unknown[]; delay: number; saved: string; messages: { type: string }[] }; externalChange(text: string): void } }
async function open(page: Page) {
  await page.goto('/');
  await expect(page.locator('.ProseMirror')).toBeVisible();
  await expect(page.locator('#status')).toHaveText('Changes applied to document');
}
test('opens without writes, edits text and tasks, saves, and switches to source', async ({ page }) => {
  await open(page);
  expect(await page.evaluate(() => window.__host.edits.length)).toBe(0);
  await page.locator('.ProseMirror h1').click(); await page.keyboard.press('End'); await page.keyboard.type(' updated');
  await expect.poll(() => page.evaluate(() => window.__host.text)).toContain('updated');
  await page.getByRole('checkbox', { name: 'Task complete' }).first().check();
  await expect.poll(() => page.evaluate(() => window.__host.text)).toContain('- [x] Review draft');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.__host.saved)).toContain('updated');
  await page.getByRole('button', { name: '</> Source' }).click();
  await expect.poll(() => page.evaluate(() => window.__host.messages.at(-1)?.type)).toBe('source');
});
test('renders Mermaid offline and retains its last valid result during invalid edits', async ({ page }) => {
  const external: string[] = []; page.on('request', req => { if (!req.url().startsWith('http://127.0.0.1:4178')) external.push(req.url()); });
  await open(page);
  await expect(page.locator('.diagram-preview svg')).toBeVisible();
  await page.getByRole('button', { name: 'Edit diagram' }).click();
  const code = page.locator('.code-block code'); await code.click();
  await page.keyboard.press('Home'); await page.keyboard.insertText('invalid >>>');
  await expect(page.locator('.diagram-error')).toContainText('last valid diagram');
  await expect(page.locator('.diagram-preview svg')).toBeVisible();
  expect(external).toEqual([]);
});
test('updates from another editor and routes undo / redo through document history', async ({ page }) => {
  await open(page); await page.evaluate(() => window.externalChange('# External update\n\nHello\n'));
  await expect(page.locator('.ProseMirror h1')).toHaveText('External update');
  await page.locator('.ProseMirror p').click(); await page.keyboard.press('End'); await page.keyboard.insertText(' world');
  await expect.poll(() => page.evaluate(() => window.__host.text)).toContain('world');
  await page.getByRole('button', { name: 'Undo', exact: true }).click(); await expect(page.locator('.ProseMirror p')).toHaveText('Hello');
  await page.getByRole('button', { name: 'Redo', exact: true }).click(); await expect(page.locator('.ProseMirror p')).toHaveText('Hello world');
});
test('retains a draft when an external change races typing', async ({ page }) => {
  await open(page); await page.evaluate(() => { window.__host.delay = 300; });
  await page.locator('.ProseMirror h1').click(); await page.keyboard.press('End'); await page.keyboard.insertText(' local');
  await page.evaluate(() => window.externalChange('# External winner\n'));
  await expect(page.locator('#conflict')).toBeVisible(); await expect(page.locator('#draft')).toContainText('local');
  await expect.poll(() => page.evaluate(() => window.__host.text)).toBe('# External winner\n');
  await page.waitForTimeout(350); await page.getByRole('button', { name: 'Use file version' }).click();
  await expect(page.locator('.ProseMirror h1')).toHaveText('External winner'); await expect(page.locator('#conflict')).toBeHidden();
});
test('unsupported syntax remains unmodified and HTML is never executed', async ({ page }) => {
  await open(page);
  const raw = '---\ntitle: Keep me\n---\n\n<script>window.hacked=true</script>\n';
  await page.evaluate(text => window.externalChange(text), raw);
  await expect(page.locator('#fallback')).toBeVisible(); await expect(page.locator('#raw')).toHaveText(raw);
  expect(await page.evaluate(() => window.__host.edits.length)).toBe(0);
});
test('Korean composition finishes as one coherent document change', async ({ page }) => {
  await open(page); await page.locator('.ProseMirror h1').click(); await page.keyboard.press('End');
  await page.locator('.ProseMirror').dispatchEvent('compositionstart'); await page.keyboard.insertText(' 한글 입력');
  await page.locator('.ProseMirror').dispatchEvent('compositionend');
  await expect.poll(() => page.evaluate(() => window.__host.text)).toContain('한글 입력');
  await expect(page.locator('#conflict')).toBeHidden();
});
test('an external update during composition preserves the Korean draft', async ({ page }) => {
  await open(page); await page.locator('.ProseMirror h1').click(); await page.keyboard.press('End');
  await page.locator('.ProseMirror').dispatchEvent('compositionstart'); await page.keyboard.insertText(' 작성 중');
  await page.evaluate(() => window.externalChange('# External edit\n'));
  await expect(page.locator('#conflict')).toBeVisible();
  await expect(page.locator('#draft')).toContainText('작성 중');
  expect(await page.evaluate(() => window.__host.edits.length)).toBe(0);
});
test('save waits for the newest queued keystrokes to reach the document', async ({ page }) => {
  await open(page); await page.evaluate(() => { window.__host.delay = 150; });
  await page.locator('.ProseMirror h1').click(); await page.keyboard.press('End');
  await page.keyboard.type(' rapid typing');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.__host.saved)).toContain('Working notes rapid typing');
  await expect(page.locator('#conflict')).toBeHidden();
});
test('toolbar creates formatting, tables and diagrams', async ({ page }) => {
  await open(page); await page.evaluate(() => window.externalChange('Hello\n'));
  await page.locator('.ProseMirror p').click(); await page.keyboard.press('Home'); await page.keyboard.down('Shift'); await page.keyboard.press('End'); await page.keyboard.up('Shift');
  await page.getByRole('button', { name: 'Bold', exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.__host.text)).toContain('**Hello**');
  await page.keyboard.press('ArrowRight'); await page.keyboard.press('Enter');
  await page.getByRole('button', { name: 'Table', exact: true }).click(); await expect(page.locator('.ProseMirror table')).toBeVisible();
});
for (const width of [320, 375, 414, 768]) test(`editor fits a ${width}px pane`, async ({ page }) => {
  await page.setViewportSize({ width, height: 850 }); await open(page);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
});
