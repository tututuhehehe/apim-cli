//! 厂商 recipe：怎么鉴权、怎么探活、怎么查额度。

pub use script::ScriptSpec;
pub(crate) use store::save_user_recipe_to;
pub use store::{delete_user_recipe, save_user_recipe, user_recipes_dir};

mod script;
mod store;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const BUILTIN_DEEPSEEK: &str = include_str!("../../recipes/deepseek.yaml");
const BUILTIN_OPENAI: &str = include_str!("../../recipes/openai.yaml");
const BUILTIN_MOONSHOT: &str = include_str!("../../recipes/moonshot.yaml");
const BUILTIN_OPENROUTER: &str = include_str!("../../recipes/openrouter.yaml");

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Recipe {
    pub id: String,
    pub name: String,
    pub base_url: String,
    /// 控制面板主页，TUI 选中厂商按 Enter 用默认浏览器打开。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// 模型列表端点模板（按 m 浏览模型用）。缺省依次尝试探活路径（以 `/models`
    /// 结尾时）、{base_url}/models、{base_url}/v1/models；GLM 这类非标厂商才需要配。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models_url: Option<String>,
    #[serde(default)]
    pub supports_groups: bool,
    /// 自定义模板变量，可进 {placeholder} 替换并注入脚本 env。存敏感值时整个文件 600。
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub vars: HashMap<String, String>,
    pub auth: Auth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<HttpCall>,
    /// 额度查询：跑脚本（stdout 逐行直显），见 ScriptSpec。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance: Option<ScriptSpec>,
    /// 本地 YAML 路径；None = 内置（编译进二进制），不可删除。
    #[serde(skip)]
    pub origin: Option<PathBuf>,
}

impl Recipe {
    pub fn normalize(&mut self) {
        while self.base_url.ends_with('/') {
            self.base_url.pop();
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Auth {
    #[serde(default)]
    pub kind: AuthKind,
    pub header: Option<String>,
    pub prefix: Option<String>,
    pub query_param: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    #[default]
    Bearer,
    Header,
    Query,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpCall {
    #[serde(default = "default_get")]
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
}

impl HttpCall {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: default_get(),
            url: url.into(),
            headers: HashMap::new(),
            body: None,
        }
    }
}

fn default_get() -> String {
    "GET".into()
}

pub fn load_recipes() -> Result<HashMap<String, Recipe>> {
    load_recipes_with(&user_recipes_dir())
}

/// builtin → 开发目录 → 指定用户目录，逐级同 id 覆盖。CLI/测试用自定义根目录。
pub fn load_recipes_with(user_dir: &Path) -> Result<HashMap<String, Recipe>> {
    let mut map = HashMap::new();
    insert_yaml(&mut map, BUILTIN_DEEPSEEK, "builtin:deepseek", None)?;
    insert_yaml(&mut map, BUILTIN_OPENAI, "builtin:openai", None)?;
    insert_yaml(&mut map, BUILTIN_MOONSHOT, "builtin:moonshot", None)?;
    insert_yaml(&mut map, BUILTIN_OPENROUTER, "builtin:openrouter", None)?;

    // 开发目录等价于内置补充：origin 清回 None，保证「内置不可删除」的保护
    // 不被 manifest 覆盖带的路径击穿（只有用户目录的 recipe 才可删）。
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("recipes");
    load_dir(&manifest_dir, &mut map)?;
    for recipe in map.values_mut() {
        recipe.origin = None;
    }
    load_dir(user_dir, &mut map)?;
    Ok(map)
}

pub(crate) fn load_dir(dir: &Path, map: &mut HashMap<String, Recipe>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let mut files: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("read {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|s| s.to_str()),
                Some("yaml") | Some("yml")
            )
        })
        .collect();
    files.sort();
    for path in files {
        let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        insert_yaml(map, &raw, &path.display().to_string(), Some(path.clone()))?;
    }
    Ok(())
}

fn insert_yaml(
    map: &mut HashMap<String, Recipe>,
    raw: &str,
    origin: &str,
    path: Option<PathBuf>,
) -> Result<()> {
    let mut recipe: Recipe =
        serde_yaml::from_str(raw).with_context(|| format!("parse {origin}"))?;
    recipe.normalize();
    recipe.origin = path;
    map.insert(recipe.id.clone(), recipe);
    Ok(())
}

pub fn request_ctx(recipe: &Recipe, token: &str) -> HashMap<String, String> {
    let mut ctx = recipe.vars.clone();
    ctx.insert("token".into(), token.to_string());
    ctx.insert("base_url".into(), recipe.base_url.clone());
    ctx
}

