# 02: README 命令表与 `apim help` 对齐

Status: ready-for-agent
Blocked by: 01

**What to build:** 用户照 README 敲命令不会撞上「`apim help` 里有、README 里没有」。今天 `apim help` 列出的别名 `apim tui` 在 README 里一次都没出现；补上它（顺带把 `--version` 写全），并由守卫钉住：从 `apim help` 的**真实输出**提取的动词集合，必须是 README 命令表动词集合的子集。

- [ ] README 补 `apim tui` 一行，并补齐 `apim --version`
- [ ] `README.zh-CN.md` 同步（AGENTS.md 约定 7：改一版必须同步另一版）
- [ ] 守卫从真实 `apim help` 输出取值（跑二进制的**外部行为**），不读源码文本、不依赖内部函数
- [ ] 这条规则今天必须**先从红变绿**（先确认它抓到了 `apim tui` 缺口）
- [ ] 断言只比动词集合（`apim` 后的第一个词，别名并入同一项），不比对措辞、行号或段落顺序
- [ ] 失败信息点名缺哪个动词
- [ ] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
