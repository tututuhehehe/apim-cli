//! 卸载结果的两套输出：给人看的分步说明（计划 + 结果），与给脚本的 `--json`。
//!
//! 两套输出共用一份数据；`--json` 始终是同一个形状（不在发布渠道时 `channel`/`command`
//! 为 `null`），脚本可以只用一套 schema 解析。

use std::path::Path;

use super::super::update::Channel;

/// 一次卸载的结果。`channel: None` = 不在发布渠道里（开发构建 / cargo 副本），
/// 那种情况什么都不删、只如实报告。
pub(super) struct Report<'a> {
    pub(super) channel: Option<Channel>,
    pub(super) exe: &'a Path,
    pub(super) command: Option<String>,
    pub(super) config_dir: &'a Path,
    pub(super) purge: bool,
    pub(super) dry_run: bool,
    /// 打算动的每一个路径（软链安装时不止一个：调用路径 + 一路跟下来的真身）。
    pub(super) targets: Vec<String>,
    pub(super) removed: Vec<String>,
    /// 动完之后仍在磁盘上的（渠道工具没删掉、或名字不是 apim 故意没删）。
    pub(super) left: Vec<String>,
    pub(super) purged: Option<String>,
    /// 各客户端（`~/.codex`、`~/.pi/agent`）里 apim 留下的东西（没启用一键导入就是空的）。
    pub(super) leftovers: Vec<String>,
}

impl Report<'_> {
    /// 动手之前先摆清楚：卸哪条渠道、删什么、留什么。
    pub(super) fn print_plan(&self) {
        let channel = self.channel.map(Channel::label).unwrap_or("不在发布渠道里");
        println!(
            "将卸载 apim {}（渠道：{channel}）",
            env!("CARGO_PKG_VERSION")
        );
        println!("  程序：{}", self.exe.display());
        if let Some(command) = &self.command {
            println!("  命令：{command}");
        }
        // 软链安装：真身不在调用路径上，先说清楚还会删哪一个
        for target in self.targets.iter().skip(1) {
            println!("  真身：{target}");
        }
        if self.purge {
            println!("  数据：删除 {}（含密钥）", self.config_dir.display());
        } else {
            println!(
                "  数据：保留 {}（要一起删加 --purge）",
                self.config_dir.display()
            );
        }
    }

    pub(super) fn print_result(&self) {
        let channel = self.channel.map(Channel::label).unwrap_or("渠道工具");
        if self.removed.is_empty() {
            println!("\n程序已由渠道工具移除（渠道：{channel}）。");
        } else {
            println!("\n已删除：{}", self.removed.join("、"));
        }
        if !self.left.is_empty() {
            println!("未删除（仍在磁盘上；不叫 apim 的文件 apim 不动，需要的话自己删）：");
            for path in &self.left {
                println!("  {path}");
            }
        }
        match (&self.purged, self.purge) {
            (Some(dir), _) => println!("已删除配置目录：{dir}（含密钥）"),
            (None, true) => println!("配置目录本来就不存在：{}", self.config_dir.display()),
            (None, false) => println!(
                "配置保留：{}（含密钥；要一起删：apim uninstall --purge）",
                self.config_dir.display()
            ),
        }
        if !self.leftovers.is_empty() {
            println!("\napim 写进客户端配置的内容没动（跟手写配置混在同一个文件里）：");
            for item in &self.leftovers {
                println!("  {item}");
            }
            println!("  怎么清见 README 的 Uninstall 一节。");
        }
        println!("\n重开一个 shell 后确认：apim --version");
    }

    pub(super) fn print_json(&self) {
        println!(
            "{}",
            serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "channel": self.channel.map(Channel::label),
                "exe": self.exe.display().to_string(),
                "command": self.command,
                "config_dir": self.config_dir.display().to_string(),
                "purge": self.purge,
                "dry_run": self.dry_run,
                "targets": self.targets,
                "removed": self.removed,
                "left": self.left,
                "purged": self.purged,
                "leftovers": self.leftovers,
            })
        );
    }
}
