# 09: 守卫 —— 无孤儿文档

Status: done
Blocked by: 01, 03, 04, 05, 07

**What to build:** 仓库里不再有「谁都不指向」的文档（上一版的 `HANDOFF.md` 就是这类）：每个文档都能从 AGENTS.md 的 Doc map 出发**最多一跳**到达。这条规则今天还会抓到一个真实缺口 —— SKILL 没有任何 Doc map 行（由 07 补上）。

- [x] 仓库内每个长期 `.md` 从 Doc map 出发**最多一跳**可达（第一跳：Doc map 那几行点到的；第二跳：被点到的索引文件再点名的）
- [x] `docs/adr/*.md` 由 Doc map 的 glob 直达，且 `docs/adr/README.md` 的**相对链接**也算一跳（索引里的 `0001-x.md` 就是这么写的）；`.scratch/<feature-slug>/` 按类覆盖（在发现范围外）
- [x] `DEV-NOTES.local.md`：存在且已在 Doc map 里点名；不存在也会静默通过（发现是扫目录，不读到就不参与）
- [x] 文件发现沿用显式根目录（仓库根非递归 + `docs/` + `.agents/`），跳过 `.scratch/`、`target/`、`.delta/`、`.pi/`
- [x] **首次运行全绿**（现状本来就都可达）；另用「造一个孤儿」验过它会红
- [x] 失败信息点名「哪个文件没有家」+ 怎么修
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿（按轮次规矩跑的三条见下方，更严）

## Done

- commit: `d6941f2`（`d6941f201028`，`tests/docs.rs` + `AGENTS.md`；票面回填在其后一次提交）
- 验证（本机）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0
  - `cargo test --all` → `335 passed; 0 failed; 4 ignored` + 守卫 `8 passed`
  - `AGENTS.md` 体量：**181 行 / 12503 字符**（在 ratchet 上限内）
  - **反向变更．A**（造一个 `docs/orphan-probe.md`）→ `every_durable_document_is_reachable_from_the_doc_map` FAILED，点名它是孤儿
  - **正对照．C**（把 ADR 行改成只指 `docs/adr/README.md`）→ 第二跳应该兜住九条 ADR。**第一遍它红了** —— 暴露一个真 bug：第二跳只认仓库相对引用，而 `docs/adr/README.md` 里的链接是**相对自己目录**的（`0001-x.md`）。补上「引用按引用者目录再试一次」后 C 变绿、A 仍红。这个 bug 正是「先写正对照」的价值
- **偏离 spec：两处，均为把口径写准。**
  1. **「可达」= 两跳（Doc map → 它点到的文件 → 那些文件再点名的）**，与你 batch 5 的补充口径一致。它的一个**有意后果**：把 Doc map 里指向 `docs/provider-kinds.md` 的那行删掉，守卫**不会**红 —— 因为 `docs/clients/codex.md`（可达）里引了它一句。也就是说「有文档提到我」就算有家，不强制每个文档占 Doc map 一行。我按你写的「或它指向的文件」保留了这层（实测过、写在这里备查），想要严到「必须占 Doc map 一行」的话就是另一条规则。
  2. **顺手把 Doc map 的 ADR 行从 `docs/adr/NNNN-*.md` 改成 `docs/adr/*.md`**：原写法是给人看的散文（`NNNN` 不是通配符），匹配器认不了；改后真实 glob 才当得上「一个家」。代价：明确性略降（`*` 包含未来可能的非 ADR 文件）—— 可接受，因为那条行的角色描述已经说了那里装什么。
