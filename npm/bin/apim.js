#!/usr/bin/env node
'use strict';

// apim 的 npm 入口 shim：定位当前平台对应的预编译二进制并原样转发参数。
// 二进制放在 apim-cli-<platform>-<arch> 子包里（optionalDependencies），
// 由 npm 按 os/cpu 只安装匹配的那一个。

const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

// 平台 → 子包名。Windows 的子包叫 windows（不用 process.platform 的 win32），
// 一是可读性，二是避开 npm 名称风控对某些模式的误判。
const PLATFORM_PACKAGES = {
  'darwin-arm64': 'apim-cli-darwin-arm64',
  'darwin-x64': 'apim-cli-darwin-x64',
  'linux-x64': 'apim-cli-linux-x64',
  'linux-arm64': 'apim-cli-linux-arm64',
  'win32-x64': 'apim-cli-windows-x64',
};

const platformKey = `${process.platform}-${process.arch}`;
const pkgName = PLATFORM_PACKAGES[platformKey];

let pkgDir;
if (pkgName) {
  try {
    // 通过子包的 package.json 定位其安装目录（不依赖 exports 字段）
    pkgDir = path.dirname(require.resolve(`${pkgName}/package.json`));
  } catch {
    pkgDir = undefined;
  }
}

if (!pkgDir) {
  process.stderr.write(
    `apim: no prebuilt binary available for ${platformKey}.\n` +
      `Install from source instead:  cargo install --path .\n` +
      `Or download manually:         https://github.com/tututuhehehe/apim-cli/releases\n`,
  );
  process.exit(1);
}

const binPath = path.join(pkgDir, 'bin', process.platform === 'win32' ? 'apim.exe' : 'apim');

if (!fs.existsSync(binPath)) {
  process.stderr.write(`apim: binary missing at ${binPath} (corrupted install?)\n`);
  process.exit(1);
}

// npm 不一定保留可执行位（尤其非 bin 字段的文件），这里补一下
if (process.platform !== 'win32') {
  try {
    fs.chmodSync(binPath, 0o755);
  } catch {
    /* 只读文件系统等情况：交给 spawn 报错即可 */
  }
}

const result = spawnSync(binPath, process.argv.slice(2), { stdio: 'inherit' });
if (result.error) {
  process.stderr.write(`apim: failed to run ${binPath}: ${result.error.message}\n`);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
