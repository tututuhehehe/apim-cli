use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::clipboard;
use crate::config::{self, KeyEntry};
use crate::form::{self, Form};
use crate::probe::{self, BalanceSnapshot, Health, ProbeResult};
use crate::recipe::{
    self, Auth, AuthKind, BalanceSpec, HttpCall, ParseSpec, Recipe, RenderField, RenderSpec,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Providers,
    Keys,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    Key,
    Provider,
}

#[derive(Debug, Clone)]
pub struct KeyState {
    pub health: Health,
    pub balance: Option<BalanceSnapshot>,
}

impl Default for KeyState {
    fn default() -> Self {
        Self {
            health: Health::Unknown,
            balance: None,
        }
    }
}

pub enum Modal {
    None,
    Form {
        kind: FormKind,
        form: Form,
        original: Option<String>,
    },
    ConfirmDeleteKey {
        key_id: String,
    },
    ConfirmDeleteProvider {
        provider_id: String,
        path: PathBuf,
    },
}

pub struct App {
    pub recipes: HashMap<String, Recipe>,
    pub keys: Vec<KeyEntry>,
    pub provider_ids: Vec<String>,
    pub selected_provider: usize,
    pub selected_key: usize,
    pub focus: Focus,
    pub states: HashMap<String, KeyState>,
    pub toast: Option<(String, Instant)>,
    pub last_refresh: Option<Instant>,
    pub modal: Modal,
    inflight: HashSet<String>,
    tx: UnboundedSender<ProbeResult>,
    client: reqwest::Client,
}

pub fn start() -> Result<(App, UnboundedReceiver<ProbeResult>)> {
    let recipes = recipe::load_recipes()?;
    let keys = config::load_keys(&recipes)?;
    let (tx, rx) = mpsc::unbounded_channel();
    let mut app = App {
        recipes,
        keys,
        provider_ids: Vec::new(),
        selected_provider: 0,
        selected_key: 0,
        focus: Focus::Keys,
        states: HashMap::new(),
        toast: None,
        last_refresh: None,
        modal: Modal::None,
        inflight: HashSet::new(),
        tx,
        client: probe::client()?,
    };
    app.rebuild_provider_list();
    if !app.keys_in_provider().is_empty() {
        app.refresh_current_provider();
    }
    Ok((app, rx))
}

impl App {
    pub fn apply(&mut self, result: ProbeResult) {
        self.inflight.remove(&result.key_id);
        self.last_refresh = Some(Instant::now());
        let state = self.states.entry(result.key_id.clone()).or_default();
        state.health = result.health;
        state.balance = result.balance;
    }

    pub fn tick(&mut self) {
        if let Some((_, at)) = &self.toast
            && at.elapsed() > Duration::from_secs(3)
        {
            self.toast = None;
        }
    }

    pub fn is_checking(&self, key_id: &str) -> bool {
        self.inflight.contains(key_id)
    }

    pub fn current_provider_id(&self) -> Option<&str> {
        self.provider_ids
            .get(self.selected_provider)
            .map(String::as_str)
    }

    pub fn current_recipe(&self) -> Option<&Recipe> {
        self.current_provider_id()
            .and_then(|id| self.recipes.get(id))
    }

    pub fn keys_in_provider(&self) -> Vec<usize> {
        let Some(provider) = self.current_provider_id() else {
            return Vec::new();
        };
        self.keys
            .iter()
            .enumerate()
            .filter(|(_, k)| k.provider == provider)
            .map(|(i, _)| i)
            .collect()
    }

    pub fn selected_key_entry(&self) -> Option<&KeyEntry> {
        let keys = self.keys_in_provider();
        keys.get(self.selected_key).copied().map(|i| &self.keys[i])
    }

    pub fn state_for(&self, key: &KeyEntry) -> KeyState {
        self.states.get(&key.id()).cloned().unwrap_or_default()
    }

