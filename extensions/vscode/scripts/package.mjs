import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';

const require = createRequire(import.meta.url);
const { version } = JSON.parse(await readFile('package.json', 'utf8'));
if (process.env.GITHUB_REF_TYPE === 'tag' && process.env.GITHUB_REF_NAME !== `extension-v${version}`) {
  throw new Error('Extension tag must match package.json: extension-v' + version);
}
const filename = `tpreview-markdown-${version}.vsix`;
await mkdir('../../dist', { recursive: true });
const output = resolve('../../dist', filename);
const result = spawnSync(process.execPath, [require.resolve('@vscode/vsce/vsce'), 'package', '--no-dependencies', '--out', output], { stdio: 'inherit' });
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
const digest = createHash('sha256').update(await readFile(output)).digest('hex');
await writeFile(output + '.sha256', `${digest}  ${filename}\n`);
console.log(`Checksum: ${filename}.sha256`);
