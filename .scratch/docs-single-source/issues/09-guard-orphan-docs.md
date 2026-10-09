# 09: 守卫 —— 无孤儿文档

Status: ready-for-agent
Blocked by: 01, 03, 04, 05, 07

**What to build:** 仓库里不再有「谁都不指向」的文档（上一版的 `HANDOFF.md` 就是这类）：每个文档都能从 AGENTS.md 的 Doc map 出发**最多一跳**到达。这条规则今天还会抓到一个真实缺口 —— SKILL 没有任何 Doc map 行（由 07 补上）。

- [ ] 仓库内每个 tracked `*.md` 从 Doc map 出发最多一跳可达
- [ ] `docs/adr/*.md` 由 `docs/adr/README.md` 索引即算可达；`.scratch/<feature-slug>/` 按类覆盖，不要求逐文件进 Doc map
- [ ] 本机私有的 `DEV-NOTES.local.md` 用**包含式**断言：存在则必须在 Doc map 里点名，不存在则静默通过 —— 这样 CI（无该文件）与开发机行为一致
- [ ] 文件发现沿用 01 的显式根目录列表，跳过 `.delta/`、`target/`、`.pi/`
- [ ] 首次运行时记录的现状全绿（03 / 04 / 05 / 07 落地之后）
- [ ] 失败信息点名「哪个文件没有家」
- [ ] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
