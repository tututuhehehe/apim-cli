# 02: Windows job 首跑红 —— 5 处 `E0432`（门只加了一半）

Status: done
Blocked by: 无（票 01 的后续修复）

**What to build:** 让 `ci.yml` 的 `windows` job（票 01 加的）绿。根因是上一轮**只给 unix-only 的定义带了门，引用它们的测试模块没带门**。

## 现状（CI 首跑红的原文）

CI run `38037195245`，job「Windows 编译检查」：`cargo check --locked --all-targets` 在 `apim` 自己的 crate 上 5 处 `E0432 unresolved import`：

| 文件 | 悬空的 import | 门在哪 |
|---|---|---|
| `clients/codex/tests/config.rs:8` | `super::helpers::assert_no_tmp` | `helpers.rs:22` `#[cfg(unix)]` |
| `clients/codex/tests/import.rs:11` | `super::helpers::{assert_no_tmp, fake_codex, fake_codex_with_empty_catalog}` | 同上 / `:63` / `:87` |
| `clients/pi/tests/import.rs:7` | `super::helpers::{fake_pi, fake_pi_failing, write_model_table}` | `pi/tests/helpers.rs:34/54/64` |
| `clients/pi/tests/verify.rs:5` | 同上 | 同上 |
| `clients/codex/tests/restart.rs:5` | `restart::{is_codex_server, looks_like_wrapped_codex_server}` | `restart.rs:89/108`（产品代码里的 `#[cfg(unix)]`） |

**根因（一句话）**：Windows 上那些 helper 不存在了，而 `use` 行还在 → 名字解析失败。上一轮的审计只查了「unix-only 的符号有没有带门」（A 方向），**没查「带门的项有没有被 Windows 上会编的代码引用」**（B 方向）—— 这正是 A 方向的镜像盲区。

## 做法

### ① 在门的边界上修，不在引用点上撒

**判据：这个测试模块是不是「整模块 unix-only」**（看它每个 `#[test]` 头顶的属性，不粘性判定 —— 别用一个 `#[cfg(unix)]` 给后面所有测试都算上）。

- **整模块 unix** → 在 `tests/mod.rs` 的 `mod xxx;` 声明处一处带门：
  - `clients/codex/tests/mod.rs`：`#[cfg(unix)] mod restart;`（它 3 个测试全部驱动 `#[cfg(unix)]` 的 `is_codex_server` / `looks_like_wrapped_codex_server`，靠 `ps` 认进程）
- **只有一部分 unix** → **不整模块带门**（会白丢 Windows 侧覆盖），把 import 拆两行：
  - `codex/tests/config.rs`：`use super::helpers::{provider_write, temp_dir};` + `#[cfg(unix)] use super::helpers::assert_no_tmp;`（前 3 个测试跨平台，后 2 个要 `0o600` / 符号链接）
  - `codex/tests/import.rs`：`{request_for, temp_dir}` + `#[cfg(unix)] {assert_no_tmp, fake_codex, fake_codex_with_empty_catalog}`（未带门的 3 个测试只用前两个）
  - `pi/tests/import.rs`：`{request_for, temp_dir}` + `#[cfg(unix)] {fake_pi, fake_pi_failing, write_model_table}`（`missing_pi_binary_fails_before_writing` 只用前两个）
  - `pi/tests/verify.rs`：整个 import 带门（前 2 个测试只解析表格，**不用任何 helper**）

每处都留了一行注释说明「为什么这个模块不整带门 / 为什么整带门」—— 下次改的人不用重新判一遍。

### ② 双向审计（本票的重点：方法写下来，下次别再漏）

工具：**本目录的 `audit-cfg.py`**（`python3 audit-cfg.py <仓库根>`，零依赖）。

- **A 方向**（上一轮做的）：扫 `std::os::unix` / `PermissionsExt` / `from_mode` / `.mode()` / `set_mode` / `unix::fs::symlink` / `OpenOptionsExt` / `libc::` / `std::os::fd` 这些 **unix-only 的符号与 trait 方法**，要求它们都在 `#[cfg(unix)]` / `#[cfg(windows)]` 门内。
  - 踩过的坑：**trait 方法也要扫**（`.mode()` 不含 trait 名字，只 grep `PermissionsExt` / `from_mode` 会漏 —— 上一轮漏过一次，被编译器当场拓住）。
