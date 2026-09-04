//! 额度查询的脚本逃生舱：接口长得怪的厂商用「跑一个脚本、显示 stdout」兜底，
//! 不再为它们扩声明式 DSL。

use serde::{Deserialize, Serialize};

use super::BalanceSpec;

/// `balance` 的两种形态。不写 `kind` = http，老 recipe 一行不用改。
#[derive(Debug, Clone)]
pub enum BalanceMode {
    Http(Box<BalanceSpec>),
    Script(ScriptSpec),
}

impl BalanceMode {
    pub fn http(&self) -> Option<&BalanceSpec> {
        match self {
            BalanceMode::Http(spec) => Some(spec),
            BalanceMode::Script(_) => None,
        }
    }

    pub fn script(&self) -> Option<&ScriptSpec> {
        match self {
            BalanceMode::Http(_) => None,
            BalanceMode::Script(spec) => Some(spec),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ScriptSpec {
    /// 外部脚本：可执行文件路径（直接 exec，尊重 shebang）。与 run 二选一。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// 内联脚本：经 shell -c 执行，recipe 单文件自包含。与 command 二选一。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    /// 执行 run 用的 shell，缺省 /bin/sh。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    /// 超时秒数，缺省 15。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

impl<'de> Deserialize<'de> for BalanceMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = serde_yaml::Value::deserialize(deserializer)?;
        let kind = raw.get("kind").and_then(|v| v.as_str()).unwrap_or("http");
        match kind {
            "http" => {
                let spec: BalanceSpec =
                    serde_yaml::from_value(raw).map_err(serde::de::Error::custom)?;
                Ok(BalanceMode::Http(Box::new(spec)))
            }
            "script" => {
                let spec: ScriptSpec =
                    serde_yaml::from_value(raw).map_err(serde::de::Error::custom)?;
                match (spec.command.as_deref(), spec.run.as_deref()) {
                    (Some(_), None) | (None, Some(_)) => Ok(BalanceMode::Script(spec)),
                    _ => Err(serde::de::Error::custom(
                        "balance.kind=script 需要 command 或 run 二选一",
                    )),
                }
            }
            other => Err(serde::de::Error::custom(format!(
                "未知 balance.kind: {other}（可用 http | script）"
            ))),
        }
    }
}

impl Serialize for BalanceMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            // http 是缺省形态，序列化不写 kind，老 recipe 存盘保持原样。
            BalanceMode::Http(spec) => spec.serialize(serializer),
            BalanceMode::Script(spec) => {
                let mut value = serde_yaml::to_value(spec).map_err(serde::ser::Error::custom)?;
                if let serde_yaml::Value::Mapping(map) = &mut value {
                    let mut ordered = serde_yaml::Mapping::new();
                    ordered.insert(
                        serde_yaml::Value::String("kind".into()),
                        serde_yaml::Value::String("script".into()),
                    );
                    for (k, v) in map.iter() {
                        ordered.insert(k.clone(), v.clone());
                    }
                    *map = ordered;
                }
                value.serialize(serializer)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Recipe;

    fn recipe_with_balance(yaml: &str) -> Recipe {
        let full = format!("id: x\nname: x\nbase_url: 'https://x'\nauth: {{kind: bearer}}\n{yaml}");
        serde_yaml::from_str(&full).unwrap()
    }

    #[test]
    fn balance_without_kind_is_http() {
        let recipe = recipe_with_balance(
            "balance:\n  request: {url: '{base_url}/b'}\n  render: {headline: h}",
        );
        assert!(recipe.balance.as_ref().unwrap().http().is_some());
    }

    #[test]
    fn script_balance_with_command_parses() {
        let recipe = recipe_with_balance(
            "balance:\n  kind: script\n  command: ~/.config/apim/scripts/x.sh\n  timeout_secs: 5",
        );
        let spec = recipe.balance.as_ref().unwrap().script().unwrap();
        assert_eq!(spec.command.as_deref(), Some("~/.config/apim/scripts/x.sh"));
        assert_eq!(spec.timeout_secs, Some(5));
    }

    #[test]
    fn script_balance_with_inline_run_parses() {
        let recipe = recipe_with_balance(
            "balance:\n  kind: script\n  run: |\n    echo hi\n  shell: /bin/zsh",
        );
        let spec = recipe.balance.as_ref().unwrap().script().unwrap();
        assert_eq!(spec.run.as_deref(), Some("echo hi\n"));
        assert_eq!(spec.shell.as_deref(), Some("/bin/zsh"));
    }

    #[test]
    fn script_without_command_or_run_is_rejected() {
        let full = "id: x\nname: x\nbase_url: 'https://x'\nauth: {kind: bearer}\nbalance:\n  kind: script\n  timeout_secs: 5";
        assert!(serde_yaml::from_str::<Recipe>(full).is_err());
    }

    #[test]
    fn unknown_kind_is_rejected() {
        let full = "id: x\nname: x\nbase_url: 'https://x'\nauth: {kind: bearer}\nbalance:\n  kind: grpc\n  run: x";
        let err = serde_yaml::from_str::<Recipe>(full)
            .unwrap_err()
            .to_string();
        assert!(err.contains("未知 balance.kind"), "{err}");
    }

    #[test]
    fn serialize_keeps_http_clean_and_tags_script() {
        let http = recipe_with_balance(
            "balance:\n  request: {url: '{base_url}/b'}\n  render: {headline: h}",
        );
        let yaml = serde_yaml::to_string(&http).unwrap();
        // auth 段自带 kind 字段，这里只看额度段不引入 kind: script
        assert!(!yaml.contains("kind: script"), "{yaml}");
        let reloaded: Recipe = serde_yaml::from_str(&yaml).unwrap();
        assert!(reloaded.balance.as_ref().unwrap().http().is_some());

        let script = recipe_with_balance("balance:\n  kind: script\n  command: ~/x.sh");
        let yaml = serde_yaml::to_string(&script).unwrap();
        assert!(yaml.contains("kind: script"), "{yaml}");
        assert!(yaml.contains("~/x.sh"));

        // 存盘再读回，还是 script
        let reloaded: Recipe = serde_yaml::from_str(&yaml).unwrap();
        assert!(reloaded.balance.as_ref().unwrap().script().is_some());
    }
}
