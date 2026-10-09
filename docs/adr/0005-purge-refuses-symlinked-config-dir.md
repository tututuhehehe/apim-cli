# ADR-0005：`--purge` 拒绝软链配置目录，且不提供 `--force` 绕过

- 状态：Accepted（2026-10-01）
- 来源：原 `DEV-NOTES.local.md` §2.7

## Context

dotfiles 用户常把 `~/.config/apim` 托管成软链。`apim uninstall --purge` 要删配置目录，而 `remove_dir_all` 对软链的行为（删链 vs 跟进去删）**不值得赌**：前者会「报告已删密钥、真身还在」，后者会删掉用户的 dotfiles 仓库。

## Decision

配置目录本身是软链时，`--purge` **显式拒绝**，并且**不提供 `--force` 绕过**。

## Consequences

- 要删的人自己 `rm -rf` 真身（错误信息里给出路径与软链目标）。
- 加 `--force` 等于把「两种坏结果」重新放回用户手里。
- 带 `#[cfg(unix)]` 测试覆盖。