- **B 方向**（本票补的）：先算出 **Windows 上真正会被编译的文件集**，再拿每个 unix 门项的名字去里面搜引用。三步：
  1. 从 `src/main.rs` 沿 `mod x;` 声明走，遇到 `#[cfg(unix)] mod x;` 就把整棵子树标成「Windows 不编」（`--all-targets` 会编 `#[cfg(test)]`，所以测试模块也走）；
  2. 把每个 `#[cfg(...)]` 属性 + 它装饰的项/语句按缩进算成一个**区域**（要能看见嵌在别的区域里的门，比如 `#[cfg(test)] mod tests` 里的 `#[cfg(unix)]` —— 否则会误报一片）；
  3. 名字**有非 unix 的同名兄弟**（`cfg(not(unix))` 那种成对实现，如 `clients/lock.rs::process_alive`、`restart.rs::scan_codex_servers`）就不算问题；否则引用点不在 unix 区域里 → 报出来。行内注释不算引用。
- **方法自身的验证（这一步不能省）**：拿修复前的提交跑，B 方向必须精确抓出 CI 报的那 5 个模块；修复后两方向都必须 0。结果见 `## Done`。

## 验收

- [ ] 5 处 `E0432` 全消，`windows` job 绿（**CI 是唯一的验证回路**：本机编不了 Windows target，ring 的 C 构建要 Windows SDK —— 票 01 已证）
- [ ] 三命令 + MSRV 全绿；`tests/docs.rs` 11 条守卫全绿（本机 host 路径）
- [ ] 双向审计两方向都是 0
- [ ] 模块划分有注释说清（整模块带门 vs 拆 import）

## 用哪条守卫钉住

**还是这条 job 自己**（同票 01），但本票补了**本机可跑的前置检查**：`audit-cfg.py` 两个方向都查，改 `src/**` 的 cfg 门时先跑它，能在本机就把这类问题挡掉（B 方向正是编译器在 Windows 上才会说的话）。没往 `tests/docs.rs` 加守卫：那是文档不变量守卫，塞 cfg 分析进去要自带一个 Rust 解析器，而 CI 的 job 已经是权威判据。

## Done

- commit: `91a951d`（`91a951da6385`，5 个文件 + 审计脚本；票面回填在其后一次提交）
- **本机验证（CI 之外的，能做的都做了）**：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning
  - `cargo test --all` → `335 passed; 0 failed; 4 ignored` + 守卫 `11 passed`
  - `cargo +1.88 check --locked --all-targets` → 编过
  - Windows target：**本机依旧编不动**（ring 要 Windows SDK），**没有**假装跑过 —— 验证回路就是 CI
- **双向审计（方法验证 + 修复后）**：
  - 对**修复前**（`git archive HEAD` 到 `/tmp`）跑 `audit-cfg.py`：B 方向报 **12 处**，全部落在那 5 个模块的 import 行上（`codex/tests/config.rs:8`、`codex/tests/import.rs:11`、`pi/tests/import.rs:7`、`pi/tests/verify.rs:5`、`codex/tests/restart.rs:5`）→ **与 CI 的 5 个 `E0432` 完全对应**（5 个模块，12 行 import）；A 方向 0 处。
  - 对**修复后**跑：B 方向 **0 处**、A 方向 **0 处**；且「Windows 不编」的清单变成 `clients/codex/tests/restart.rs`（正是新加的门）。
  - 审计脚本自己踩了三个坑，都修了并记在脚本里：① 只 grep trait 名会漏 trait 方法（`.mode()`）；② 区域扫描不能跳过外层区域（否则看不见 `cfg(test) mod tests` 里的 `#[cfg(unix)]`）；③ `findall` 已经剥掉了 `#[`/`]`，再切 `[2:-1]` 会把 `cfg(unix)` 切成 `g(unix` → 整条门静默失效（**这个坑最阴：脚本会一声不响地少报**）。
- **偏离 spec：无**。判据用的是「整模块 unix → 门加在 `mod` 声明；部分 unix → 拆 import 并注释说清」，正是票面要求的形式。
- 遗留（不阻塞）：Windows job 仍只证明「能编」，不证明行为；`codex/tests/restart.rs` 整模块在 Windows 上不编（它 3 个测试本来就全是 `#[cfg(unix)]`，没丢覆盖）。
