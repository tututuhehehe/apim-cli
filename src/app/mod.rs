//! 应用状态：选中项、导航、探活调度。弹窗与存取在子模块。

pub use modal::Modal;

mod keys_store;
mod modal;
mod providers_store;

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::clipboard;
use crate::config::{self, KeyEntry};
use crate::probe::{self, BalanceSnapshot, Health, ProbeResult};
use crate::recipe::Recipe;

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
    pub(crate) inflight: HashSet<String>,
    pub(crate) tx: UnboundedSender<ProbeResult>,
    pub(crate) client: reqwest::Client,
}

impl App {
    pub fn start() -> Result<(App, UnboundedReceiver<ProbeResult>)> {
        let recipes = crate::recipe::load_recipes()?;
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

    pub(crate) fn rebuild_provider_list(&mut self) {
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

    /// 选中厂商按 Enter：用默认浏览器打开它的控制面板主页。
    pub fn open_homepage(&mut self) {
        let Some(recipe) = self.current_recipe() else {
            return;
        };
        let name = recipe.name.clone();
        match recipe.homepage.as_deref().filter(|u| !u.is_empty()) {
            Some(url) => match crate::browser::open(url) {
                Ok(()) => self.toast = Some((format!("已打开 {name} 主页"), Instant::now())),
                Err(err) => self.toast = Some((format!("打开失败: {err}"), Instant::now())),
            },
            None => {
                self.toast = Some((
                    format!("{name} 未配置主页 URL（e 编辑添加）"),
                    Instant::now(),
                ))
            }
        }
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
