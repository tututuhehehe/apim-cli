# 01: CI 加 Windows 编译检查（`TODO-11`）

Status: done
Blocked by: 无（可立即开工）

**What to build:** `cfg(windows)` 分支的编译错不再等到 `release.yml` 打 tag 才暴露。`ci.yml` 里再加一个 job，跟现有 `msrv` job 同风格，把 Windows 目标**从头编一遍**。

## 现状

`ci.yml` 只有两个 job（都在 ubuntu）：`check`（fmt/clippy/test/JS）与 `msrv`（1.88 + `--locked`）。Windows 专属代码有 4 处（都是 `#[cfg(windows)]`）：

| 位置 | 内容 |
|---|---|
| `src/cli/uninstall/cleanup.rs:59` | `remove_binary` 在 Windows 下的 `bail!` |
| `src/cli/uninstall/mod.rs:192` | Windows 下没有 npm 卸载的说明段 |
| `src/cli/update/mod.rs:171` | npm 渠道 EBUSY 的提示 |
| `src/cli/uninstall/tests.rs:285` | 对应的 Windows 测试 |

它们**只在 `release.yml` 打 tag 时**才被编译 → 编译错要发到 5 平台构建里才现身。

**分清楚两类（第一版票面写糊了，这里改正）**：

- **产品代码**的 `#[cfg(windows)]` 分支（上表前三个）确实只在 `release.yml` 里编（`cargo build --release`）→ 它们坏了就是真·发版阻塞项，这是 `TODO-11` 的原意。
- **测试代码**（`cli/uninstall/tests.rs:285`，以及本票修的那两个测试文件）**在 `release.yml` 里根本不会被编译** —— `cargo build --release` **不带 `--all-targets`**，不编测试。所以「Windows 上测试代码编不过」**从来不是发版阻塞项**，只是「这条 job 的 `--all-targets` 一上来就红」。

因此这条 job 的收益也要如实描述：它能证明**Windows 上能编**，**不能证明 Windows 上的行为**（测试里的假脚本 / `0o755` / `ps` 本来就是 unix 语义，那些测试在 Windows 上也不会跑 —— job 只跑 `cargo check`，不跑 `cargo test`）。

## 做法（两条）

### ① job 落在 `windows-latest`，不是 ubuntu

原方案（ubuntu 上 `rustup target add x86_64-pc-windows-msvc` + `cargo check --target …`）**跑不通**，本机实测（macOS，同样能代表 ubuntu）：

```
rustup target add x86_64-pc-windows-msvc        → ✓ 装上了（rust-std）
cargo check --target x86_64-pc-windows-msvc --all-targets
  → error occurred in cc-rs: … LC_ALL="C" "cc" … "--target=x86_64-pc-windows-msvc" …
    fatal error: 'assert.h' file not found      （ring 0.17 的 build script 要为目标编译 C）
```

原因：`cargo check` 只跳过**链接**，**不跳过 build script**；msvc 目标需要 Windows SDK / `cl.exe`，ubuntu 与 macOS 都没有。**这不是「换条命令」能绕过的** —— 换 `x86_64-pc-windows-gnu` 就要求装 mingw 交叉 C 工具链（本机没有；且那是另一个目标）。

`windows-latest` 上目标就是 runner 自己的 host（`x86_64-pc-windows-msvc`）→ 不需要 `target add`、不需要交叉工具链 ✓ 而且 `release.yml` 的 Windows 构建本来就跑在 `windows-latest` ✓（先例）。

### ② 顺手修两处既有的「Windows 编不过」（否则这条 job 一上来就红）

`src/cli/tests.rs` 与 `src/recipe/dup/tests.rs` 的模块声明是 `#[cfg(test)] mod tests;`（**没有 unix 门**），而文件里是**模块级**的 unix-only 引用：

- `src/cli/tests.rs:6` `use std::os::unix::fs::PermissionsExt;` + `:30` `fs::Permissions::from_mode(0o755)`
- `src/recipe/dup/tests.rs:3` 同上 + `:46` 同上

`std::os::unix` 在 Windows 上不存在 → **必然编译错**（语言事实，不是猜测）。改法：把那两行收进 `#[cfg(unix)]`（`use` 也一起收），helper 本身保留（Windows 没有「可执行位」这回事，helper 仍两边可用 —— 不把整个测试模块挪进 `#[cfg(unix)]`，免得白丢 Windows 侧的测试覆盖）。

全仓审计（见 `## Done`）已确认**只有这 4 处**未带门：其余 unix-only 引用（`app/mod.rs`、`cli/uninstall/tests.rs`、`clients/{codex,pi}/tests/*`、`clients/codex/official.rs`、`config/store.rs`、`openai_auth.rs`、`recipe/store.rs`）都在 `#[cfg(unix)]` 里 ✓。

## 验收

- [ ] `ci.yml` 多一个 job：`windows-latest` + checkout + `dtolnay/rust-toolchain@stable` + `Swatinem/rust-cache@v2` + `cargo check --locked --all-targets`
  - `--locked` 的依据：`release.yml` 已经在 `windows-latest` 上用 `cargo build --release --locked --target …` ✓，锁文件与平台无关（跨平台依赖本来就在锁里）→ 不会因平台差异红
