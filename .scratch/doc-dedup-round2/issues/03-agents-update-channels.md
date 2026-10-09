# 03: `apim update` 的渠道契约搬进 RELEASING，约定 12 只留一行（`TODO-20`）

Status: done
Blocked by: 无（可立即开工）

**What to build:** `AGENTS.md` 里那条约 1000 字符的「`apim update` 只认三条渠道」，机制细节搬进 `docs/RELEASING.md`（渠道表与发布期坑本来就在那儿），`AGENTS.md` 只留「三条渠道 + 指针」；并**同步下调** `AGENTS.md` 的 ratchet 上限，让省下来的预算不被后来的沉积吃掉。

**① 重复位置**：`AGENTS.md` 第 **127 行**（约定 12，单行约 1000 字符）。

`docs/RELEASING.md` **已经有**（不必搬）：`install.sh.sha256` 资产与用途（3 处）、`detect_channel`、`uninstall_program` 的穷尽 `match`、「`target/` 下的开发构建会被拒绝更新」、`各渠道速查` 表。

`RELEASING.md` **还没有、必须搬过去的**（逐条落）：

1. 渠道识别规则：npm 看 `node_modules/apim-cli`、brew 看 `Cellar/apim`、其余当 install.sh 装的裸二进制；
2. **install.sh 渠道不是 `curl | sh`**：URL 钉到本次要更新到的 tag → 下载到临时文件 → 形状校验（是 shell 脚本 / 是本仓库安装器 / 含 sha256 校验）→ 按 Release 的 `install.sh.sha256` 校验摘要（**拿不到摘要就拒绝执行**）→ 才 `sh <file>`；
3. `APIM_INSTALL_DIR` 钉在当前二进制的**真实位置**（先 canonicalize，否则符号链接会被替换掉）；
4. **npm 渠道更新前要同时核对主包与当前平台子包的版本**（`npm view <pkg> version`；npm 发布异步、optional 依赖失败静默跳过 —— 只看主包会装出跑不起来的 shim，v0.1.4 实测）；落后于 GitHub tag 时报两个版本号并拒绝安装（`--force` 可越过）；
5. 加渠道的清单：同时改 `Channel` + 识别规则 + 单测 + RELEASING 的表；`apim uninstall` 复用 `detect_channel`，新渠道的卸载动作被 `uninstall_program` 的穷尽 `match` 拦下（编译器逼你补），但**提示语与单测仍要手工过一遍**。

**② 新家**：`docs/RELEASING.md` 新增一节 **`## 更新渠道的契约（`apim update` / `apim uninstall`）`**，插在 `## 各渠道速查` **之后**（表与契约相邻，加渠道时一起改）。**不新开 `docs/update-channels.md`**：那会拆散「表 + 契约」，还要多一行 Doc map。

**③ 原地留什么指针**：`AGENTS.md` 约定 12 一行，保住「三条渠道 + 机制有家 + 什么时候读」：

> 12. **`apim update` 只认三条渠道**（install.sh / npm / Homebrew）：渠道识别规则、三条硬约束（钉 tag + 摘要校验、npm 主包与平台子包版本交叉核对、`target/` 与 `~/.cargo/bin` 一律不更新）与「加渠道要同步改什么」见 `docs/RELEASING.md` 的「更新渠道的契约」一节 —— 动 `src/cli/update/` 或 `src/cli/uninstall/` 之前先读它。

Doc map 那一行的描述同步扩成「发版手册（含更新渠道的契约）/ 额度脚本提示词」。

**④ 守卫**：
- **复用现有第 4 条（`AGENTS.md` ratchet）**，并且**本票负责下调上限**：按实施后的实测值收紧 `AGENTS_LINES_MAX` / `AGENTS_CHARS_MAX`（预计字符数少约 900），注释仍写「只允许变小或保持」。
- **新增第 11 条** `the_update_channel_contract_lives_in_its_doc`：`AGENTS.md` **不得**再出现 `detect_channel` / `install.sh.sha256` / `APIM_INSTALL_DIR` / `npm view` / `Cellar/apim` / `node_modules/apim-cli`；`docs/RELEASING.md` **必须含** `detect_channel` 与 `install.sh.sha256`（证明是**搬**过去了，不只是删了）。
- 失败信息：`problem(file, RULE, "<多了/少了哪个标识符>", "机制搬进 RELEASING 的「更新渠道的契约」一节；AGENTS.md 只留一行 + 指针")`。

