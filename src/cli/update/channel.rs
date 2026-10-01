//! 认「当前这个 apim 是从哪条渠道装的」—— 只认：npm、Homebrew、install.sh 裸二进制。

use std::path::Path;

use super::install_sh_url;

/// 当前 apim 的安装渠道。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Channel {
    Npm,
    Homebrew,
    /// install.sh 装的裸二进制（`/usr/local/bin/apim`、`~/.local/bin/apim` 等）。
    Binary,
}

impl Channel {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Channel::Npm => "npm",
            Channel::Homebrew => "Homebrew",
            Channel::Binary => "install.sh（裸二进制）",
        }
    }

    /// 这条渠道的更新命令（给用户看的原样命令）。
    /// install.sh 这条是给用户**手动兜底**用的，所以指 `main` 分支 —— 照着手敲的人不该依赖
    /// apim 这次查到的 tag；apim 真正执行的那条在 `install_sh.rs` 里，一定钉 tag。
    pub(crate) fn command_hint(self) -> String {
        match self {
            Channel::Npm => "npm install -g apim-cli@latest".to_string(),
            Channel::Homebrew => "brew upgrade apim".to_string(),
            Channel::Binary => format!("curl -fsSL {} | sh", install_sh_url("main")),
        }
    }
}

/// 认渠道。`None` = 不是这三条发布渠道装的，一律不动：
/// - `target/` 下是 `cargo run` / `cargo build` 的产物，覆盖掉就把开发二进制换成 Release 版；
/// - `~/.cargo/bin` 是 `cargo install` 留下的副本（项目明确不推荐这条路，本机 PATH 里还有一份
///   被它遮挡的 npm 版），install.sh 往那儿装只会多一份版本不一致的二进制。
///
/// 同时看原路径与 `canonicalize()` 之后的路径：Homebrew 会经过 `/opt/homebrew/bin/apim`
/// 这样的软链，只看原路径认不出来。
pub(crate) fn detect_channel(exe: &Path) -> Option<Channel> {
    let canonical = exe.canonicalize().unwrap_or_else(|_| exe.to_path_buf());
    let raw = exe.to_string_lossy().replace('\\', "/");
    let real = canonical.to_string_lossy().replace('\\', "/");
    let has = |needle: &str| raw.contains(needle) || real.contains(needle);

    if has("/target/debug/") || has("/target/release/") || has("/.cargo/bin/") {
        return None;
    }
    // npm 全局包：二进制在 node_modules/apim-cli-<平台>/bin/ 下
    if has("node_modules") && has("apim-cli") {
        return Some(Channel::Npm);
    }
    // Homebrew（含 Linuxbrew）：brew 把真身放在 Cellar/apim/<版本>/
    if has("Cellar/apim") {
        return Some(Channel::Homebrew);
    }
    Some(Channel::Binary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_npm_global_install() {
        assert_eq!(
            detect_channel(Path::new(
                "/opt/homebrew/lib/node_modules/apim-cli/node_modules/apim-cli-darwin-arm64/bin/apim"
            )),
            Some(Channel::Npm)
        );
        assert_eq!(
            detect_channel(Path::new(
                "C:\\Users\\me\\AppData\\Roaming\\npm\\node_modules\\apim-cli-windows-x64\\bin\\apim.exe"
            )),
            Some(Channel::Npm)
        );
    }

    #[test]
    fn detects_homebrew() {
        assert_eq!(
            detect_channel(Path::new("/opt/homebrew/Cellar/apim/0.1.9/bin/apim")),
            Some(Channel::Homebrew)
        );
        // Linuxbrew
        assert_eq!(
            detect_channel(Path::new("/home/me/.linuxbrew/Cellar/apim/0.1.9/bin/apim")),
            Some(Channel::Homebrew)
        );
    }

    #[test]
    fn detects_install_sh_binary() {
        // install.sh 的默认落点：/usr/local/bin（可写）或 ~/.local/bin
        assert_eq!(
            detect_channel(Path::new("/usr/local/bin/apim")),
            Some(Channel::Binary)
        );
        assert_eq!(
            detect_channel(Path::new("/Users/me/.local/bin/apim")),
            Some(Channel::Binary)
        );
    }

    /// 本地开发构建不属于任何发布渠道 —— 绝不能拿 Release 覆盖掉它。
    #[test]
    fn dev_builds_are_not_a_channel() {
        assert_eq!(
            detect_channel(Path::new("/Users/me/apim-cli/target/debug/apim")),
            None
        );
        assert_eq!(
            detect_channel(Path::new(
                "/Users/me/Documents/VIBE/apim管理/target/release/apim"
            )),
            None
        );
    }

    /// 路径里带 apim-cli 但**不含 node_modules** 时不能误判成 npm（两者要同时满足）。
    #[test]
    fn repo_path_alone_is_not_npm() {
        assert_eq!(
            detect_channel(Path::new("/Users/me/apim-cli/bin/apim")),
            Some(Channel::Binary)
        );
    }

    /// `~/.cargo/bin` 是 cargo install 留下的副本（本机 PATH 里还有一份被它遮挡的
    /// npm 版），跟 target/ 一样不属于发布渠道。
    #[test]
    fn cargo_install_is_not_a_release_channel() {
        assert_eq!(detect_channel(Path::new("/Users/me/.cargo/bin/apim")), None);
    }

    /// 提示里的手动命令与真正执行的 URL 都从 `install_sh_url` 拼出来，只差 tag / main：
    /// 手敲那条按 main，执行那条钉 tag。
    #[test]
    fn install_sh_urls_share_one_template() {
        assert_eq!(
            install_sh_url("v1.2.3"),
            "https://raw.githubusercontent.com/tututuhehehe/apim-cli/v1.2.3/install.sh"
        );
        assert_eq!(
            Channel::Binary.command_hint(),
            format!("curl -fsSL {} | sh", install_sh_url("main"))
        );
    }
}
