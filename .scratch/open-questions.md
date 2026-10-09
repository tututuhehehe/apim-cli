# 未决问题裁决清单

**角色**：跨两个在制品（`TODO-1` OAuth 凭据按厂商存 / `docs-single-source` 文档单一来源）的**裁决清单**。
来源：`wC:p3`（agent `apim-skills`）在 grilling / to-spec / to-tickets 三轮里积累的 28 条未答问题。
**答案落回各自的 spec / ticket / TODO 之后，本文件退役** —— 它不是长期文档，别往里追加新问题。

- ✅ = 已答复（理由与去向下述）
- ⏳ = 需要人类拍板（我给了倾向，但你说了算）

**全部已定**（2026-10-09）。A07 由用户拍板：**竞态本轮修、回写也做**；其余 27 条按本文件答复执行。
实施方仍是 `wC:p3` 那个 pi —— 它读这份文件执行，不需要你在 pane 里重述。

---

## A 组 · TODO-1：OAuth 凭据按厂商存之后，按 `x` 导入 Codex 官方路该写哪一份

> **决议（2026-10-09，用户口径）：`TODO-1` 关闭 → `docs/adr/0008`。**
> OAuth（`AUTH` 行 + `x` 导入官方路）**只服务内置 `openai`**，是特殊通道，不发给其他厂商；凭据保持单份；继续禁止复制内置 `openai`。
> 因此 A02 / A03 / A05 / A06 / A08 / A09 / A11 / A12 / A13 **随之失效**，下面保留原文只作决策留档。

### A01 单槽约束：接受还是本轮就做「多登录位」
- 影响：决定后面全部 A 组条目的形态。
- 选项：a) 接受单槽；b) 本轮做多登录位（= apim 管多个 `CODEX_HOME`）。
- **答复（用户口径，2026-10-09）：保持现状**（✅）。`x` 只出现在内置 `openai` 的 `AUTH` 行上，语义就是「通过 OpenAI 官方方式给 Codex 用」；不做多登录位、不做多 `CODEX_HOME`。

### A02 凭据的键：`provider id` 还是 `provider.alias`
- 影响：能否同时保有同一厂商的多个账号；AUTH 行 / ★ 对账 / 导入面板是否要多一层选择。
- 选项：a) 按 provider id（`openai-oauth.<id>.json`）；b) 按 alias（与密钥对称，多一层 UI）。
- **答复：本条失效**（✅）。`AUTH` 行只服务内置 `openai`，凭据保持**单份**（`openai-oauth.json` + 伴生 host-id），不按 id 分表 —— 见 `docs/adr/0008`。

### A03 OAuth 能力怎么声明
- 选项：a) recipe 加 `oauth: openai_codex`（serde default 无；编辑表单只读，与 `kind` 同口径）；b) 继续硬编码 id。
- **答复：本条失效**（✅）。不加 `oauth:` 能力字段：既然能力只给内置 `openai`，那 6 处「id 正好是 openai」的门就保持原样（`docs/adr/0008`）。

### A04 核心：按 `x` 写谁的凭据
- 影响：`x` 的语义、AUTH 行分布、导入后提示语；动 `app::open_import` / `clients/codex/official` / `ui::{keys,balance}` / README 导入章节。
- 选项：a) 写**当前 AUTH 行所在厂商**那份（覆盖前照旧备份，代价靠文案承担）；b) 固定写内置 `openai`；c) 导入时让用户选。
- **答复（用户口径）：保持现状**（✅）。`x` 只存在于内置 `openai` 的 `AUTH` 行；「在哪个厂商页按就写谁的凭据」这个泛化**不做**（其他厂商根本没有这个条目/功能）。

### A05 覆盖之后 A 的凭据留不留
- 选项：a) 保留、不删不问；b) 一并删；c) 弹窗问。
- **答复：本条失效**（✅）。没有第二个厂商的凭据，也就不存在「顶掉 A」。

### A06 旧单文件怎么迁
- 选项：a) 读时迁移（发现旧文件且 id 是 `openai` → rename，host-id 同）；b) 启动时一次性迁移；c) 不迁。
- **答复：本条失效**（✅）。文件布局不变，不需要迁移。

