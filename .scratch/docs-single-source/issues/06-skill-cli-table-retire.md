# 06: SKILL 命令表下线 + README 指针 + 禁令修句

Status: ready-for-agent
Blocked by: 无（可立即开工）

**What to build:** 用 skill 的 agent 不再读到一份**已不存在的 CLI**：`.agents/skills/apim/SKILL.md` 删掉那份手抄的命令表，改为指向 README 的 CLI 章节；它只保留别处没有的知识（额度脚本契约）。同时修掉 SKILL 里那句与 AGENTS.md 约定 10 相反的安装说明。

- [ ] 删掉「CLI 命令速查」表；SKILL 里残留的 `apim` 调用只作**流程示例**（注册厂商 + 绑定脚本、`apim status <id> --json` 验证），不定义签名
- [ ] SKILL 顶部（「apim 是什么」那句附近）加一条指向 `README.md` CLI 章节的指针
- [ ] `cargo install --path .` / `target/debug/apim` 那句改为「跑本地代码用 `cargo run -- <args>`（见 AGENTS.md 约定 10）」
- [ ] `docs/quota-script-prompt.md` 里同一句禁令一并改掉（R4 会扫到它）
- [ ] recipe 字段表裁到只剩额度脚本相关的 `balance` / `vars`，其余（`base_url` / `homepage` / `models_url` / `auth.kind` / `health`）指向 README
- [ ] frontmatter 的 `description` 收敛为它真正专长的事：额度查询脚本的编写与绑定
- [ ] 额度脚本独有的知识**一条不丢**：env 注入表、输出契约（exit 0 / 非 0 / 超时 / 截断）、已知坑、mock 三步流程、写脚本自查清单
- [ ] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
