//! 卸载结果的两套输出：给人看的分步说明（计划 + 结果），与给脚本的 `--json`。

use std::path::Path;

use super::super::update::Channel;

/// 一次卸载的结果：human 输出与 `--json` 共用同一份数据，免得两处各写一遍字段。
pub(super) struct Report<'a> {
    pub(super) channel: Channel,
    pub(super) exe: &'a Path,
    pub(super) command: String,
    pub(super) config_dir: &'a Path,
    pub(super) purge: bool,
    pub(super) dry_run: bool,
    pub(super) removed: Vec<String>,
    pub(super) purged: Option<String>,
    /// `~/.codex` 里 apim 留下的东西（没启用一键导入就是空的）。
    pub(super) leftovers: Vec<String>,
}

impl Report<'_> {
    /// 动手之前先摆清楚：卸哪条渠道、删什么、留什么。
    pub(super) fn print_plan(&self) {
        println!(
            "将卸载 apim {}（渠道：{}）",
            env!("CARGO_PKG_VERSION"),
            self.channel.label()
        );
        println!("  程序：{}", self.exe.display());
        println!("  命令：{}", self.command);
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
        if self.removed.is_empty() {
            println!("\n程序已由渠道工具移除（渠道：{}）。", self.channel.label());
        } else {
            println!("\n已删除：{}", self.removed.join("、"));
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
            println!("\nCodex 那边 apim 写过的内容没动（跟手写配置混在同一个文件里）：");
            for item in &self.leftovers {
                println!("  {item}");
            }
            println!("  怎么还原见 README 的 Uninstall 一节。");
        }
        println!("\n重开一个 shell 后确认：apim --version");
    }

    pub(super) fn print_json(&self) {
        println!(
            "{}",
            serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "channel": self.channel.label(),
                "exe": self.exe.display().to_string(),
                "command": self.command,
                "config_dir": self.config_dir.display().to_string(),
                "purge": self.purge,
                "dry_run": self.dry_run,
                "removed": self.removed,
                "purged": self.purged,
                "leftovers": self.leftovers,
            })
        );
    }
}
