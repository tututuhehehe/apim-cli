//! 额度查询的脚本绑定：每个厂商一个脚本，apim 注入环境变量、跑之、stdout 直显。

use serde::{Deserialize, Serialize};

/// 额度查询脚本：command 指外部可执行文件（尊重 shebang），run 为内联脚本
/// （经 shell -c 执行）。二选一，缺一反序列化即报错。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScriptSpec {
    /// 外部脚本路径（~ 会展开）。与 run 二选一。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// 内联脚本。与 command 二选一。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    /// 执行 run 用的 shell，缺省 /bin/sh。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    /// 超时秒数，缺省 15。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

impl<'de> Deserialize<'de> for ScriptSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            command: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            run: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            shell: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            timeout_secs: Option<u64>,
        }
        let raw = Raw::deserialize(deserializer)?;
        if raw.command.is_none() && raw.run.is_none() {
            return Err(serde::de::Error::custom(
                "额度查询需要脚本：balance.kind=script + command 或 run",
            ));
        }
        Ok(ScriptSpec {
            command: raw.command,
            run: raw.run,
            shell: raw.shell,
            timeout_secs: raw.timeout_secs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::Recipe;

    fn recipe_with_balance(yaml: &str) -> std::result::Result<Recipe, String> {
        let full = format!("id: x\nname: x\nbase_url: 'https://x'\nauth: {{kind: bearer}}\n{yaml}");
        serde_yaml::from_str(&full).map_err(|e| e.to_string())
    }

    #[test]
    fn script_balance_with_command_parses() {
        let recipe = recipe_with_balance(
            "balance:\n  kind: script\n  command: ~/.config/apim/scripts/x.sh\n  timeout_secs: 5",
        )
        .unwrap();
        let spec = recipe.balance.as_ref().unwrap();
        assert_eq!(spec.command.as_deref(), Some("~/.config/apim/scripts/x.sh"));
        assert_eq!(spec.timeout_secs, Some(5));
    }

    #[test]
    fn script_balance_with_inline_run_parses() {
        let recipe = recipe_with_balance(
            "balance:\n  kind: script\n  run: |\n    echo hi\n  shell: /bin/zsh",
        )
        .unwrap();
        let spec = recipe.balance.as_ref().unwrap();
        assert_eq!(spec.run.as_deref(), Some("echo hi\n"));
        assert_eq!(spec.shell.as_deref(), Some("/bin/zsh"));
    }

    #[test]
    fn script_without_command_or_run_is_rejected() {
        let err = recipe_with_balance("balance:\n  kind: script\n  timeout_secs: 5").unwrap_err();
        assert!(err.contains("需要脚本"), "{err}");
    }

    #[test]
    fn legacy_declarative_balance_is_rejected() {
        // 声明式 http 形态已退役：老 YAML 会得到「缺脚本」的明确报错
        let err = recipe_with_balance(
            "balance:\n  request: {url: '{base_url}/b'}\n  render: {headline: h}",
        )
        .unwrap_err();
        assert!(err.contains("需要脚本"), "{err}");
    }

    #[test]
    fn serialize_roundtrips() {
        // kind: script 在 YAML 里是可选文档标记（反序列化时忽略），存盘不带也认
        let recipe = recipe_with_balance("balance:\n  command: ~/x.sh").unwrap();
        let yaml = serde_yaml::to_string(&recipe).unwrap();
        assert!(yaml.contains("~/x.sh"), "{yaml}");
        let reloaded: Recipe = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(
            reloaded.balance.as_ref().unwrap().command.as_deref(),
            Some("~/x.sh")
        );
    }
}