### A07 本轮范围：回写与竞态做不做
- 选项：a) 回写不做、`§3.9` 竞态本轮一起修；b) 都做；c) 都不做。
- **答复（用户拍板，2026-10-09）：两件都做，同一轮**（✅）。已立项为 `.scratch/oauth-credential-sync/`（spec + ticket）。
  ① **竞态**（`TODO-12`）：`fetch_usage` 的「load → 可能刷新 → save」读改写与登录落盘互相覆盖 → 用户可见后果是**底栏说「已连接」但凭据已被写回旧的那份**，之后额度查询会失败/要求重新登录。修法二选一：`save` 里做 compare-and-swap（`WRITE_LOCK` 内重读，`client_id` + `refresh_token` 变了就跳过），或登录在途时不派发 OAuth 探针。
  ② **回写**（`TODO-2`）：Codex 自己刷新 token 后只写回 `~/.codex/auth.json`，apim 那份落后。**必须认 ownership**（只有 `auth.json` 里的 refresh token 就是我们写进去的那份才回写；用户自己 `codex login` 的另一个账号绝不能抄进 apim），并挂在 `refresh_active_keys` 的节奏上。

### A08 多厂商 AUTH 状态：单槽还是 `HashMap`
- 选项：a) 跟着当前厂商走（切厂商重查）；b) `HashMap<String, _>`。
- **答复：本条失效**（✅）。保持现状（单槽状态跟当前厂商走）—— 因为只有 `openai` 有 `AUTH` 行。

### A09 ★ 与「Codex：…」的对账口径
- 选项：a) 回读 `auth.json` 的 **refresh token** 与 apim 各厂商凭据对账，显示「Codex：官方 OAuth（provider A）」，对不上显示「非 apim 写入的外部登录」；b) 只显示「官方 OAuth」。
- **答复：保持现状**（✅）。`codex_route` 不必说出「是谁的凭据」：只有一份 openai 凭据，不存在 A/B 对账问题。

### A10 「内置 `openai` 不可复制」这条闸要不要放开
- 选项：a) 放开 + 同轮补上单槽提示；b) 保留闸。
- **答复（用户口径）：保留闸，不放开**（✅）。`AUTH` 条目是特殊通道，放开复制会牵扯较多（官方登录位单槽 + 导入路径 + 备份回滚 + 文档双语），暂不做。见 `docs/adr/0008`。

### A11 导入前要不要加确认
- 选项：a) 不加通用确认；未登录时按 `x` 提示「先按 `o`」；b) AUTH 行加确认弹窗；c) 未登录时直接开登录。
- **答复：本条失效**（✅）。`x` 的交互不变（无确认弹窗；未登录时按 `x` 仍按现状提示）。

### A12 CLI 形状
- 选项：a) 位置参数 `apim auth <provider> login|status|logout|import-codex`，缺省 `openai` 兼容旧命令；b) `--provider <id>`。
- **答复：本条失效**（✅）。CLI 形状不变：`apim auth openai login|status|logout|import-codex`。

### A13 导入面板要不要「目标厂商」这层信息
- 选项：a) 不加；b) 加一行只读「目标厂商」；c) 加一步可改。
- **答复：本条失效**（✅）。导入面板不加「目标厂商」信息。

---

## B 组 · docs-single-source：口径差异、新事实、显式冲突

### B01 「21.5k 字符」是整份 AGENTS.md，不是「目录结构」一节
- **我的答复：接受核过的数（a）**（✅）。整份 226 行 / 22.5k 字符；目录结构 88 行（39%）、核心约定 66 行、Doc map 20 行。这条是 retro 报错了数，spec 用它自己的数。

### B02 约定 11/14/15 是 42 行不是 55 行
- **我的答复：用核过的数（a）**（✅）。42 行 = 核心约定那节 66 行的 **64%**、全文 19% —— 比例比绝对行数更能说明问题，spec 里就按这个口径写。

### B03 约定 15（厂商类型）需要第三个家
- **我的答复：a**（✅）。新建 `docs/provider-kinds.md`，并在 Doc map 补一行「厂商类型 / 模型与非模型的差别」→ 塞进 `docs/clients/` 会让读者按「客户端」去找它。

### B04 `apim tui` 在 README 里一次都没出现
- **我的答复：a**（✅）。补 README 的 `apim tui` 一行（顺带把 `--version` 写全），**中英双版同步**。别名是真实用户入口，README 缺它就是缺；让守卫排除别名是掩盖问题。

### B05 SKILL 没有任何 Doc map 行
- **我的答复：a**（✅）。Doc map 补一行「项目技能 / 面向 agent 的用法包：`.agents/skills/*/SKILL.md`」。skill 有名字有角色，本来就该在 Doc map 里；豁免 skill 目录等于承认 Doc map 不完备。

