# 01: 文档守卫骨架 + 目录树 ↔ `src/` 一致性

Status: ready-for-agent
Blocked by: 无（可立即开工）

**What to build:** 仓库第一次有了「文档不变量由机器守」的执行路径 —— 一个顶层集成测试二进制，跑 `cargo test` 就会执行；它用**显式的根目录列表**读文档与源码（绝不全仓递归），本次落地第一条不变量：AGENTS.md 的目录树与 `src/` **双向**一致。跑 `cargo test` 绿；故意在树里加一个不存在的路径、或藏起一个真实存在的非测试源文件时它会红并点名叫出来。

- [ ] 新增顶层集成测试二进制（仓库根的 `tests/` 目录；这是本特性唯一的新 seam），`cargo test` 与 `cargo test --all` 都会执行，**不新增 CI 步骤**
- [ ] 文件发现走显式根目录列表（仓库根、`docs/`、`.agents/skills/`、`.scratch/`），并跳过 `target/`、`.delta/`、`.pi/`；**不得全仓递归**（本机 `.delta/worktrees/` 下有整仓副本，含同名 `AGENTS.md` 与 `docs/**`，全仓扫描必假红）
- [ ] 断言方向一：目录树里每个 `src/` 路径都真实存在
- [ ] 断言方向二：`src/` 下每个**非测试** `.rs` 文件都被目录树点名（`tests/` 目录与 `tests.rs` 是测试代码，按目录归拢、不要求逐个进树）
- [ ] 「目录结构」那段的标题写明这条覆盖口径，使「什么算漏」有明示定义
- [ ] 失败信息点名具体文件路径（例：「目录树缺少 `src/cli/uninstall/report.rs`」）
- [ ] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