    fn rebuild_provider_list(&mut self) {
        let mut ids: Vec<String> = Vec::new();
        for key in &self.keys {
            if !ids.iter().any(|p| p == &key.provider) {
                ids.push(key.provider.clone());
            }
        }
        for id in self.recipes.keys() {
            if !ids.iter().any(|p| p == id) {
                ids.push(id.clone());
            }
        }
        if ids.len() > 1 {
            ids[1..].sort();
        }
        self.provider_ids = ids;
        if self.selected_provider >= self.provider_ids.len() {
            self.selected_provider = self.provider_ids.len().saturating_sub(1);
        }
        let n = self.keys_in_provider().len();
        if n == 0 {
            self.selected_key = 0;
        } else if self.selected_key >= n {
            self.selected_key = n - 1;
        }
    }

    pub fn move_up(&mut self) {
        match self.focus {
            Focus::Providers => {
                if self.selected_provider > 0 {
                    self.selected_provider -= 1;
                    self.selected_key = 0;
                    self.refresh_current_provider();
                }
            }
            Focus::Keys => {
                if self.selected_key > 0 {
                    self.selected_key -= 1;
                }
            }
        }
    }

    pub fn move_down(&mut self) {
        match self.focus {
            Focus::Providers => {
                if self.selected_provider + 1 < self.provider_ids.len() {
                    self.selected_provider += 1;
                    self.selected_key = 0;
                    self.refresh_current_provider();
                }
            }
            Focus::Keys => {
                let n = self.keys_in_provider().len();
                if n > 0 && self.selected_key + 1 < n {
                    self.selected_key += 1;
                }
            }
        }
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Providers => Focus::Keys,
            Focus::Keys => Focus::Providers,
        };
    }

    pub fn copy_selected(&mut self) {
        match self.focus {
            Focus::Providers => {
                let Some(recipe) = self.current_recipe() else {
                    self.toast = Some(("没有可复制的厂商".into(), Instant::now()));
                    return;
                };
                let name = recipe.name.clone();
                let url = recipe.base_url.clone();
                match clipboard::copy(&url) {
                    Ok(()) => {
                        self.toast = Some((format!("已复制 {name} 的 Base URL"), Instant::now()))
                    }
                    Err(err) => self.toast = Some((format!("复制失败: {err}"), Instant::now())),
                }
            }
            Focus::Keys => {
                let Some(key) = self.selected_key_entry() else {
                    self.toast = Some(("没有可复制的密钥".into(), Instant::now()));
                    return;
                };
                let alias = key.alias.clone();
                let token = key.token.clone();
                match clipboard::copy(&token) {
                    Ok(()) => self.toast = Some((format!("已复制 {alias}"), Instant::now())),
                    Err(err) => self.toast = Some((format!("复制失败: {err}"), Instant::now())),
                }
            }
        }
    }

    // ---- 打开弹窗 --------------------------------------------------------

    pub fn open_add(&mut self) {
        match self.focus {
            Focus::Providers => {
                self.modal = Modal::Form {
                    kind: FormKind::Provider,
                    form: form::provider_add(),
                    original: None,
                };
            }
            Focus::Keys => {
                let providers = self.provider_ids.clone();
                if providers.is_empty() {
                    self.toast = Some(("先添加一个厂商".into(), Instant::now()));
                    return;
                }
                self.modal = Modal::Form {
                    kind: FormKind::Key,
                    form: form::key_add(providers, self.selected_provider),
                    original: None,
                };
            }
        }
    }

    pub fn open_edit(&mut self) {
        match self.focus {
            Focus::Providers => {
                let Some(id) = self.current_provider_id().map(String::from) else {
                    return;
                };
                let Some(recipe) = self.recipes.get(&id) else {
                    return;
                };
                let strip = |url: &str| url.strip_prefix("{base_url}").unwrap_or(url).to_string();
                let health_path = recipe
                    .health
                    .as_ref()
                    .map(|h| strip(&h.url))
                    .unwrap_or_default();
                let balance = recipe.balance.as_ref();
                let balance_path = balance.map(|b| strip(&b.request.url)).unwrap_or_default();
                let balance_json = balance
                    .and_then(|b| b.parse.fields.get("total_balance").cloned())
                    .unwrap_or_default();
                self.modal = Modal::Form {
                    kind: FormKind::Provider,
                    form: form::provider_edit(
                        &recipe.id,
                        &recipe.name,
                        &recipe.base_url,
                        &health_path,
                        &balance_path,
                        &balance_json,
                    ),
                    original: Some(id),
                };
            }
            Focus::Keys => {
                let Some(key) = self.selected_key_entry().cloned() else {
                    self.toast = Some(("没有可编辑的密钥".into(), Instant::now()));
                    return;
                };
                let providers = self.provider_ids.clone();
                let selected = providers
                    .iter()
                    .position(|p| p == &key.provider)
                    .unwrap_or(0);
                self.modal = Modal::Form {
                    kind: FormKind::Key,
                    form: form::key_edit(
                        providers,
                        selected,
                        &key.alias,
                        key.group.as_deref().unwrap_or(""),
                        &key.token,
                    ),
                    original: Some(key.id()),
                };
            }
        }
    }

    pub fn open_delete(&mut self) {
        match self.focus {
            Focus::Providers => {
                let Some(id) = self.current_provider_id().map(String::from) else {
                    return;
                };
                let n = self.keys.iter().filter(|k| k.provider == id).count();
                if n > 0 {
                    self.toast = Some((
                        format!("该厂商下还有 {n} 条密钥，先删除密钥"),
                        Instant::now(),
                    ));
                    return;
                }
                let Some(recipe) = self.recipes.get(&id) else {
                    return;
                };
                match &recipe.origin {
                    Some(path) => {
                        self.modal = Modal::ConfirmDeleteProvider {
                            provider_id: id,
                            path: path.clone(),
                        };
                    }
                    None => {
                        self.toast =
                            Some(("内置厂商不可删除，可用 e 编辑覆盖".into(), Instant::now()));
                    }
                }
            }
            Focus::Keys => {
                let Some(key) = self.selected_key_entry() else {
                    self.toast = Some(("没有可删除的密钥".into(), Instant::now()));
                    return;
                };
                self.modal = Modal::ConfirmDeleteKey { key_id: key.id() };
            }
        }
    }

    pub fn cancel_modal(&mut self) {
        self.modal = Modal::None;
    }

    pub fn form(&mut self) -> Option<&mut Form> {
        match &mut self.modal {
            Modal::Form { form, .. } => Some(form),
            _ => None,
        }
    }

    // ---- 保存 ------------------------------------------------------------

    pub fn save_form(&mut self) {
        let Modal::Form {
            kind,
            form,
            original,
        } = &self.modal
        else {
            return;
        };
        let kind = *kind;
        let form = form.clone();
        let original = original.clone();
        match kind {
            FormKind::Key => self.save_key_form(&form, original.as_deref()),
            FormKind::Provider => self.save_provider_form(&form, original.as_deref()),
        }
    }

    fn save_key_form(&mut self, form: &Form, original: Option<&str>) {
        let provider = form.select_value(0).to_string();
        let alias = form.text(1).trim().to_string();
        let group = form.text(2).trim().to_string();
        let token = form.text(3).trim().to_string();

        if alias.is_empty() {
            self.set_form_error("别名必填");
            return;
        }
        if token.is_empty() {
            self.set_form_error("密钥必填");
            return;
        }
        let new_id = format!("{provider}.{alias}");
        if original != Some(new_id.as_str()) && self.keys.iter().any(|k| k.id() == new_id) {
            self.set_form_error(&format!("{new_id} 已存在"));
            return;
        }

        let entry = KeyEntry {
            provider: provider.clone(),
            alias,
            group: if group.is_empty() { None } else { Some(group) },
            token,
        };
        match original {
            None => self.keys.push(entry),
            Some(orig) => {
                if let Some(slot) = self.keys.iter_mut().find(|k| k.id() == orig) {
                    *slot = entry;
                    self.states.remove(orig);
                } else {
                    self.modal = Modal::None;
                    return;
                }
            }
        }

        if let Err(err) = config::save_keys(&self.keys) {
            self.toast = Some((format!("保存失败: {err}"), Instant::now()));
            self.modal = Modal::None;
            return;
        }
        self.modal = Modal::None;
        self.toast = Some((format!("已保存 {new_id}"), Instant::now()));
        self.rebuild_provider_list();
        self.selected_provider = self
            .provider_ids
            .iter()
            .position(|p| p == &provider)
            .unwrap_or(self.selected_provider);
        self.selected_key = self
            .keys_in_provider()
            .iter()
            .position(|i| self.keys[*i].id() == new_id)
            .unwrap_or(0);
        self.focus = Focus::Keys;
        self.refresh_current_provider();
    }

    fn save_provider_form(&mut self, form: &Form, original: Option<&str>) {
        let id = match original {
            Some(id) => id.to_string(),
            None => form.text(0).trim().to_string(),
        };
        let name = form.text(1).trim().to_string();
        let mut base_url = form.text(2).trim().to_string();
        let health_path = normalize_path(form.text(3).trim());
        let balance_path = normalize_path(form.text(4).trim());
        let balance_json = form.text(5).trim().to_string();

        if original.is_none() {
            if id.is_empty() {
                self.set_form_error("ID 必填");
                return;
            }
            if !id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            {
                self.set_form_error("ID 只能用小写字母、数字、-");
                return;
            }
            if self.recipes.contains_key(&id) {
                self.set_form_error(&format!("厂商 {id} 已存在"));
                return;
            }
        }
        if name.is_empty() {
            self.set_form_error("名称必填");
            return;
        }
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            self.set_form_error("Base URL 要以 http(s):// 开头");
            return;
        }
        base_url = base_url.trim_end_matches('/').to_string();
        if !balance_path.is_empty() && balance_json.is_empty() {
            self.set_form_error("配了额度路径就要填取值路径");
            return;
        }

        let mut recipe = match original {
            Some(id) => self
                .recipes
                .get(id)
                .cloned()
                .unwrap_or_else(|| default_recipe(id, &name, &base_url)),
            None => default_recipe(&id, &name, &base_url),
        };
        recipe.name = name;
        recipe.base_url = base_url;
        recipe.normalize();

        // 探活：路径没变就保留原有 headers/body
        let existing_health = recipe.health.clone();
        recipe.health = if health_path.is_empty() {
            None
        } else {
            let unchanged = existing_health
                .as_ref()
                .is_some_and(|h| url_suffix(&h.url) == health_path);
            if unchanged {
                existing_health
            } else {
                Some(HttpCall::get(format!("{{base_url}}{health_path}")))
            }
        };

        // 额度：路径和取值都没动就保留原有 parse/render（DeepSeek 这类）
        let existing_balance = recipe.balance.clone();
        recipe.balance = if balance_path.is_empty() {
            None
        } else {
            let unchanged = existing_balance.as_ref().is_some_and(|b| {
                url_suffix(&b.request.url) == balance_path && balance_json.is_empty()
            });
            if unchanged {
                existing_balance
            } else {
                Some(generic_balance(&balance_path, &balance_json))
            }
        };

        let path = match recipe::save_user_recipe(&recipe) {
            Ok(path) => path,
            Err(err) => {
                self.toast = Some((format!("保存失败: {err}"), Instant::now()));
                self.modal = Modal::None;
                return;
            }
        };
        recipe.origin = Some(path);
        self.recipes.insert(id.clone(), recipe);
        self.modal = Modal::None;
        self.toast = Some((format!("已保存厂商 {id}"), Instant::now()));
        self.rebuild_provider_list();
        self.selected_provider = self
            .provider_ids
            .iter()
            .position(|p| p == &id)
            .unwrap_or(self.selected_provider);
        self.selected_key = 0;
        self.focus = Focus::Keys;
    }

    fn set_form_error(&mut self, msg: &str) {
        if let Some(form) = self.form() {
            form.error = Some(msg.into());
        }
    }

    // ---- 删除确认 --------------------------------------------------------

    pub fn confirm_delete(&mut self) {
        match &self.modal {
            Modal::ConfirmDeleteKey { .. } => self.confirm_delete_key(),
            Modal::ConfirmDeleteProvider { .. } => self.confirm_delete_provider(),
            Modal::None | Modal::Form { .. } => {}
        }
    }

    fn confirm_delete_key(&mut self) {
        let Modal::ConfirmDeleteKey { key_id } = &self.modal else {
            return;
        };
        let key_id = key_id.clone();
        self.keys.retain(|k| k.id() != key_id);
        if let Err(err) = config::save_keys(&self.keys) {
            self.toast = Some((format!("保存失败: {err}"), Instant::now()));
        } else {
            self.toast = Some((format!("已删除 {key_id}"), Instant::now()));
        }
        self.states.remove(&key_id);
        self.modal = Modal::None;
        self.rebuild_provider_list();
        let n = self.keys_in_provider().len();
        if self.selected_key >= n && n > 0 {
            self.selected_key = n - 1;
        }
        if n > 0 {
            self.refresh_current_provider();
        }
    }

    fn confirm_delete_provider(&mut self) {
        let Modal::ConfirmDeleteProvider { provider_id, path } = &self.modal else {
            return;
        };
        let provider_id = provider_id.clone();
        let path = path.clone();
        match recipe::delete_user_recipe(&path) {
            Ok(()) => self.toast = Some((format!("已删除厂商 {provider_id}"), Instant::now())),
            Err(err) => self.toast = Some((format!("删除失败: {err}"), Instant::now())),
        }
        self.recipes.remove(&provider_id);
        self.modal = Modal::None;
        self.rebuild_provider_list();
    }

    // ---- probing ---------------------------------------------------------

    pub fn refresh_current_provider(&mut self) {
        let idxs = self.keys_in_provider();
        for idx in idxs {
            self.spawn_probe(idx);
        }
    }

    fn spawn_probe(&mut self, idx: usize) {
        let Some(key) = self.keys.get(idx).cloned() else {
            return;
        };
        let Some(recipe) = self.recipes.get(&key.provider).cloned() else {
            return;
        };
        let id = key.id();
        if !self.inflight.insert(id.clone()) {
            return;
        }
        self.states.entry(id).or_default().health = Health::Checking;
        let tx = self.tx.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            let result = probe::probe(&client, &recipe, &key).await;
            let _ = tx.send(result);
        });
    }

    pub async fn refresh_blocking(&mut self) {
        let idxs = self.keys_in_provider();
        let client = self.client.clone();
        let mut futs = Vec::new();
        for idx in idxs {
            let Some(key) = self.keys.get(idx).cloned() else {
                continue;
            };
            let Some(recipe) = self.recipes.get(&key.provider).cloned() else {
                continue;
            };
            let id = key.id();
            self.inflight.insert(id.clone());
            self.states.entry(id).or_default().health = Health::Checking;
            let client = client.clone();
            futs.push(async move { probe::probe(&client, &recipe, &key).await });
        }
        for result in futures::future::join_all(futs).await {
            self.apply(result);
        }
    }

    pub fn toast_text(&self) -> Option<&str> {
        self.toast.as_ref().map(|(s, _)| s.as_str())
    }
}