### B06 约定 13 vs Doc map：客户端契约的家
- **我的答复：a**（✅）。契约的家 = `docs/clients/*.md`（能整段读完、能被 diff 命中）；Rust 子模块文档只放一行指针；**约定 13 改成指针措辞**。Doc map 是后立的规则，取它，但要在同一轮把约定 13 改掉，不能两处并存。

### B07 与 ADR-0007 的关系
- **我的答复：a + b**（✅）。spec/票里明确区分「代码重复（ADR-0007 容忍）」与「文档事实重复（本次要消）」；**并且新开一条 ADR-0009「文档事实单一来源」**（0008 已被 OAuth 单一提供者决定占用），把 Doc map 这条 repo 级规则记成决策 —— 否则以后一定有人拿 ADR-0007 来挡文档去重。

### B08 `docs/TODO.md` 里指向 `target/apim-review/` 的悬空引用
- **我的答复：a**（✅）。新开 `TODO-16`：把评审结论内联进相应条目或落到 `docs/`，删掉指向 `target/`（它在 `.gitignore` 里，一条 `cargo clean` 就没）。顺手在票 10（收尾）里做掉。

### B09 AGENTS.md「运行时数据」与 README「Where the data lives」重叠
- **我的答复：a**（✅）。纯用户向的内容（目录在哪、权限、怎么备份）留给 README；AGENTS 只留开发者视角（内部状态、env 变量、日志路径）。作为票 07 的审计项执行。

---

## C 组 · to-tickets：拆票与执行决策

### C01 03（Codex 契约）与 04（Pi 契约）拆开还是合并
- **我的答复：a 保持拆开**（✅）。5 个票可立刻并行是这次拆票最大的收益；合并后它会是本组最大的一票，还拖慢 Pi 那侧的开工。

### C02 04 要不要 `Blocked by: 03`
- **我的答复：a 不加边**（✅）。骨架顺序已由 spec 固定，两票改的是 AGENTS.md 的不同段落、不同新文件；实现时若真撞车，在 04 里顺手统一即可 —— 不值得为「可能的一致性」把并行度砍掉。

### C03 08 与 09 是否合并
- **我的答复：a 保持拆开**（✅）。08 只等 06/07，09 等的面更宽；合并只是账面好看，还让 08 的守卫推迟落地。

### C04 守卫的载体：`tests/` 集成测试 vs CI 脚本
- **我的答复：a 顶层 `tests/` 集成测试二进制**（✅）。它一次搭上三条既有执行路径（本地 `cargo test`、CI 的 `cargo test --all`、MSRV job 的 `--all-targets`），且不引入第二种语言。两个附带要求：① AGENTS.md 的目录结构要补 `tests/` 一行（否则目录树守卫自己就红）；② 守卫失败信息要写清「哪个文件、哪条规则、怎么修」。

### C05 `.scratch/docs-single-source/` 现在提交吗
- **我的答复：a 现在提交**（✅）。`docs/agents/issue-tracker.md` 明确要求 `.scratch/` 进 git（别的机器与评审 agent 要看得到）。连同本文件一个提交。

### C06 要不要补 triage 词汇表
- **我的答复：b 暂不装 `triage` 技能**（✅），但**在 `docs/agents/issue-tracker.md` 补一行**：`Status:` 取值用规范五值（`needs-triage` / `needs-info` / `ready-for-agent` / `ready-for-human` / `wontfix`）。这样票里的 `ready-for-agent` 不悬空，也不用为一个取值装一整套 triage 流程。

---

## 答复之后的下一步

1. **A 组**：`TODO-1` 已关闭（`docs/adr/0008`）。A07 已定 → 新在制品 `.scratch/oauth-credential-sync/`（覆盖 `TODO-12` 竞态 + `TODO-2` 回写），与 `docs-single-source` 并行。
2. **B 组**：答案已写进 `docs-single-source/spec.md` 的口径要求里，按票 01→10 顺序执行；`ADR-0008` 与 `TODO-16` 顺手立起来。
3. **C 组**：票 01/03/04/05/06 即 frontier，可 5 个上下文并行（本会话不并行，交给 `wC:p3` 串行做也可）。
4. 收尾：`docs/TODO.md` 的 `TODO-1` 划掉、`CHANGELOG.md` 记账、本文件退役。