- [x] 在 `docs/RELEASING.md` 的「各渠道速查」之后新增「更新渠道的契约」一节，上面 5 条逐条落进去（并把原来那句部分重复的长引用**合并进去**，不留新重复）
- [x] `AGENTS.md` 约定 12 换成那一行指针；Doc map 的描述同步扩写
- [x] **下调** `AGENTS_CHARS_MAX` 到实测值：**12508 → 11848**（行数上限 181 不变 —— 约定 12 本来就只占 1 行）
- [x] 新增第 11 条守卫（禁用标识符 + RELEASING 必含标识符），失败信息三件套齐全
- [x] 反向变更各自单独验（见下方 Done 的原文；③ 的写法按实际情况改了，见偏离 ①）
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿（守卫二进制 11 条）
- [x] 收尾（spec 里的动作，不额外开票）：`TODO-18/19/20` 已从台账删掉；另按你的要求新开 `TODO-21`

## Done

- commit: `04feaab`（`04feaab8c3cc`，`AGENTS.md` + `docs/RELEASING.md` + `tests/docs.rs`）+ `c201d87`（收尾：台账删 18/19/20、开 `TODO-21`）；票面回填在其后一次提交
- 验证（本机）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0
  - `cargo test --all` → `335 passed; 0 failed; 4 ignored` + 守卫 `11 passed`
  - `AGENTS.md`：**181 行 / 11848 字符**（上限 = 181 行 / **11848** 字符，本票把字符上限从 12508 收到实测值）
- **反向验证**（各自单独跑，实测报错如下，验完恢复）：
  - A：把 `（`APIM_INSTALL_DIR` 先 canonicalize）` 写回 `AGENTS.md` → **两条同时红**：`the_update_channel_contract_lives_in_its_doc` 报 `出现了只在机制节里的 APIM_INSTALL_DIR / 怎么修：把机制搬进 docs/RELEASING.md 的「更新渠道的契约」一节`；`agents_md_stays_within_its_budget` 报 `字符数 11883 超过上限 11848（多了 35 字符）`
  - B：把 RELEASING 那节的 `detect_channel` 换掉 → `the_update_channel_contract_lives_in_its_doc` **FAILED**，报 `docs/RELEASING.md：「更新渠道的契约」一节里没有 detect_channel（机制没搬过来？）`
  - C：往 `AGENTS.md` 标题加**一个字符** → `agents_md_stays_within_its_budget` **FAILED**，报 `字符数 11849 超过上限 11848（多了 1 字符）`—— 证明收紧后的上限是**紧的**，不是留了余量的数字
- **偏离 spec：四处，前三处是票面写错/写得不准。**
  1. **票面的反向变更 ③（「把 ratchet 上限调回旧值 → 应红」）写错了**：上限是**最大值**，把它调大永远不会红。改成「往 `AGENTS.md` 加一个字符 → 红（11849 > 11848）」，这才是「收紧」的正确证法；「上限确实下调了」靠 diff 证明（12508 → 11848）。
  2. **实测省下 655 字符，不是 spec 里预计的约 900**（12503 → 11848）。约定 12 的机制里有不少是一句话就能说完的短约束，压不成更短。
  3. **行数上限没变**（181）：约定 12 本来就是**一行**（只是很长），所以这次瘦身全在字符数上 —— 又一次说明「行数上线」单独守不住，字符才是常驻上下文的成本（`ADR-0010` 里也是这个理由）。
  4. **顺手合并了 RELEASING 里原有的一句部分重复**：`各渠道速查` 下面那段「`apim update` 只认三条…」（含三条渠道、install.sh 摘要校验、`target/` 不更新、`detect_channel`、穷尽 `match`）与新写的契约节重叠 → 压成一句引导语 + 指向新节，否则本票会在 RELEASING 内造出**新的**重复。

## Done

（实施后回填：commit sha / 三条验证命令与结果 / 反变换记录 / 偏离）