/// 单趟扫描替换 `{name}`：名字在 ctx 里就输出对应值（值本身不再二次扫描），
/// 不认识的占位符原样保留。结果与 HashMap 迭代顺序无关——某个 var 的值
/// 恰好含 `{别的占位符}` 时也不会被嵌套展开。
pub fn subst(input: &str, ctx: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let tail = &rest[open..]; // 以 '{' 开头
        match tail.find('}') {
            Some(close) => {
                let name = &tail[1..close];
                match ctx.get(name) {
                    Some(value) => out.push_str(value),
                    None => out.push_str(&tail[..=close]), // 保留字面量 {name}
                }
                rest = &tail[close + 1..];
            }
            // 后面再无 '}'，剩余部分全是字面量
            None => {
                out.push_str(tail);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn subst_json(value: &Value, ctx: &HashMap<String, String>) -> Value {
    match value {
        Value::String(s) => Value::String(subst(s, ctx)),
        Value::Array(items) => Value::Array(items.iter().map(|v| subst_json(v, ctx)).collect()),
        Value::Object(map) => {
            let mut next = serde_json::Map::new();
            for (k, v) in map {
                next.insert(k.clone(), subst_json(v, ctx));
            }
            Value::Object(next)
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nested_ctx() -> HashMap<String, String> {
        HashMap::from([
            ("a".to_string(), "{b}".to_string()),
            ("b".to_string(), "x".to_string()),
        ])
    }

    #[test]
    fn subst_single_pass_regardless_of_iteration_order() {
        // {a} 的值 "{b}" 不被二次展开，结果与 HashMap 迭代顺序无关
        let ctx = nested_ctx();
        assert_eq!(subst("{a}", &ctx), "{b}");
        assert_eq!(subst("{a}-{b}", &ctx), "{b}-x");
        assert_eq!(subst_json(&Value::from("{a}"), &ctx), Value::from("{b}"));
    }

    #[test]
    fn subst_keeps_unknown_placeholders() {
        let ctx = nested_ctx();
        assert_eq!(subst("{nope}", &ctx), "{nope}");
        assert_eq!(subst("pre {nope} post", &ctx), "pre {nope} post");
        // 没有闭合的 '{' 也原样保留
        assert_eq!(subst("dangling {", &ctx), "dangling {");
        assert_eq!(subst("no brace", &ctx), "no brace");
    }

    #[test]
    fn subst_plain_placeholders_unchanged() {
        let ctx = HashMap::from([
            ("token".to_string(), "sk-test".to_string()),
            (
                "base_url".to_string(),
                "https://api.example.com".to_string(),
            ),
            ("access_token".to_string(), "at-1".to_string()),
        ]);
        assert_eq!(
            subst("{base_url}/models", &ctx),
            "https://api.example.com/models"
        );
        assert_eq!(subst("Bearer {token}", &ctx), "Bearer sk-test");
        assert_eq!(subst("{access_token}|{token}", &ctx), "at-1|sk-test");
    }

    #[test]
    fn builtin_recipes_load_with_expected_metadata() {
        // 不存在的用户目录：只验 builtin（+ 开发目录）注册结果。
        // 探活路径与额度脚本一并锁死，防止 YAML 改动后测试悄悄失同步
        // （OpenRouter 的 /models 是公开端点，探活必须用鉴权的 /key）。
        let user_dir = Path::new("target/apim-builtin-tests-no-user-dir");
        let map = load_recipes_with(user_dir).unwrap();
        let expected = [
            (
                "deepseek",
                "DeepSeek",
                "https://api.deepseek.com",
                "/models",
                "deepseek-quota.sh",
            ),
            (
                "openai",
                "OpenAI",
                "https://api.openai.com",
                "/v1/models",
                "openai-quota.sh",
            ),
            (
                "moonshot",
                "Moonshot AI",
                "https://api.moonshot.cn",
                "/v1/models",
                "moonshot-quota.sh",
            ),
            (
                "openrouter",
                "OpenRouter",
                "https://openrouter.ai/api/v1",
                "/key",
                "openrouter-quota.sh",
            ),
        ];
        for (id, name, base_url, health_suffix, script_name) in expected {
            let recipe = map
                .get(id)
                .unwrap_or_else(|| panic!("builtin {id} missing"));
            assert_eq!(recipe.name, name, "{id} name");
            assert_eq!(recipe.base_url, base_url, "{id} base_url");
            let health = recipe
                .health
                .as_ref()
                .unwrap_or_else(|| panic!("{id} 缺 health"));
            assert!(
                health.url.ends_with(health_suffix),
                "{id} health url 应以 {health_suffix} 结尾，实际 {}",
                health.url
            );
            let command = recipe
                .balance
                .as_ref()
                .and_then(|b| b.command.as_deref())
                .unwrap_or_else(|| panic!("{id} 额度应绑定脚本"));
            assert!(
                command.ends_with(script_name),
                "{id} 额度脚本应为 {script_name}，实际 {command}"
            );
            assert!(
                recipe.origin.is_none(),
                "{id} 应为内置（origin None，不可删）"
            );
        }
    }

    #[test]
    fn user_recipe_with_same_id_overrides_builtin() {
        // 核心约定：用户目录同 id YAML 整体覆盖内置（origin 变为用户文件路径）
        let user_dir = Path::new("target/apim-builtin-override-tests");
        fs::create_dir_all(user_dir).unwrap();
        let override_path = user_dir.join("openai.yaml");
        fs::write(
            &override_path,
            "id: openai\nname: 我的 OpenAI 中转\nbase_url: 'https://my-relay.example.com'\nauth: {kind: bearer}\n",
        )
        .unwrap();
        let map = load_recipes_with(user_dir).unwrap();
        let recipe = map.get("openai").expect("openai 仍存在");
        assert_eq!(recipe.name, "我的 OpenAI 中转");
        assert_eq!(recipe.base_url, "https://my-relay.example.com");
        assert_eq!(recipe.origin.as_deref(), Some(override_path.as_path()));
        fs::remove_file(&override_path).unwrap();
    }

    #[test]
    fn builtin_balance_is_script_bound() {
        // 四家内置厂商的额度一律走脚本（command 指向用户 scripts 目录）
        for (id, script) in [
            ("deepseek", "deepseek-quota.sh"),
            ("openai", "openai-quota.sh"),
            ("moonshot", "moonshot-quota.sh"),
            ("openrouter", "openrouter-quota.sh"),
        ] {
            let map = load_recipes_with(Path::new("target/apim-builtin-balance-tests")).unwrap();
            let recipe = map
                .get(id)
                .unwrap_or_else(|| panic!("builtin {id} missing"));
            let spec = recipe
                .balance
                .as_ref()
                .and_then(|b| b.command.as_deref())
                .unwrap_or_else(|| panic!("{id} 额度应绑定脚本"));
            assert!(spec.ends_with(script), "{id} 脚本名不对: {spec}");
        }
    }
}
