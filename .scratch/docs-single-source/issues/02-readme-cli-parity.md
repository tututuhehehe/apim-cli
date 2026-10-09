# 02: README 命令表与 `apim help` 对齐

Status: done
Blocked by: 01

**What to build:** 用户照 README 敲命令不会撞上「`apim help` 里有、README 里没有」。今天 `apim help` 列出的别名 `apim tui` 在 README 里一次都没出现；补上它（顺带把 `--version` 写全），并由守卫钉住：从 `apim help` 的**真实输出**提取的动词集合，必须是 README 命令表动词集合的子集。

- [x] README 补 `apim tui` 一行（`--version` 两版本来就写着，无需补）
- [x] `README.zh-CN.md` 同步（两版各 +1 行，未动其它句子）
- [x] 守卫从真实 `apim help` 输出取值（跑二进制的**外部行为**，`CARGO_BIN_EXE_apim`），不读源码文本、不依赖内部函数
- [x] 这条规则**先从红变绿**：加完守卫就跑，正好红在 `apim tui`（两版各一条）
- [x] 断言只比动词集合（`apim` 后第一个词，别名 `keys`→`key` / `upgrade`→`update` 并入同一项），不比对措辞、行号或段落顺序（也不按小节标题定位 —— zh 的标题是译过的）
- [x] 失败信息点名缺哪个动词 + 哪个文件 + 怎么修
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿（按轮次规矩跑的三条见下方，更严）

## Done

- commit: `9f98f62`（`9f98f6222c3a`，`tests/docs.rs` + 两份 README；票面回填在其后一次提交）
- 验证（本机）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0
  - `cargo test --all` → `335 passed; 0 failed; 4 ignored` + 守卫 `5 passed`
  - **先红后绿**：只写守卫、不补 README 时恰好红在 `apim tui`（`README.md` 与 `README.zh-CN.md` 各一条）；补一行后 5 passed
  - **反向变更**（验完恢复）：给 `apim help` 的文本里加一个假子命令 `frobnicate` → 两版 README 各报一条「`apim help` 里有子命令 `frobnicate`，README 里一次都没出现」+ 怎么修（这一刀模拟的才是真正的失效模式：加了子命令、README 没跟上）
- **偏离 spec：两处，均为「比票面更准」。**
  1. **README 那一侧不按小节限定**：票面 AC 说「README 命令表」，实做是「两份 README 全文」。理由：`README.zh-CN.md` 的小节标题是译过的（`## CLI（AI / 脚本友好）`），按标题定位等于把守卫焊死在中文字符串上（改个标题就假红）——这与 spec 的测试决定「只断言结构与集合、不断言译文措辞」相同。代价：一句跑题的提及也能算“已记录”；但真实失效模式是「根本没人写」，这一版能抓。
  2. **`--version` 未新增**：票面 AC 写「补齐 `apim --version`」，实际上两版 README 的 Usage 块里早有 `apim --version`（守卫也确认了），所以本票没为它添行。