- [ ] 4 处 unix-only 引用收进 `#[cfg(unix)]`
- [ ] 本机把**能跑的**跑完并如实记录（见 `## Done`：target 能装、check 在 ring 处失败）
- [ ] 三命令（fmt / clippy / test --all）+ MSRV `--locked` 全绿；`tests/docs.rs` 的 11 条守卫全绿
- [ ] `docs/TODO.md` 删掉 `TODO-11`

## 用哪条守卫钉住

**这条 job 自己就是守卫**，本票不新增守卫。理由：现有 11 条守卫都在 `tests/docs.rs` 里管**文档**不变量（Doc map / 目录树 / 体量 ratchet / 命令面 / 禁令唯一化），没有一条适合解析 `ci.yml`；而在文档测试里塞一条「ci.yml 里有 Windows job」的子串断言，只能保护「它没被删掉」、保护不了「它是对的」，还要把 YAML 解析搬进测试 ✗。这跟「跨平台构建由 `release.yml` 自己验证」是同一套逻辑：**让会失败的东西自己失败**。

## Done

- commit: `1e76e42`（`1e76e425555a`，`ci.yml` + 两处 gating 修复 + 删 `TODO-11`；票面回填在其后一次提交）
- **本机真跑的结果（req 3）—— 如实记录，包括跑不通的那半**：
  - `rustup target add x86_64-pc-windows-msvc` → **✓ 装上了**（本机原来只有 `aarch64-apple-darwin`）
  - `cargo check --target x86_64-pc-windows-msvc --all-targets` → **✗ 跑不完**，卡在依赖的 C 构建：
    ```
    cargo:warning=ring-0.17.14/include/ring-core/check.h:27:11: fatal error: 'assert.h' file not found
    error occurred in cc-rs: command did not execute successfully: … "cc" … "--target=x86_64-pc-windows-msvc" …
    error: failed to run custom build command for `ring v0.17.14`
    ```
    即：**卡的不是 target（rust-std 装好了），而是 Windows 的 C 工具链**；`cargo check` 只跳过链接、不跳过 build script。ubuntu runner 上同理（msvc 目标要 `cl.exe`/SDK）→ 这就是 job 改到 `windows-latest` 的依据。
  - 我也试过用假 C 工具器把它推到「只类型检查我们自己的 crate」（`CC_…=true`，再换成会 touch 产物的假 `cc`/`ar`）：ring 的 build script 会校验产物存在，最终停在 `cc-rs: Could not copy or create a hard-link to the generated lib file` → **不折腾了**，如实说：本机无法完成这条 Windows check。
  - 因此本票在「本机可验证」的部分做到的是：target 装得上✓、命令形式正确✓（它把整张依赖图编到 ring 才断）、依赖树无 unix-only 残留✓（见下）。**这条 job 的第一次真实运行，会发生在下一次 push/PR** —— 我不假装跑过它。
- **审计（替代本机编译的证明，本次发现 2 轮）**：全仓扫 unix-only 的**符号与方法**（`std::os::unix` / `PermissionsExt` / `from_mode` / `.mode()` / `set_mode` / `symlink` / `OpenOptionsExt` / `libc::` / `std::os::fd`）并检查是否在 `#[cfg(unix)]` 里：**改完之后 0 处未带门**。
  - 第一轮我漏了 `.mode()`（只 grep 了 trait 名与 `from_mode`）→ 被**编译器当场拓住**（E0599: no method named `mode`），这也说明「审计靠 grep 不够、编译器才是权威」；改成扫 trait 方法后才齐全。
- 验证（本机 host 路径）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0
  - `cargo test --all` → `335 passed; 0 failed; 4 ignored` + 守卫 `11 passed`
  - `cargo +1.88 check --locked --all-targets` → 编过
  - `ci.yml` 结构核对：3 个 job（`check` / `msrv` / `windows`），新 job 与 `msrv` 同风格；`--locked` 与 `release.yml` 的 Windows 构建同口径
- **偏离 spec（原方案）：一处，是必不得已。** 原定「ubuntu runner + `rustup target add` + `cargo check --target x86_64-pc-windows-msvc`」**物理上跑不通**（上面那条 ring 错误）。改用 `windows-latest` + native host target：不需要交叉工具链、不需要 `target add`，且 honor 了 `--locked`（与 release.yml 一致）。我只在 ubuntu 上找得到替代：`x86_64-pc-windows-gnu` + mingw 交叉（本机没有 mingw，要 `brew install mingw-w64`；而且那是另一个目标，不是发布用的 msvc）—— 如果你更想要「本机跑过」而非「跟发布目标一致」，告诉我，我换成那条。
- **本票的检查边界（有意）**：Windows job 只跑 `cargo check`，**不跑 `cargo test`** —— 它管的是「Windows 上能编译吗」，不是「Windows 上测试跑得过吗」（测试里有 `sh` 脚本 / 可执行位这类 unix 语义）。这与 `TODO-11` 的原话（「编译错要发版才暴露」）一致。
- **首跑红，已在票 02 修掉（同一目录）**：`## Done` 上面那段「否则这条 job 一上来就红」当时只说了修 `src/{cli,recipe}` 两处，**没发现还有 5 个测试模块引用了 unix 门内的 helper**（我只查了「unix-only 符号有没有带门」这一个方向）→ CI run `38037195245` 红。教训：**cfg 审计要双向查**，方法已写进票 02（`audit-cfg.py` 也放在本目录）。
- 本票的 `现状` 里那句「编译错要发到 5 平台构建里才现身」已改正为「产品代码是发版阻塞项；测试代码 release.yml 根本不编」—— 见上面「分清楚两类」。
