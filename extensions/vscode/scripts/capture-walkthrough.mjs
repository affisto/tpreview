import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { expect } from '@playwright/test';

// Record only the isolated VS Code renderer, never the user's desktop or other apps.
export async function captureWalkthrough(page, initialFrame, file) {
  const output = resolve('../../dist/md-visual-editor-media');
  const framesDir = join(output, 'frames');
  await mkdir(framesDir, { recursive: true });
  const client = await page.context().newCDPSession(page);
  await client.send('Emulation.setDeviceMetricsOverride', { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false });
  let frame = initialFrame;
  const shots = [], chapters = [], captured = [];
  let frameNumber = 0, writes = Promise.resolve();
  const start = performance.now();
  client.on('Page.screencastFrame', event => {
    const name = `frame-${String(frameNumber++).padStart(6, '0')}.jpg`;
    captured.push({ name, time: performance.now() - start });
    writes = writes.then(() => writeFile(join(framesDir, name), Buffer.from(event.data, 'base64')));
    void client.send('Page.screencastFrameAck', { sessionId: event.sessionId }).catch(() => {});
  });
  await client.send('Page.startScreencast', { format: 'jpeg', quality: 92, maxWidth: 1440, maxHeight: 900, everyNthFrame: 1 });
  const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
  const chapter = text => { chapters.push({ time: (performance.now() - start) / 1000, text }); console.log('Recording:', text); };
  const shot = async (name, description) => {
    await page.screenshot({ path: join(output, name) }); shots.push({ name, description });
  };
  async function findEditor() {
    await expect.poll(async () => {
      for (const candidate of page.frames()) if (await candidate.locator('.ProseMirror').count()) { frame = candidate; return true; }
      return false;
    }).toBe(true);
    await expect(frame.locator('.ProseMirror h1')).toHaveText('Launch notes');
  }
  const move = async locator => {
    await locator.scrollIntoViewIfNeeded();
    const box = await locator.boundingBox();
    if (box) await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 18 });
    await pause(350);
  };
  const click = async locator => { await move(locator); await locator.click(); };
  const selectText = async (locator, endOnly = false) => locator.evaluate((element, collapse) => {
    const selection = window.getSelection(); const range = document.createRange();
    range.selectNodeContents(element); if (collapse) range.collapse(false);
    selection.removeAllRanges(); selection.addRange(range);
  }, endOnly);
  try {
    chapter('MD Visual Editor · Markdown을 화면에서 바로 편집하세요');
    await pause(3000);
    await shot('01-overview.png', '제목, 체크리스트, 표, Mermaid를 한 화면에서 확인합니다.');

    chapter('명령 팔레트에서 MD Visual Editor를 실행합니다');
    await click(frame.getByRole('button', { name: '</> Source' }));
    await expect(page.locator('.monaco-editor .view-lines').first()).toBeVisible();
    await page.locator('.monaco-editor .view-lines').first().click();
    await pause(800);
    await page.keyboard.press('F1');
    await page.locator('.quick-input-widget input').fill('>MD Visual Editor: Open Visual Markdown Editor');
    await expect(page.locator('.quick-input-list')).toContainText('Open Visual Markdown Editor');
    await pause(2500);
    await shot('02-open-editor.png', '명령 팔레트에서 MD Visual Editor: Open Visual Markdown Editor를 실행합니다.');
    await page.keyboard.press('Enter'); await findEditor(); await pause(2000);

    chapter('문장을 클릭해 수정하고, 선택한 글자에 서식을 적용합니다');
    const paragraph = frame.locator('.ProseMirror > p').first();
    await click(paragraph); await selectText(paragraph, true);
    await page.keyboard.type(' Ready for the team.', { delay: 110 });
    await pause(1000);
    await selectText(paragraph);
    await click(frame.getByRole('button', { name: 'Bold', exact: true }));
    await page.keyboard.press('ArrowRight'); await pause(1800);
    await shot('03-formatting.png', '문장을 직접 수정하고 Bold 버튼으로 굵게 표시합니다.');

    chapter('체크박스를 누르고 표의 셀을 직접 수정합니다');
    await click(frame.getByRole('checkbox', { name: 'Task complete' }).first());
    await pause(1200);
    const cell = frame.locator('.ProseMirror td').filter({ hasText: /^Draft$/ });
    await click(cell);
    await selectText(cell.locator('p'));
    await page.keyboard.type('Published', { delay: 150 }); await pause(1500);
    await expect(frame.locator('.ProseMirror table tbody tr')).toHaveCount(3);
    await expect(frame.locator('.diagram-preview svg')).toBeVisible();
    await shot('04-tasks-and-table.png', '체크리스트 완료 상태와 표의 상태 값을 변경합니다.');

    chapter('실행 취소와 다시 실행도 VS Code 문서에 반영됩니다');
    // A single paste creates a clear, reversible change for this demonstration.
    await selectText(frame.locator('.ProseMirror td').filter({ hasText: /^Published$/ }).locator('p'));
    await page.keyboard.insertText('Ready'); await pause(1000);
    await click(frame.getByRole('button', { name: 'Undo', exact: true })); await pause(1500);
    await click(frame.getByRole('button', { name: 'Redo', exact: true })); await pause(1500);

    chapter('Edit diagram을 눌러 Mermaid 코드를 수정합니다');
    await click(frame.getByRole('button', { name: 'Edit diagram' }));
    const code = frame.locator('.code-block pre');
    await click(code);
    await selectText(code);
    await page.keyboard.insertText('flowchart LR\n    Draft --> Review\n    Review --> Publish\n    Publish --> Feedback');
    await expect(frame.locator('.diagram-preview svg .nodeLabel').filter({ hasText: 'Feedback' })).toBeVisible();
    await pause(2500);
    await shot('05-mermaid-code.png', 'Mermaid 소스를 바꾸면 아래 미리보기가 갱신됩니다.');
    await click(frame.getByRole('button', { name: 'Hide code' }));
    await pause(1500);

    chapter('Save를 누르면 원래의 .md 파일에 저장됩니다');
    await click(frame.getByRole('button', { name: 'Save', exact: true }));
    await expect.poll(() => readFile(file, 'utf8')).toContain('Feedback');
    await pause(2200);
    await shot('06-mermaid-preview.png', '코드를 접고 다이어그램을 확인한 뒤 같은 Markdown 파일에 저장합니다.');

    chapter('Source로 전환하면 편집 결과를 Markdown 원문으로 볼 수 있습니다');
    await click(frame.getByRole('button', { name: '</> Source' }));
    await expect(page.locator('.monaco-editor .view-lines').first()).toContainText('Feedback');
    await pause(2500);
    await shot('07-markdown-source.png', '일반 VS Code 소스 편집기로 전환해 Markdown 원문을 확인합니다.');
    await page.locator('.monaco-editor .view-lines').first().click();
    await pause(500);
    await page.keyboard.press('F1');
    await page.locator('.quick-input-widget input').fill('>MD Visual Editor: Open Visual Markdown Editor');
    await page.keyboard.press('Enter'); await findEditor();
    chapter('MD Visual Editor · 편집은 시각적으로, 파일은 Markdown 그대로');
    await pause(3500);
  } finally {
    await client.send('Page.stopScreencast');
    await writes;
  }
  const duration = (performance.now() - start) / 1000;
  if (captured.length < 10) throw new Error('Insufficient recorded frames');
  const manifest = captured.map((frame, index) => {
    const nextTime = captured[index + 1]?.time ?? duration * 1000;
    return `file 'frames/${frame.name}'\nduration ${Math.max(.01, (nextTime - frame.time) / 1000).toFixed(4)}`;
  }).join('\n') + `\nfile 'frames/${captured.at(-1).name}'\n`;
  await writeFile(join(output, 'frames.ffconcat'), manifest);
  const stamp = seconds => { const ms = Math.floor(seconds * 1000); return `${String(Math.floor(ms / 3600000)).padStart(2, '0')}:${String(Math.floor(ms / 60000) % 60).padStart(2, '0')}:${String(Math.floor(ms / 1000) % 60).padStart(2, '0')},${String(ms % 1000).padStart(3, '0')}`; };
  await writeFile(join(output, 'walkthrough.ko.srt'), chapters.map((item, index) => `${index + 1}\n${stamp(item.time)} --> ${stamp(chapters[index + 1]?.time ?? duration)}\n${item.text}\n`).join('\n'));
  await writeFile(join(output, 'capture.json'), JSON.stringify({ capturedAt: new Date().toISOString(), app: 'MD Visual Editor', duration, frames: captured.length, screenshots: shots, chapters }, null, 2));
  await writeFile(join(output, 'edited-example.md'), await readFile(file));
  const encode = spawnSync('ffmpeg', ['-hide_banner', '-loglevel', 'warning', '-y', '-f', 'concat', '-safe', '0', '-i', 'frames.ffconcat', '-vf', "fps=30,scale=1440:900,subtitles=walkthrough.ko.srt:force_style='FontName=Apple SD Gothic Neo,FontSize=12,PrimaryColour=&HFFFFFF,BorderStyle=3,OutlineColour=&H88000000,Outline=1,Shadow=0,MarginV=12'", '-t', duration.toFixed(3), '-c:v', 'libx264', '-preset', 'medium', '-crf', '19', '-pix_fmt', 'yuv420p', '-movflags', '+faststart', 'md-visual-editor-walkthrough.mp4'], { cwd: output, stdio: 'inherit' });
  if (encode.status !== 0) throw new Error('Video encoding failed; frames and subtitles are retained.');
  console.log(`Captured ${shots.length} screenshots and ${duration.toFixed(1)} seconds of real VS Code footage in ${output}`);
}
