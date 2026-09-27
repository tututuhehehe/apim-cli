#!/usr/bin/env node
// 把 GitHub Release 里的预编译二进制组装成 npm 包。
//
//   node scripts/publish-npm.mjs <version>            # 组装 + npm pack（不发布）
//   node scripts/publish-npm.mjs <version> --publish  # 组装 + npm publish
//
// 结构（平台子包模型，同 esbuild / @biomejs/biome）：
//   apim-cli               主包：bin/apim.js shim + optionalDependencies
//   apim-cli-<os>-<arch>   平台子包：内含该平台的预编译二进制
//
// 前置：目标 tag 的 Release 资产已存在；本机 gh 已登录（或 CI 里给了 GH_TOKEN）。
// 版本号与 Cargo.toml / git tag 一致（不带 v 前缀）。

import { execFileSync } from 'node:child_process';
import {
  chmodSync,
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';

const REPO = 'tututuhehehe/apim-cli';
const ROOT = path.resolve(import.meta.dirname, '..');

// target triple → npm 子包信息（os/cpu 供 npm 自动挑选）
const PLATFORMS = [
  { target: 'aarch64-apple-darwin', pkg: 'apim-cli-darwin-arm64', os: 'darwin', arch: 'arm64', ext: '' },
  { target: 'x86_64-apple-darwin', pkg: 'apim-cli-darwin-x64', os: 'darwin', arch: 'x64', ext: '' },
  { target: 'x86_64-unknown-linux-gnu', pkg: 'apim-cli-linux-x64', os: 'linux', arch: 'x64', ext: '' },
  { target: 'aarch64-unknown-linux-gnu', pkg: 'apim-cli-linux-arm64', os: 'linux', arch: 'arm64', ext: '' },
  { target: 'x86_64-pc-windows-msvc', pkg: 'apim-cli-win32-x64', os: 'win32', arch: 'x64', ext: '.exe' },
];

const args = process.argv.slice(2);
const version = (args.find((a) => !a.startsWith('--')) || '').replace(/^v/, '');
const publish = args.includes('--publish');
const outIdx = args.indexOf('--out');
const outDir = outIdx >= 0 ? args[outIdx + 1] : null;

if (!/^\d+\.\d+\.\d+/.test(version)) {
  console.error('用法: node scripts/publish-npm.mjs <version> [--publish]');
  process.exit(1);
}

const tag = `v${version}`;
const repoUrl = `https://github.com/${REPO}`;

function run(cmd, argv, opts = {}) {
  execFileSync(cmd, argv, { stdio: 'inherit', ...opts });
}

function baseManifest(name, description) {
  return {
    name,
    version,
    description,
    license: 'MIT',
    homepage: repoUrl,
    repository: { type: 'git', url: `git+${repoUrl}.git` },
    bugs: { url: `${repoUrl}/issues` },
  };
}

const work = mkdtempSync(path.join(tmpdir(), 'apim-npm-'));
console.log(`apim npm: 组装 ${version}（tag ${tag}）→ ${work}`);

// 1) 拉取该 tag 的 Release 资产
console.log('apim npm: 下载 Release 资产');
run('gh', ['release', 'download', tag, '--repo', REPO, '--dir', work, '--clobber']);

const built = [];

for (const p of PLATFORMS) {
  const archive = `apim-${tag}-${p.target}`;
  const extractDir = path.join(work, `x-${p.target}`);
  mkdirSync(extractDir, { recursive: true });

  if (p.ext === '.exe') {
    run('unzip', ['-oq', path.join(work, `${archive}.zip`), '-d', extractDir]);
  } else {
    run('tar', ['-xzf', path.join(work, `${archive}.tar.gz`), '-C', extractDir]);
  }

  // 2) 组装平台子包
  const pkgDir = path.join(work, 'build', p.pkg);
  const binDir = path.join(pkgDir, 'bin');
  mkdirSync(binDir, { recursive: true });

  const srcBin = path.join(extractDir, `apim${p.ext}`);
  const dstBin = path.join(binDir, `apim${p.ext}`);
  copyFileSync(srcBin, dstBin);
  if (p.ext === '') chmodSync(dstBin, 0o755);

  writeFileSync(
    path.join(pkgDir, 'package.json'),
    `${JSON.stringify(
      {
        ...baseManifest(p.pkg, `apim prebuilt binary for ${p.os}-${p.arch}`),
        os: [p.os],
        cpu: [p.arch],
        files: [`bin/apim${p.ext}`],
      },
      null,
      2,
    )}\n`,
  );
  built.push(pkgDir);
}

// 3) 组装主包（shim + optionalDependencies）
const mainDir = path.join(work, 'build', 'apim-cli');
mkdirSync(path.join(mainDir, 'bin'), { recursive: true });
copyFileSync(path.join(ROOT, 'npm', 'bin', 'apim.js'), path.join(mainDir, 'bin', 'apim.js'));
writeFileSync(
  path.join(mainDir, 'README.md'),
  [
    '# apim-cli',
    '',
    'Installs the prebuilt [`apim`](https://github.com/tututuhehehe/apim-cli) binary for your platform — a terminal manager for model-provider API keys (TUI + CLI).',
    '',
    '```bash',
    'npm install -g apim-cli',
    'apim --version',
    '```',
    '',
    'Or run it without installing: `npx apim-cli`.',
    '',
    `Full docs: ${repoUrl}#readme`,
    '',
  ].join('\n'),
);
writeFileSync(
  path.join(mainDir, 'package.json'),
  `${JSON.stringify(
    {
      ...baseManifest('apim-cli', 'Terminal manager for model-provider API keys (TUI + CLI)'),
      bin: { apim: 'bin/apim.js' },
      files: ['bin/apim.js'],
      engines: { node: '>=18' },
      keywords: ['apim', 'api-key', 'cli', 'tui', 'llm', 'openai'],
      optionalDependencies: Object.fromEntries(PLATFORMS.map((p) => [p.pkg, version])),
    },
    null,
    2,
  )}\n`,
);
built.push(mainDir);

// 4) 发布（先平台子包，后主包）
const packDir = path.join(work, 'pack');
mkdirSync(packDir, { recursive: true });
// --provenance 需要 OIDC（GitHub Actions），本地手动首发布时不能加
const provenance = process.env.GITHUB_ACTIONS ? ['--provenance'] : [];

for (const dir of built) {
  const argv = publish
    ? ['publish', dir, '--access', 'public', ...provenance]
    : ['pack', dir, '--pack-destination', packDir];
  console.log(`apim npm: ${publish ? 'publish' : 'pack'} ${path.basename(dir)}`);
  run('npm', argv);
}

if (!publish) {
  console.log('\napim npm: 仅打包（未发布）');
  for (const f of readdirSync(packDir)) console.log(`  ${path.join(packDir, f)}`);
  console.log('\n检查无误后加 --publish 发布。');
}

// --out：把打好的 tgz 复制到指定目录（便于本地核对 / 端到端测试）
if (outDir) {
  mkdirSync(outDir, { recursive: true });
  for (const f of readdirSync(packDir)) copyFileSync(path.join(packDir, f), path.join(outDir, f));
  console.log(`apim npm: 产物已复制到 ${outDir}`);
}

rmSync(work, { recursive: true, force: true });
