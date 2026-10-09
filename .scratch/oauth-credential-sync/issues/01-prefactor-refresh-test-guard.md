# 01: prefactor —— 给 `refresh` 的档位门与落盘顺序加测试守卫

Status: ready-for-agent
Blocked by: 无（可立即开工）

**What to build:** 本轮要改的那段代码（`refresh` 的请求表单 + 「先落盘、再推导 account id」的两次写盘）先有兜底测试：把请求表单抽成纯函数并按档位表驱动断言；把两次写盘的编排做成可注入的形态，从而能用一条测试钉住「新 token 一定先落盘」。做完之后，把这两条规则改回去会立刻变红 —— 这正是 `TODO-13` 记的缺口。

- [ ] 请求表单抽成纯函数：给定凭据 → 期望的键值对；表驱动覆盖 Codex 档位与动态注册档位两行，断言只有非 Codex 档位带 `resource`
- [ ] 落盘顺序可验证：抽一个可注入的编排（假 http 闭包注入，先例：`uninstall` 的 `run_with(args, exe, dir)` 注入点、`app/import` 的 runner 注入点），断言「POST 成功后先落盘、再推导 account id」这一顺序
- [ ] 两条测试都**不使用真实网络与真实凭据**（合成凭据即可）
- [ ] 不改变任何外部行为（本票只加测试与为可测性做的最小抽取）
- [ ] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
