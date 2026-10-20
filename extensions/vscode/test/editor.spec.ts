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
  const external: string[] = []; page.on('request', req => { if (!req.url().startsWith(`http://127.0.0.1:${process.env.TPREVIEW_TEST_PORT ?? 4178}`)) external.push(req.url()); });
  await open(page);
  await expect(page.locator('.diagram-preview svg')).toBeVisible();
  await page.getByRole('button', { name: 'Edit diagram' }).click();
  const code = page.locator('.code-block pre'); await code.click();
  await page.keyboard.press('Home'); await page.keyboard.insertText('invalid >>>');
  await expect(page.locator('.diagram-error')).toContainText('last valid diagram');
  await expect(page.locator('.diagram-preview svg')).toBeVisible();
  expect(external).toEqual([]);
});
test('replacing all Mermaid source preserves the editable root and updates the document', async ({ page }) => {
  await open(page);
  await page.getByRole('button', { name: 'Edit diagram' }).click();
  const code = page.locator('.code-block pre'); await code.click();
  await code.evaluate(element => {
    const range = document.createRange(); range.selectNodeContents(element);
    getSelection()!.removeAllRanges(); getSelection()!.addRange(range);
  });
  await page.keyboard.insertText('flowchart LR\n  Draft --> Feedback');
  await expect.poll(() => page.evaluate(() => window.__host.text)).toContain('Draft --> Feedback');
  await expect(page.locator('.diagram-preview svg .nodeLabel').filter({ hasText: 'Feedback' })).toBeVisible();
  await expect(code).toHaveText('flowchart LR\n  Draft --> Feedback');
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

test('frontmatter is readable and stays byte exact while the body is edited and saved', async ({ page }) => {
  await open(page);
  const prefix = '\uFEFF---\r\ntitle: "한글 문서"\r\nsummary: |\r\n  First line\r\n  Second line\r\ntags: [one, two]\r\n---\r\n';
  await page.evaluate(text => window.externalChange(text), prefix + '\r\n# Body\r\n\r\nRead me.\r\n');
  await expect(page.locator('#metadata')).toContainText('한글 문서');
  await expect(page.locator('.ProseMirror h1')).toHaveText('Body');
  await expect(page.locator('#fallback')).toBeHidden();
  expect(await page.evaluate(() => window.__host.edits.length)).toBe(0);
  await page.locator('.ProseMirror p').click(); await page.keyboard.press('End'); await page.keyboard.insertText(' Edited.');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.__host.saved)).toContain('Edited.');
  expect((await page.evaluate(() => window.__host.saved)).startsWith(prefix)).toBe(true);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(page.locator('.ProseMirror p')).toHaveText('Read me.');
  await expect(page.locator('#metadata')).toContainText('Second line');
});

test('single-click and modifier-click send links, Alt-click keeps editing, anchors reveal headings', async ({ page }) => {
  await open(page);
  await page.evaluate(() => window.externalChange('# 한글 제목\n\n[Web](https://example.org) [Relative](../next%20doc.md#heading) [Absolute](/tmp/next.md) [Jump](#한글-제목-1)\n\n' + 'Paragraph before the target.\n\n'.repeat(50) + '## 한글 제목\n'));
  const messages = () => page.evaluate(() => window.__host.messages.filter(m => m.type === 'link'));
  await page.getByRole('link', { name: 'Web', exact: true }).click();
  await expect.poll(messages).toEqual([{ type: 'link', href: 'https://example.org' }]);
  await page.getByRole('link', { name: 'Relative', exact: true }).click({ modifiers: [process.platform === 'darwin' ? 'Meta' : 'Control'] });
  await page.getByRole('link', { name: 'Absolute', exact: true }).click();
  await expect.poll(messages).toHaveLength(3);
  await page.getByRole('link', { name: 'Web', exact: true }).click({ modifiers: ['Alt'] });
  expect(await messages()).toHaveLength(3);
  await page.getByRole('link', { name: 'Jump', exact: true }).click();
  await expect(page.locator('.ProseMirror h2')).toBeInViewport();
  expect(await page.evaluate(() => window.__host.edits.length)).toBe(0);
});

test('navigation waits until pending edits reach the host', async ({ page }) => {
  await open(page);
  await page.evaluate(() => { window.externalChange('# Draft\n\n[Next](next.md)\n'); window.__host.delay = 300; });
  await expect(page.locator('.ProseMirror h1')).toHaveText('Draft');
  await page.locator('.ProseMirror h1').click(); await page.keyboard.press('End'); await page.keyboard.insertText(' changed');
  await page.getByRole('link', { name: 'Next', exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.__host.messages.at(-1)?.type)).toBe('link');
  expect(await page.evaluate(() => window.__host.text)).toContain('Draft changed');
  await expect(page.locator('#conflict')).toBeHidden();
});
