use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::clipboard;
use crate::config::{self, KeyEntry};
use crate::form::{FormMode, KeyForm};
use crate::probe::{self, BalanceSnapshot, Health, ProbeResult};
use crate::recipe::{self, Recipe};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Providers,
    Keys,
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
    Form(KeyForm),
    ConfirmDelete { key_id: String },
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
    if !app.provider_ids.is_empty() {
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

    // ---- CRUD ----------------------------------------------------------

    pub fn open_add(&mut self) {
        let provider_ids: Vec<String> = self.provider_ids.clone();
        if provider_ids.is_empty() {
            self.toast = Some(("没有可用的厂商 recipe".into(), Instant::now()));
            return;
        }
        self.modal = Modal::Form(KeyForm::add(provider_ids, self.selected_provider));
    }

    pub fn open_edit(&mut self) {
        let Some(key) = self.selected_key_entry().cloned() else {
            self.toast = Some(("没有可编辑的密钥".into(), Instant::now()));
            return;
        };
        let provider_ids: Vec<String> = self.provider_ids.clone();
        self.modal = Modal::Form(KeyForm::edit(provider_ids, &key));
    }

    pub fn open_delete(&mut self) {
        let Some(key) = self.selected_key_entry() else {
            self.toast = Some(("没有可删除的密钥".into(), Instant::now()));
            return;
        };
        self.modal = Modal::ConfirmDelete { key_id: key.id() };
    }

    pub fn form(&mut self) -> Option<&mut KeyForm> {
        match &mut self.modal {
            Modal::Form(form) => Some(form),
            _ => None,
        }
    }

    pub fn save_form(&mut self, form: &KeyForm) {
        let result = self.validate_form(form);
        let entry = match result {
            Ok(entry) => entry,
            Err(err) => {
                if let Some(f) = self.form() {
                    f.error = Some(err);
                }
                return;
            }
        };

        let new_id = entry.id();
        match &form.mode {
            FormMode::Add => self.keys.push(entry),
            FormMode::Edit { original_id } => {
                let Some(slot) = self.keys.iter_mut().find(|k| &k.id() == original_id) else {
                    self.modal = Modal::None;
                    return;
                };
                *slot = entry;
                self.states.remove(original_id);
            }
        }

        if let Err(err) = config::save_keys(&self.keys) {
            self.toast = Some((format!("保存失败: {err}"), Instant::now()));
            self.modal = Modal::None;
            return;
        }
        self.modal = Modal::None;
        self.toast = Some((format!("已保存 {new_id}"), Instant::now()));
        let provider = new_id
            .split_once('.')
            .map(|(p, _)| p.to_string())
            .unwrap_or_default();
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

    fn validate_form(&self, form: &KeyForm) -> std::result::Result<KeyEntry, String> {
        let Some(provider) = form.provider_id() else {
            return Err("没有可用的厂商".into());
        };
        let provider = provider.to_string();
        let alias = form.alias.value.trim().to_string();
        if alias.is_empty() {
            return Err("别名必填".into());
        }
        let token = form.token.value.trim().to_string();
        if token.is_empty() {
            return Err("密钥必填".into());
        }
        let editing = match &form.mode {
            FormMode::Edit { original_id } => original_id.clone(),
            FormMode::Add => String::new(),
        };
        let new_id = format!("{provider}.{alias}");
        if new_id != editing && self.keys.iter().any(|k| k.id() == new_id) {
            return Err(format!("{new_id} 已存在"));
        }
        let group = form.group.value.trim().to_string();
        Ok(KeyEntry {
            provider,
            alias,
            group: if group.is_empty() { None } else { Some(group) },
            token,
        })
    }

    pub fn cancel_modal(&mut self) {
        self.modal = Modal::None;
    }

    pub fn confirm_delete(&mut self) {
        let Modal::ConfirmDelete { key_id } = &self.modal else {
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

    // ---- probing -------------------------------------------------------

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
