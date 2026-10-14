import { build, context } from 'esbuild';
import { mkdir, copyFile, readFile, readdir, writeFile, rm } from 'node:fs/promises';
if (!process.argv.includes('--watch')) await rm('dist', { recursive: true, force: true });
await mkdir('dist', { recursive: true });
await mkdir('../../dist', { recursive: true });
const lock = JSON.parse(await readFile('package-lock.json', 'utf8'));
const notices = ['Third-party packages included in the MD Visual Editor webview dependency tree.\n'];
for (const [path, entry] of Object.entries(lock.packages)) {
  if (!path || entry.dev) continue;
  const pkg = JSON.parse(await readFile(`${path}/package.json`, 'utf8'));
  notices.push(`\n${'='.repeat(72)}\n${pkg.name} ${pkg.version}\nLicense: ${pkg.license ?? 'See package license'}\n`);
  for (const file of await readdir(path, { withFileTypes: true })) {
    if (file.isFile() && /^(licen[cs]e|copying|notice)([.\-]|$)/i.test(file.name)) notices.push(await readFile(`${path}/${file.name}`, 'utf8'));
  }
}
await writeFile('dist/THIRD_PARTY_NOTICES.txt', notices.join('\n'));
const builds = [
  { entryPoints: ['src/extension.ts'], outfile: 'dist/extension.cjs', bundle: true, platform: 'node', format: 'cjs', external: ['vscode'], target: 'node20' },
  { entryPoints: ['src/webview/main.ts'], outdir: 'dist/webview', bundle: true, platform: 'browser', format: 'esm', splitting: true, target: 'es2022', minify: true, legalComments: 'external' },
];
await copyFile('../../LICENSE', 'LICENSE');
if (process.argv.includes('--watch')) {
  for (const options of builds) await (await context(options)).watch();
} else {
  for (const options of builds) await build(options);
}
