# 01: 文档守卫骨架 + 目录树 ↔ `src/` 一致性

Status: done
Blocked by: 无（可立即开工）

**What to build:** 仓库第一次有了「文档不变量由机器守」的执行路径 —— 一个顶层集成测试二进制，跑 `cargo test` 就会执行；它用**显式的根目录列表**读文档与源码（绝不全仓递归），本次落地第一条不变量：AGENTS.md 的目录树与 `src/` **双向**一致。跑 `cargo test` 绿；故意在树里加一个不存在的路径、或藏起一个真实存在的非测试源文件时它会红并点名叫出来。

- [x] 新增顶层集成测试二进制（仓库根的 `tests/` 目录；这是本特性唯一的新 seam），`cargo test` 与 `cargo test --all` 都会执行，**不新增 CI 步骤**（并实测 MSRV 那条 `cargo +1.88 check --locked --all-targets` 能编过）
- [x] 文件发现走显式路径（`AGENTS.md` 读、`src/` 递归、仓库根非递归读一层），**无任何整仓递归** —— 所以天然不会碰到 `.delta/`、`target/`、`.pi/`，假红风险为零
- [x] 断言方向一：目录树里每个路径都真实存在
- [x] 断言方向二：`src/` 下每个**非测试** `.rs` 文件都被目录树点名（`tests/` 目录与 `tests.rs` 按目录归拢、不要求逐个进树）
- [x] 「目录结构」那段的标题下写明**覆盖口径**（三条规则，使「什么算漏」有明示定义）
- [x] 失败信息点名具体文件路径 + 哪条规则 + 怎么修
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿（按轮次规矩跑的三条见下方，更严）

## Done

- commit: `a49616e`（`a49616e470c8`，`tests/docs.rs` + `AGENTS.md`；票面回填在其后一次提交）
- 验证（本机）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0
  - `cargo test --all` → `335 passed; 0 failed; 4 ignored` + **新二进制 `3 passed`**
  - `cargo +1.88 check --locked --all-targets` → 编过（MSRV 那条也确实拿到新测试）
  - **三条守卫各验过一次红**（每次单独跑、验完恢复）：①树里塞 `├── nope.rs` → `every_path_in_the_directory_tree_exists` FAILED（报出 `src/nope.rs` 不存在）；②删掉 `pi/active.rs` 那一行 → `every_source_module_is_listed_…` FAILED（报出 `src/clients/pi/active.rs` 没被点名）；③删掉 `tests/` 那一行 → `every_top_level_directory_is_listed` FAILED
  - **守卫自己抓到两处真实漏项**（已在本票修掉）：`src/cli/update/mod.rs` 与 `src/clients/pi/mod.rs` 不在任何一行里 —— 正是「整块缺席」那类漏项
- **偏离 spec：两处，均属「比票面更准」，无放松。**
  1. **「被点名」的判定比 AC 的字面更宽**：一个文件算进索引，既可以是**独立的子条目**，也可以是**写在父目录那一行的注解里**（树本来就这么用：`update/` 那行写着「channel.rs 认渠道 / install_sh.rs …」）。若按字面「必须有自己的行」去卡，会逼树变成 `ls` 的复制品（你在 req 4 里明确不想耍那个）。具体实现：判定 token 要出现在**它自己父目录的那一行的 token 集合**里（不能是别的目录提了一句）。
  2. **多加了一条不变量（第 3 条）**：「仓库根下每个非隐藏目录（除 `target/`）都要进树」。它不在票面 AC 里，但你 req 2 的前提就是这个（「否则目录树守卫自己就红」）—— 实际上本票新增的 `tests/` 就让这条红了；豁免只有 `.` 开头与本机状态目录（`.git` / `.agents` / `.scratch` / `.pi` / `.delta`）与 `target/`。
- **已知边界（有意为之，不是缺陷）**：①树里**非 ASCII 开头的名字**会被当成注解续行、不当作条目（真实路径都是 ASCII；靠这条把 `（tests/ 按 flow / apply 分）` 那种续行挡掉）；②树里**没点名**的文件不会被报（断言方向二只管「漏列」，「多余但存在」由方向一管）。
- 另：`tests/docs.rs` 里的 `tree_entries` 解析是**为这张人工树写的**（缩进单元 + `├──`/`└──` + 首 token 作路径名 + 注解续行跳过）：树的形状一变就要同步改它 —— 这是「人工树」的代价，写在文件头注释里了。