fn normalize_path(p: &str) -> String {
    if p.is_empty() || p.starts_with('/') {
        p.to_string()
    } else {
        format!("/{p}")
    }
}

fn url_suffix(url: &str) -> String {
    url.strip_prefix("{base_url}").unwrap_or(url).to_string()
}

fn default_recipe(id: &str, name: &str, base_url: &str) -> Recipe {
    Recipe {
        id: id.into(),
        name: name.into(),
        base_url: base_url.into(),
        supports_groups: false,
        auth: Auth {
            kind: AuthKind::Bearer,
            header: None,
            prefix: None,
            query_param: None,
        },
        health: None,
        balance: None,
        origin: None,
    }
}

fn generic_balance(path: &str, json_path: &str) -> BalanceSpec {
    let mut headers = HashMap::new();
    headers.insert("Accept".to_string(), "application/json".to_string());
    BalanceSpec {
        request: HttpCall {
            method: "GET".into(),
            url: format!("{{base_url}}{path}"),
            headers,
            body: None,
        },
        parse: ParseSpec {
            available: None,
            items: None,
            fields: HashMap::from([("total_balance".to_string(), json_path.to_string())]),
        },
        render: RenderSpec {
            headline: "{total_balance}".into(),
            fields: vec![RenderField {
                label: "额度".into(),
                value: "{total_balance}".into(),
            }],
        },
    }
}
