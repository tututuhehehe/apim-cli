#!/usr/bin/env node
'use strict';

// apim 的 npm 入口 shim：定位当前平台对应的预编译二进制并原样转发参数。
// 二进制放在 apim-cli-<platform>-<arch> 子包里（optionalDependencies），
// 由 npm 按 os/cpu 只安装匹配的那一个。

const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const pkgName = `apim-cli-${process.platform}-${process.arch}`;

let pkgDir;
try {
  // 通过子包的 package.json 定位其安装目录（不依赖 exports 字段）
  pkgDir = path.dirname(require.resolve(`${pkgName}/package.json`));
} catch {
  process.stderr.write(
    `apim: no prebuilt binary available for ${process.platform}-${process.arch}.\n` +
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
