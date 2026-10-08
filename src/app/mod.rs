//! 应用状态：选中项、导航、探活调度。弹窗与存取在子模块。

pub use modal::{InspectorTarget, Modal, ModelsStatus, SearchTarget, filter_models};

pub(crate) use import::{ImportFlow, ImportOutcome, ImportStep, ModelPick, handle_import_key};

mod import;
mod keys_store;
mod modal;
mod providers_store;
mod undo;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::clients::{Agent, RestartReport};
use crate::clipboard;
use crate::config::{self, KeyEntry};
use crate::probe::{self, BalanceSnapshot, Health, ProbeResult};
use crate::recipe::{ProviderKind, Recipe};

pub(crate) use undo::UndoAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Providers,
    Keys,
}

/// 一个分页（模型 / 非模型）自己的列表视图状态：厂商列表、选中项、过滤词。
/// 两个分页各存一份，所以切来切去都各自停在原处；下标记法同 `ProviderKind::ALL`。
#[derive(Debug, Default)]
pub struct TabView {
    pub provider_ids: Vec<String>,
    pub selected: usize,
    pub filter: Option<String>,
}

#[derive(Debug, Clone)]
pub struct KeyState {
    pub health: Health,
    pub balance: Option<BalanceSnapshot>,
    /// 该 key 最近一次探测结果到达的时间；从未收到结果时为 None。
    pub updated: Option<Instant>,
}

impl Default for KeyState {
    fn default() -> Self {
        Self {
            health: Health::Unknown,
            balance: None,
            updated: None,
        }
    }
}

/// 自动全量刷新间隔：启动刷一次，之后到点后台全量重刷（含探活+额度）。
pub const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// 后台任务回执：模型列表拉取 / 一键导入结果。两者共用一条通道，
/// 因为同一时刻只会有一个弹窗在等结果，消费端按 key_id 匹配即可。
pub enum TaskMsg {
    /// (key_id, 请求代际, 该密钥可见的模型列表)。
    /// 代际来自 App 的单调计数器：面板关掉再开、对同一把密钥重新发请求时，旧结果代际不匹配
    /// 就会被丢弃（否则旧列表会盖掉新列表）。浏览弹窗（`m` 键）不关心代际，固定传 0。
    Models(String, u64, std::result::Result<Vec<String>, String>),
    /// 一键导入到客户端的结果
    Import(Box<ImportOutcome>),
    OAuthBalance(u64, std::result::Result<Vec<String>, String>),
    OAuthLogin(std::result::Result<(), String>),
    /// AUTH 行的 `x`：把 apim 的 OpenAI Codex OAuth 凭据导入 Codex 官方路的结果 + 重载结果。
    OAuthCodex(
        Box<std::result::Result<crate::clients::codex::OfficialReport, String>>,
        crate::clients::RestartReport,
    ),
}

/// 探活/额度结果：带上**探针代际**。配置变更后旧代际的结果会被丢弃，
/// 因此不会把按旧配置算出的数字填回面板（详见 App::apply）。
pub type ProbeMsg = (u64, ProbeResult);

pub struct App {
    pub recipes: HashMap<String, Recipe>,
    pub keys: Vec<KeyEntry>,
    /// 当前分页：模型 / 非模型（Tab 键切换）。
    pub tab: ProviderKind,
    /// 两个分页各自的视图状态（下标 = `ProviderKind::ALL` 的顺序）。
    pub(crate) tabs: [TabView; 2],
    pub selected_key: usize,
    pub focus: Focus,
    pub states: HashMap<String, KeyState>,
    pub toast: Option<(String, Instant)>,
    pub last_refresh: Option<Instant>,
    pub modal: Modal,
    /// 密钥过滤关键字（alias/分组包含，大小写不敏感）；None = 未过滤。
    /// 只影响视图与导航，keys 始终是全集。
    pub key_filter: Option<String>,
    /// 在途探针：key_id → 代际号。既作「该 key 是否在探」的去重依据，
    /// 也用于丢弃过期结果（代际不匹配即作废）。空 = 没有在途探针。
    pub(crate) probe_seq: HashMap<String, u64>,
    /// 探针代际发号器（单调递增）。
    pub(crate) next_probe_seq: u64,
    pub(crate) tx: UnboundedSender<ProbeMsg>,
    pub(crate) tx_task: UnboundedSender<TaskMsg>,
    pub(crate) client: reqwest::Client,
    /// 下一次自动全量刷新的时间点。
    pub(crate) next_auto_refresh: Instant,
    /// OpenAI Codex OAuth quota (separate from per-API-key script results).
    pub oauth_balance: Option<Result<Vec<String>, String>>,
    pub oauth_checking: bool,
    pub oauth_login_running: bool,
    pub oauth_seq: u64,
    /// Codex 现在走哪条路（官方 OAuth / 官方 API Key / 某个 provider / 未登录）。
    /// 与 ★ 角标同思路：**回读 `~/.codex` 现场**算出来的，不记台账，额度面板 AUTH 区显示。
    pub codex_route: crate::clients::codex::CodexRoute,
    /// 配置根目录（`~/.config/apim` 或 `APIM_CONFIG_DIR`）。落盘都经它，
    /// 测试注入临时目录，不碰真实配置。
    pub(crate) config_dir: PathBuf,
    /// 各客户端**现在真正在用**的密钥 id（客户端配置现场读出来的，密钥行 ★ 角标用）。
    /// 启动时算一次，导入成功后与刷新时重算 —— 客户端配置可能被用户手改，所以不信自己的记忆。
    pub active_keys: HashMap<Agent, Vec<String>>,
    /// 客户端配置目录的测试注入（生产为空：各客户端按自身规则解析，如 codex 认 `CODEX_HOME`）。
    pub(crate) agent_homes: HashMap<Agent, PathBuf>,
    /// 导入成功后是否自动重启 codex 守护进程（codex 只在 daemon 启动时读一次模型目录）。
    /// 测试里一律关掉，免得 `cargo test` 去杀用户机器上正在跑的 codex。
    pub(crate) restart_codex_daemon: bool,
    /// 导入面板的请求代际发号器（单调递增）：面板关掉再开、对同一把密钥重发请求时，
    /// 靠它把旧结果丢掉。
    pub(crate) next_import_seq: u64,
    /// 面板 → 客户端适配层的调用点（`None` = 走真实 `Agent::import`）。
    /// 只给测试替换，用来断言请求交给了所选客户端。
    pub(crate) import_runner: Option<import::ImportRunner>,
    /// 本次打开面板后的写操作历史（Ctrl+Z 逐步回退），只存可逆的写操作。
    pub(crate) undo_stack: VecDeque<UndoAction>,
}

impl App {
    pub fn start() -> Result<(App, UnboundedReceiver<ProbeMsg>, UnboundedReceiver<TaskMsg>)> {
        let recipes = crate::recipe::load_recipes()?;
        let keys = config::load_keys(&recipes)?;
        let (tx, rx) = mpsc::unbounded_channel();
        let (tx_task, rx_task) = mpsc::unbounded_channel();
        let config_dir = config::config_dir();
        let agent_homes = HashMap::new();
        let active_keys = Agent::detect_active_keys(&keys, &recipes, &agent_homes);
        let mut app = App {
            recipes,
            keys,
            tab: ProviderKind::Model,
            tabs: Default::default(),
            selected_key: 0,
            focus: Focus::Providers,
            states: HashMap::new(),
            toast: None,
            last_refresh: None,
            modal: Modal::None,
            key_filter: None,
            probe_seq: HashMap::new(),
            next_probe_seq: 0,
            tx,
            tx_task,
            client: probe::client()?,
            next_auto_refresh: Instant::now() + AUTO_REFRESH_INTERVAL,
            oauth_balance: None,
            oauth_checking: false,
            oauth_login_running: false,
            oauth_seq: 0,
            codex_route: crate::clients::codex::CodexRoute::SignedOut,
            config_dir,
            active_keys,
            agent_homes,
            restart_codex_daemon: true,
            next_import_seq: 0,
            import_runner: None,
            undo_stack: VecDeque::new(),
        };
        app.rebuild_provider_list();
        // 打开即全量刷一遍所有厂商；切换厂商只读缓存，到点自动重刷。
        app.refresh_all_keys();
        Ok((app, rx, rx_task))
    }

    pub fn apply(&mut self, msg: ProbeMsg) {
        let (seq, result) = msg;
        // 只接受「当前代际」的结果：配置变更（编辑厂商/换 token/删除/撤销）会让
        // 旧代际作废，迟到的旧结果在此丢弃，不会把按旧配置算出的数字填回面板。
        if self.probe_seq.get(&result.key_id) != Some(&seq) {
            return;
        }
        self.probe_seq.remove(&result.key_id);
        // 密钥已删但探测仍在途：丢弃迟到结果，不在 states 里复活死条目。
        if !self.keys.iter().any(|k| k.id() == result.key_id) {
            return;
        }
        self.last_refresh = Some(Instant::now());
        let state = self.states.entry(result.key_id.clone()).or_default();
        state.health = result.health;
        state.balance = result.balance;
        state.updated = Some(Instant::now());
    }

    /// 配置变更后作废该厂商的在途探针与缓存读数：
    /// - 摘掉 probe_seq：已发出的旧探针结果回来时代际不匹配，会被 `apply` 丢弃；
    /// - 清 states：旧配置算出的数字/健康状态不再可信，等下一次刷新重算。
    pub(crate) fn invalidate_provider(&mut self, id: &str) {
        let prefix = format!("{id}.");
        self.probe_seq.retain(|key, _| !key.starts_with(&prefix));
        self.states.retain(|key, _| !key.starts_with(&prefix));
    }

    /// 单条密钥的配置变更（换 token / 改名 / 删除）：作废它的在途探针与读数。
    pub(crate) fn invalidate_key(&mut self, key_id: &str) {
        self.probe_seq.remove(key_id);
        self.states.remove(key_id);
    }

    pub fn tick(&mut self) {
        if let Some((_, at)) = &self.toast
            && at.elapsed() > Duration::from_secs(3)
        {
            self.toast = None;
        }
        if Instant::now() >= self.next_auto_refresh {
            self.refresh_all_keys();
        }
    }

    pub fn is_checking(&self, key_id: &str) -> bool {
        self.probe_seq.contains_key(key_id)
    }

    /// 用户 recipe 目录（写盘统一走它，测试用临时目录）。
    pub(crate) fn recipes_dir(&self) -> PathBuf {
        self.config_dir.join("recipes")
    }

    // ---- 分页（模型 / 非模型）与列表视图 ----------------------------------

    fn tab_view(&self) -> &TabView {
        &self.tabs[self.tab as usize]
    }

    fn tab_view_mut(&mut self) -> &mut TabView {
        &mut self.tabs[self.tab as usize]
    }

    /// 当前分页的全部厂商 id（未被过滤词收窄）。
    pub fn provider_ids(&self) -> &[String] {
        &self.tab_view().provider_ids
    }

    /// 当前分页里选中的厂商下标（过滤生效时是过滤后视图的下标）。
    pub fn selected_provider(&self) -> usize {
        self.tab_view().selected
    }

    pub fn provider_filter(&self) -> Option<&str> {
        self.tab_view().filter.as_deref()
    }

    pub fn set_provider_filter(&mut self, value: Option<String>) {
        self.tab_view_mut().filter = value;
        // 过滤词一改，原来的选中下标可能越界——选中项必须始终落在可见列表里
        self.clamp_selections();
    }

    /// 某分页的厂商 id（含该分页自己的过滤词）。分页条与当前列表都走它。
    pub fn provider_ids_in(&self, kind: ProviderKind) -> Vec<String> {
        filter_provider_ids(&self.tabs[kind as usize], &self.recipes)
    }

    /// `Tab`：切换厂商分页（模型 ↔ 非模型）。
    ///
    /// 分页各自记着选中项与过滤词，这里只切指针 + 密钥选中归零（选中厂商变了）。
    /// 顺手重算 ★：与切厂商同款，用户可能刚在别的窗口手改了客户端配置。
    pub fn switch_tab(&mut self) {
        let next = match self.tab {
            ProviderKind::Model => ProviderKind::NonModel,
            ProviderKind::NonModel => ProviderKind::Model,
        };
        self.show_tab(next);
    }

    /// 切到指定分页。
    pub fn show_tab(&mut self, kind: ProviderKind) {
        self.tab = kind;
        self.selected_key = 0;
        self.refresh_active_keys();
    }

    /// 把某厂商设为当前分页的选中项——分页跟着厂商的类型走。
    /// 保存 / 复制厂商后必须这么做：否则新建的厂商在另一个分页里，用户看不见它。
    pub(crate) fn focus_provider(&mut self, id: &str) {
        if let Some(kind) = self.recipes.get(id).map(|r| r.kind) {
            self.show_tab(kind);
        }
        let fallback = self.selected_provider();
        let pos = self.provider_ids_filtered().iter().position(|p| p == id);
        self.tab_view_mut().selected = pos.unwrap_or(fallback);
        self.selected_key = 0;
    }

    /// 额度脚本目录（复制厂商时脚本副本落这里）。
    pub(crate) fn scripts_dir(&self) -> PathBuf {
        self.config_dir.join("scripts")
    }

    /// 当前焦点厂商（provider_filter 生效时 selected_provider 是过滤后视图的下标）。
    pub fn current_provider_id(&self) -> Option<&str> {
        let filtered = self.provider_ids_filtered();
        let id = filtered.get(self.selected_provider())?;
        // 借用必须出自全集 provider_ids，不能出自临时的过滤 Vec
        self.provider_ids()
            .iter()
            .find(|p| p == &id)
            .map(String::as_str)
    }

    /// 过滤后的厂商 id 列表（id 或显示名命中 provider_filter，大小写不敏感）。
    /// 无过滤时等于当前分页的 provider_ids 全集；provider_ids 本身不被动。
    pub fn provider_ids_filtered(&self) -> Vec<String> {
        self.provider_ids_in(self.tab)
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

    /// 当前厂商下命中 key_filter 的密钥下标（别名或分组包含，大小写不敏感）。
    /// 无过滤时等于 keys_in_provider 全集。
    pub fn keys_in_provider_filtered(&self) -> Vec<usize> {
        let all = self.keys_in_provider();
        match self.key_filter.as_deref() {
            None | Some("") => all,
            Some(f) => {
                let needle = f.to_lowercase();
                all.into_iter()
                    .filter(|&i| {
                        let k = &self.keys[i];
                        text_contains(&needle, &k.alias)
                            || k.group
                                .as_deref()
                                .is_some_and(|g| text_contains(&needle, g))
                    })
                    .collect()
            }
        }
    }

    pub fn selected_key_entry(&self) -> Option<&KeyEntry> {
        let keys = self.keys_in_provider_filtered();
        keys.get(self.selected_key).copied().map(|i| &self.keys[i])
    }

    /// 密钥表里 AUTH 行的选择位：只有内置 `openai` 分页才有（与 `ui::oauth_row_status` 同一个门），
    /// 排在过滤后密钥行的**后面**（下标 = 过滤后密钥数）。`Some(n)` 时 n 就是那一行的下标。
    pub fn auth_row_index(&self) -> Option<usize> {
        (self.current_provider_id() == Some("openai"))
            .then(|| self.keys_in_provider_filtered().len())
    }

    /// 光标是不是停在 AUTH 行上（`x` 与底栏提示按它分流）。
    pub fn auth_row_selected(&self) -> bool {
        self.auth_row_index() == Some(self.selected_key)
    }

    /// 密钥级按键（c/i/d/m/e）落在 AUTH 行上时的统一提示：那一行只有 `x` 有含义。
    /// 返回 true = 已处理，调用方直接返回。
    fn note_auth_row_only_imports(&mut self) -> bool {
        if !self.auth_row_selected() {
            return false;
        }
        self.toast = Some((
            "AUTH 行是 Codex OAuth：只支持 x（导入到 Codex）".into(),
            Instant::now(),
        ));
        true
    }

    pub fn state_for(&self, key: &KeyEntry) -> KeyState {
        self.states.get(&key.id()).cloned().unwrap_or_default()
    }

    /// 这把密钥**现在正被哪些**客户端用（密钥行的 ★ 角标；顺序同 `Agent::ALL`）。
    pub fn agents_using(&self, key_id: &str) -> Vec<Agent> {
        Agent::ALL
            .iter()
            .copied()
            .filter(|agent| {
                self.active_keys
                    .get(agent)
                    .is_some_and(|ids| ids.iter().any(|id| id == key_id))
            })
            .collect()
    }

    /// 重读各客户端配置，刷新「哪把密钥现在正被谁用」（★ 角标）。
    /// 客户端配置可能被用户手改，所以导入成功后、刷新时都要重算，而不是信 apim 自己的记忆。
    ///
    /// 顺手回读 codex 现在走哪条路（官方 OAuth / 官方 API Key / 某个 provider / 未登录）：
    /// 与 ★ 同一个节奏，用户在别的窗口里改了 `~/.codex` 也能立刻看到。
    pub(crate) fn refresh_active_keys(&mut self) {
        self.active_keys = Agent::detect_active_keys(&self.keys, &self.recipes, &self.agent_homes);
        self.codex_route = crate::clients::codex::codex_route(Some(&self.codex_home()));
    }

    /// codex 的配置目录（`CODEX_HOME` / `~/.codex`；测试经 `agent_homes` 注入）。
    pub(crate) fn codex_home(&self) -> PathBuf {
        self.agent_homes
            .get(&Agent::Codex)
            .cloned()
            .unwrap_or_else(crate::clients::codex::codex_home)
    }

    /// 重建两个分页各自的厂商列表：有密钥的厂商排在前面（顺序同密钥清单），
    /// 其余按 id 排序。两个分页互不干扰，各自只收自己类型的厂商。
    pub(crate) fn rebuild_provider_list(&mut self) {
        for (i, kind) in ProviderKind::ALL.iter().enumerate() {
            let mut ids: Vec<String> = Vec::new();
            for key in &self.keys {
                let mine = self.recipes.get(&key.provider).map(|r| r.kind) == Some(*kind);
                if mine && !ids.iter().any(|p| p == &key.provider) {
                    ids.push(key.provider.clone());
                }
            }
            let mut rest: Vec<String> = self
                .recipes
                .iter()
                .filter(|(id, r)| r.kind == *kind && !ids.iter().any(|p| p == *id))
                .map(|(id, _)| id.clone())
                .collect();
            rest.sort();
            ids.extend(rest);
            self.tabs[i].provider_ids = ids;
        }
        self.clamp_selections();
    }

    /// 过滤/增删后把选中项钳回过滤后视图的有效范围（无过滤时即全集范围）。
    /// **两个分页都钳**：非当前分页的列表也可能刚缩水（在另一页撤销/删厂商），
    /// 留着越界的选中下标，切回去就是「有厂商但一个都没选中」。
    pub(crate) fn clamp_selections(&mut self) {
        for i in 0..self.tabs.len() {
            let n = filter_provider_ids(&self.tabs[i], &self.recipes).len();
            let selected = self.tabs[i].selected;
            self.tabs[i].selected = if n == 0 { 0 } else { selected.min(n - 1) };
        }
        let nk = self.keys_in_provider_filtered().len();
        // AUTH 行也算一个可选位置（openai 分页）：上限 = 密钥数 + 1
        let limit = self.auth_row_index().map(|i| i + 1).unwrap_or(nk);
        if limit == 0 {
            self.selected_key = 0;
        } else if self.selected_key >= limit {
            self.selected_key = limit - 1;
        }
    }

    /// 是否有任何过滤生效（决定无弹窗 Esc 是清过滤还是退出）。
    pub fn has_filter(&self) -> bool {
        self.key_filter.is_some() || self.provider_filter().is_some()
    }

    /// 无弹窗 Esc：优先清当前焦点列表的过滤，焦点侧没有则清另一侧（一次只清一个）。
    /// 返回是否清掉了过滤（清掉了就不退出 TUI）。
    pub fn clear_filter(&mut self) -> bool {
        let (keys_set, provider_set) =
            (self.key_filter.is_some(), self.tab_view().filter.is_some());
        let clear_keys = match (self.focus, keys_set, provider_set) {
            (_, false, false) => return false,
            (Focus::Keys, true, _) => true,
            (Focus::Keys, false, true) => false,
            (Focus::Providers, _, true) => false,
            (Focus::Providers, true, false) => true,
        };
        if clear_keys {
            self.key_filter = None;
        } else {
            self.tab_view_mut().filter = None;
        }
        self.clamp_selections();
        true
    }

    pub fn move_up(&mut self) {
        match self.focus {
            Focus::Providers => {
                let selected = self.selected_provider();
                if selected > 0 {
                    self.select_provider(selected - 1);
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
                let selected = self.selected_provider();
                if selected + 1 < self.provider_ids_filtered().len() {
                    self.select_provider(selected + 1);
                }
            }
            Focus::Keys => {
                let n = self.keys_in_provider_filtered().len();
                // AUTH 行（openai 分页）也是可选中的一行，排在密钥行之后
                let limit = self.auth_row_index().map(|i| i + 1).unwrap_or(n);
                if limit > 0 && self.selected_key + 1 < limit {
                    self.selected_key += 1;
                }
            }
        }
    }

    /// 选中另一个厂商（`j`/`k`/方向键切厂商都走这里）。
    ///
    /// 顺手把 ★ 的现场重算一遍：用户常常在另一个窗口/编辑器里手改了客户端配置再切回来，
    /// 按一下键就该看到真话，不用等 `r` 或 5 分钟的自动刷新（只读一个小文件，无网络）。
    fn select_provider(&mut self, pos: usize) {
        self.tab_view_mut().selected = pos;
        self.selected_key = 0;
        self.refresh_active_keys();
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
                if self.note_auth_row_only_imports() {
                    return;
                }
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

    /// 全量刷新：所有厂商的所有密钥（启动一次 + 每 AUTO_REFRESH_INTERVAL 一次）。
    pub fn refresh_all_keys(&mut self) {
        // 用户可能手改了客户端的 config（切 provider / 换 token），顺手把 ★ 重算一遍
        self.refresh_active_keys();
        for idx in 0..self.keys.len() {
            self.spawn_probe(idx);
        }
        self.spawn_oauth_probe();
        self.next_auto_refresh = Instant::now() + AUTO_REFRESH_INTERVAL;
    }

    /// 手动刷新（`r`）：当前厂商。切换厂商不触发刷新，只读缓存。
    pub fn refresh_current_provider(&mut self) {
        self.refresh_active_keys();
        let idxs = self.keys_in_provider();
        for idx in idxs {
            self.spawn_probe(idx);
        }
        if self.current_provider_id() == Some("openai") {
            self.spawn_oauth_probe();
        }
    }

    /// 按**厂商 id** 刷新（配置变更后用新配置重探）。
    /// 不依赖当前选中项，因此过滤生效时也不会探错厂商。
    pub(crate) fn refresh_provider(&mut self, provider: &str) {
        for idx in 0..self.keys.len() {
            if self.keys[idx].provider == provider {
                self.spawn_probe(idx);
            }
        }
    }

    fn spawn_oauth_probe(&mut self) {
        if self.oauth_checking {
            return;
        }
        self.oauth_checking = true;
        self.oauth_seq += 1;
        let seq = self.oauth_seq;
        let dir = self.config_dir.clone();
        let tx = self.tx_task.clone();
        tokio::spawn(async move {
            let result = crate::openai_auth::fetch_usage(&dir)
                .await
                .map(|usage| usage.map(|u| u.lines).unwrap_or_default())
                .map_err(|e| e.to_string());
            let _ = tx.send(TaskMsg::OAuthBalance(seq, result));
        });
    }

    pub fn apply_oauth_balance(&mut self, seq: u64, result: Result<Vec<String>, String>) {
        if seq != self.oauth_seq {
            return;
        }
        self.oauth_checking = false;
        self.oauth_balance = Some(result);
    }

    pub fn start_openai_oauth_login(&mut self) {
        if self.oauth_login_running {
            return;
        }
        self.oauth_login_running = true;
        let dir = self.config_dir.clone();
        let tx = self.tx_task.clone();
        tokio::spawn(async move {
            let result = crate::openai_auth::login(&dir)
                .await
                .map_err(|e| format!("{e:#}"));
            let _ = tx.send(TaskMsg::OAuthLogin(result));
        });
    }

    /// 右下额度面板里的 `o` 提示：它只在内置 OpenAI 厂商的密钥栏有效。
    /// 无效位置不能静默（m/x/c/d/e/i 都会解释自己）。
    pub fn note_oauth_unavailable(&mut self) {
        self.toast = Some((
            "o：仅内置 OpenAI 厂商的密钥栏支持 Codex OAuth 登录".into(),
            Instant::now(),
        ));
    }

    pub fn apply_oauth_login(&mut self, result: Result<(), String>) {
        self.oauth_login_running = false;
        match result {
            Ok(()) => {
                self.toast = Some(("OpenAI Codex OAuth 已连接".into(), Instant::now()));
                self.oauth_balance = None;
                self.oauth_checking = false;
                self.spawn_oauth_probe();
            }
            Err(error) => {
                // 错误链在前、日志路径在后：底栏很窄，真正有用的原因不能被路径挤掉
                let log = crate::openai_auth::log_path(&self.config_dir);
                self.toast = Some((
                    format!("OAuth 登录失败: {error}（详情见 {}）", log.display()),
                    Instant::now(),
                ));
            }
        }
    }

    /// AUTH 行按 `x`：把 apim 的 OpenAI Codex OAuth 凭据导入 Codex 的**官方路**
    /// （写 `auth.json` + 摘掉 config.toml 的第三方路由），校验通过后重启 codex 守护进程。
    ///
    /// 与第三方导入（`x` 面板）分开：官方路没有「选模型」这回事（模型表由 ChatGPT 后端给），
    /// 也不需要 apim 的密钥行。
    pub fn import_oauth_to_codex(&mut self) {
        if self.oauth_login_running {
            self.toast = Some(("OAuth 登录还在进行，等它结束再导入".into(), Instant::now()));
            return;
        }
        let cred = match crate::openai_auth::load(&self.config_dir) {
            Ok(Some(cred)) => cred,
            Ok(None) => {
                self.toast = Some((
                    "还没有 OAuth 凭据：先按 o 完成 OpenAI Codex 授权".into(),
                    Instant::now(),
                ));
                return;
            }
            Err(err) => {
                self.toast = Some((format!("读取 OAuth 凭据失败：{err:#}"), Instant::now()));
                return;
            }
        };
        let home = self.codex_home();
        let restart_daemon = self.restart_codex_daemon;
        let tx = self.tx_task.clone();
        tokio::spawn(async move {
            let (result, restart) = tokio::task::spawn_blocking(move || {
                let result = crate::clients::codex::import_official(&home, &cred);
                // 写成功才重启：codex 的 daemon 只在启动时读一次配置
                let restart = if result.is_ok() && restart_daemon {
                    crate::clients::codex::restart_daemon().unwrap_or_default()
                } else {
                    RestartReport::default()
                };
                (result, restart)
            })
            .await
            .unwrap_or_else(|err| {
                (
                    Err(format!("导入任务异常终止：{err}")),
                    RestartReport::default(),
                )
            });
            let _ = tx.send(TaskMsg::OAuthCodex(Box::new(result), restart));
        });
    }

    /// 官方路导入结果落地：成功就把「改了哪些键 / 备份在哪 / 要不要重启」说清楚。
    pub fn apply_oauth_codex(
        &mut self,
        result: Result<crate::clients::codex::OfficialReport, String>,
        restart: RestartReport,
    ) {
        match result {
            Ok(report) => {
                // 配置变了：★ 与「codex 走哪条路」都要按新现场重算（第三方密钥的 ★ 会消失）
                self.refresh_active_keys();
                let mut note = "已把 OpenAI Codex OAuth 导入 Codex（官方路）".to_string();
                if !report.removed.is_empty() {
                    note.push_str(&format!(" · 摘掉 {}", report.removed.join("/")));
                }
                if !report.backups.is_empty() {
                    let names: Vec<String> = report
                        .backups
                        .iter()
                        .filter_map(|path| path.file_name().and_then(|n| n.to_str()))
                        .map(str::to_string)
                        .collect();
                    note.push_str(&format!(" · 旧配置备份为 {}", names.join(" / ")));
                }
                if restart.killed > 0 {
                    note.push_str(&format!(
                        " · 已重启 Codex 守护进程({} 个)，重开它即可用官方账号",
                        restart.killed
                    ));
                } else if restart.ambiguous > 0 {
                    note.push_str(&format!(
                        " · 没找到标准形态的 Codex 守护进程（有 {} 个类似进程没敢动），若它正开着请手动重启",
                        restart.ambiguous
                    ));
                } else {
                    note.push_str(" · 重开 Codex 生效");
                }
                self.toast = Some((note, Instant::now()));
            }
            Err(message) => {
                self.toast = Some((format!("导入 Codex 官方路失败：{message}"), Instant::now()))
            }
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
        if self.probe_seq.contains_key(&id) {
            return;
        }
        self.next_probe_seq += 1;
        let seq = self.next_probe_seq;
        self.probe_seq.insert(id.clone(), seq);
        self.states.entry(id).or_default().health = Health::Checking;
        let tx = self.tx.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            let result = probe::probe(&client, &recipe, &key).await;
            let _ = tx.send((seq, result));
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
            self.next_probe_seq += 1;
            let seq = self.next_probe_seq;
            self.probe_seq.insert(id.clone(), seq);
            self.states.entry(id).or_default().health = Health::Checking;
            let client = client.clone();
            futs.push(async move { (seq, probe::probe(&client, &recipe, &key).await) });
        }
        for msg in futures::future::join_all(futs).await {
            self.apply(msg);
        }
        // 快照也把 AUTH 额度拉真数据，否则 `--snapshot` 面板与 TUI 不一致
        if self.current_provider_id() == Some("openai") {
            let result = crate::openai_auth::fetch_usage(&self.config_dir)
                .await
                .map(|usage| usage.map(|u| u.lines).unwrap_or_default())
                .map_err(|e| e.to_string());
            self.oauth_balance = Some(result);
        }
    }

    pub fn toast_text(&self) -> Option<&str> {
        self.toast.as_ref().map(|(s, _)| s.as_str())
    }
}

/// 大小写不敏感的包含判断（needle 应已 to_lowercase）。
fn text_contains(needle_lower: &str, haystack: &str) -> bool {
    haystack.to_lowercase().contains(needle_lower)
}

/// 分页视图 + 过滤词 → 命中的厂商 id（顺序沿 provider_ids）。
fn filter_provider_ids(view: &TabView, recipes: &HashMap<String, Recipe>) -> Vec<String> {
    match view.filter.as_deref() {
        None | Some("") => view.provider_ids.clone(),
        Some(f) => {
            let needle = f.to_lowercase();
            view.provider_ids
                .iter()
                .filter(|id| {
                    let name = recipes
                        .get(*id)
                        .map(|r| r.name.as_str())
                        .unwrap_or(id.as_str());
                    text_contains(&needle, id) || text_contains(&needle, name)
                })
                .cloned()
                .collect()
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::recipe::Auth;
    use std::collections::HashSet;
    use tokio::time::timeout;

    /// 直接构造 App：假 recipe（health=None、balance=None，probe 立即返回、零网络），
    /// 不经过 App::start，不读任何配置目录，不触碰 ~/.config/apim。
    pub(crate) fn test_app(
        providers: &[(&str, &[&str])],
    ) -> (App, UnboundedReceiver<ProbeMsg>, UnboundedReceiver<TaskMsg>) {
        let mut recipes = HashMap::new();
        let mut keys = Vec::new();
        for (pid, aliases) in providers {
            recipes.insert(
                (*pid).to_string(),
                Recipe {
                    id: (*pid).to_string(),
                    name: format!("{pid} 假厂商"),
                    kind: ProviderKind::Model,
                    base_url: "https://example.invalid".into(),
                    homepage: None,
                    models_url: None,
                    supports_groups: false,
                    vars: HashMap::new(),
                    auth: Auth::default(),
                    health: None,
                    balance: None,
                    origin: None,
                },
            );
            for alias in *aliases {
                keys.push(KeyEntry {
                    provider: (*pid).to_string(),
                    alias: (*alias).to_string(),
                    group: None,
                    token: "sk-test-placeholder".into(),
                });
            }
        }
        let (tx, rx) = mpsc::unbounded_channel();
        let (tx_task, rx_task) = mpsc::unbounded_channel();
        let mut app = App {
            recipes,
            keys,
            tab: ProviderKind::Model,
            tabs: Default::default(),
            selected_key: 0,
            focus: Focus::Keys,
            states: HashMap::new(),
            toast: None,
            last_refresh: None,
            modal: Modal::None,
            key_filter: None,
            probe_seq: HashMap::new(),
            next_probe_seq: 0,
            tx,
            tx_task,
            client: probe::client().expect("构建测试用 reqwest client"),
            next_auto_refresh: Instant::now() + AUTO_REFRESH_INTERVAL,
            config_dir: test_config_dir("app"),
            active_keys: HashMap::new(),
            agent_homes: HashMap::from([
                (Agent::Codex, test_agent_home()),
                (Agent::Pi, test_agent_home()),
            ]),
            restart_codex_daemon: false,
            oauth_balance: None,
            oauth_checking: false,
            oauth_login_running: false,
            oauth_seq: 0,
            codex_route: crate::clients::codex::CodexRoute::SignedOut,
            next_import_seq: 0,
            import_runner: None,
            undo_stack: VecDeque::new(),
        };
        app.rebuild_provider_list();
        (app, rx, rx_task)
    }

    /// 测试用配置目录：target/ 下的临时目录，避免任何测试写到真实 ~/.config/apim。
    pub(crate) fn test_config_dir(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("apim-app-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// 每个测试 App 一个**空**的客户端配置目录：★ 检测默认什么都读不到，
    /// 不碰开发者本机的 `~/.codex`。要测「codex 现在在用哪把密钥」就往里写 config.toml。
    pub(crate) fn test_agent_home() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "apim-agent-home-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建测试用的客户端配置目录");
        dir
    }

    /// 已过期的触发点：取 1s 前；极端情况下（刚开机，时钟起点晚于 1s 前）退回 now，
    /// tick 判断 now >= next 仍成立。
    fn overdue() -> Instant {
        Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now)
    }

    fn remaining_until_next(app: &App) -> Duration {
        app.next_auto_refresh
            .checked_duration_since(Instant::now())
            .expect("next_auto_refresh 应排在未来")
    }

    #[tokio::test]
    async fn tick_when_due_probes_every_key_and_reschedules() {
        let (mut app, mut rx, _rx_models) =
            test_app(&[("alpha", &["a1", "a2"]), ("beta", &["b1"])]);
        app.next_auto_refresh = overdue();
        app.tick();
        let mut got = HashSet::new();
        for _ in 0..3 {
            let msg = timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("到点 tick 应触发全量探测")
                .expect("channel 不应关闭");
            got.insert(msg.1.key_id);
        }
        assert_eq!(
            got,
            HashSet::from([
                "alpha.a1".to_string(),
                "alpha.a2".to_string(),
                "beta.b1".to_string(),
            ])
        );
        // 触发全量后应重新排期到 ~5 分钟后，而不是立刻连环触发
        let until = remaining_until_next(&app);
        assert!(
            until > Duration::from_secs(4 * 60),
            "下一次自动刷新排得太近: {until:?}"
        );
        assert!(until <= AUTO_REFRESH_INTERVAL);
    }

    #[tokio::test]
    async fn switching_provider_reads_cache_without_probing() {
        let (mut app, mut rx, _rx_models) = test_app(&[("alpha", &["a1"]), ("beta", &["b1"])]);
        app.focus = Focus::Providers;
        app.move_down();
        assert_eq!(app.current_provider_id(), Some("beta"));
        app.move_up();
        assert_eq!(app.current_provider_id(), Some("alpha"));
        // 切换厂商只读缓存：短时间内不应收到任何探测消息，也不应有在途探针
        let leaked = timeout(Duration::from_millis(150), rx.recv()).await;
        assert!(leaked.is_err(), "切换厂商不应触发探测，却收到了消息");
        assert!(app.probe_seq.is_empty());
    }

    #[tokio::test]
    async fn refresh_all_keys_schedules_next_cycle_five_minutes_out() {
        let (mut app, mut rx, _rx_models) = test_app(&[("alpha", &["a1"])]);
        app.refresh_all_keys();
        assert!(app.is_checking("alpha.a1"));
        let until = remaining_until_next(&app);
        assert!(until <= AUTO_REFRESH_INTERVAL);
        assert!(
            until > AUTO_REFRESH_INTERVAL - Duration::from_secs(5),
            "排期偏差过大: {until:?}"
        );
        // 收结果并 apply：在途清空、last_refresh 落地
        let msg = timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("应有一条探测结果")
            .expect("channel 不应关闭");
        app.apply(msg);
        assert!(!app.is_checking("alpha.a1"));
        assert!(app.probe_seq.is_empty());
        assert!(app.last_refresh.is_some());
    }

    #[tokio::test]
    async fn refresh_all_keys_dedupes_probes() {
        let (mut app, mut rx, _rx_models) = test_app(&[("alpha", &["a1", "a2"])]);
        app.refresh_all_keys();
        // 结果尚未 apply（同步代码不 yield），在途仍在——第二次全量应被去重
        app.refresh_all_keys();
        let mut received = Vec::new();
        loop {
            match timeout(Duration::from_millis(300), rx.recv()).await {
                Ok(Some(msg)) => received.push(msg),
                Ok(None) => panic!("channel 提前关闭"),
                Err(_) => break,
            }
        }
        assert_eq!(
            received.len(),
            2,
            "每个 key 恰好一条结果，重复全量应被在途标记去重"
        );
        for msg in received {
            app.apply(msg);
        }
        assert!(app.probe_seq.is_empty());
    }

    /// 结果只接受「当前代际」：配置变更作废后，迟到结果不得回填旧数字。
    /// 关键场景：作废后立刻重探会重新占上同一个 key id——旧结果仍必须被丢弃。
    #[test]
    fn apply_drops_results_from_stale_probe_generation() {
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1"])]);
        let stale = |ms| ProbeResult {
            key_id: "alpha.a1".into(),
            health: Health::Live { ms },
            balance: None,
        };

        // 没有任何在途探针 → 丢弃
        app.apply((1, stale(7)));
        assert!(!app.states.contains_key("alpha.a1"), "无在途探针不应落库");

        // 当前代际 = 1 → 接受
        app.probe_seq.insert("alpha.a1".into(), 1);
        app.apply((1, stale(7)));
        assert!(matches!(app.states["alpha.a1"].health, Health::Live { .. }));

        // 配置变更：作废旧代际与读数
        app.invalidate_provider("alpha");
        assert!(!app.states.contains_key("alpha.a1"));
        assert!(!app.is_checking("alpha.a1"));

        // 立刻重探（新代际 2 占回同一个 key）→ 旧代际 1 的结果必须被丢弃
        app.probe_seq.insert("alpha.a1".into(), 2);
        app.apply((1, stale(99)));
        assert!(
            !app.states.contains_key("alpha.a1"),
            "旧代际结果不得回填（哪怕新探针占着同一个 key）"
        );
        // 新代际的结果正常落地
        app.apply((2, stale(11)));
        assert!(matches!(
            app.states["alpha.a1"].health,
            Health::Live { ms: 11 }
        ));
    }

    /// 保存厂商后：旧在途探针作废、旧读数清空，并按新配置立刻重探。
    #[tokio::test]
    async fn saving_provider_invalidates_and_reprobes() {
        let (mut app, _rx, _rx_models) = test_app(&[("p", &["main"])]);
        app.config_dir = test_config_dir("provider-reprobe");
        app.focus = Focus::Providers;
        // 模拟旧配置的探针在途（代际 1；发号器也推到 1，保证新探针拿到 2）+ 旧读数
        app.next_probe_seq = 1;
        app.probe_seq.insert("p.main".into(), 1);
        app.states.insert(
            "p.main".into(),
            KeyState {
                health: Health::Live { ms: 9 },
                ..Default::default()
            },
        );
        let mut form = crate::form::provider_add(ProviderKind::Model);
        form.fields[0] = crate::form::Field::text("ID", "p");
        form.fields[1] = crate::form::Field::text("名称", "改过");
        form.fields[2] = crate::form::Field::text("Base URL", "https://p2.example.invalid");
        form.fields[5] = crate::form::Field::text("探活路径", ""); // 不联网
        form.fields[6] = crate::form::Field::text("脚本路径", "");

        app.save_provider_form(&form, Some("p"));

        assert!(
            app.is_checking("p.main"),
            "旧探针作废后应发新的（否则会被在途标记去重挡掉）"
        );
        assert!(
            matches!(app.states["p.main"].health, Health::Checking),
            "旧读数应清掉并转「检查中」"
        );
        // 旧代际（1）的迟到结果不得覆盖新探针
        let new_seq = app.probe_seq["p.main"];
        assert_ne!(new_seq, 1, "新探针必须是新代际");
        app.apply((
            1,
            ProbeResult {
                key_id: "p.main".into(),
                health: Health::Live { ms: 9 },
                balance: None,
            },
        ));
        assert!(
            matches!(app.states["p.main"].health, Health::Checking),
            "旧代际结果不得覆盖新探针"
        );
    }

    // ---- `/` 实时过滤 ------------------------------------------------------

    /// 给密钥补分组，供分组命中用例使用。
    fn with_groups(app: &mut App) {
        app.keys[0].group = Some("生产".into());
        app.keys[1].group = Some("个人".into());
    }

    #[test]
    fn key_filter_matches_alias_case_insensitive() {
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["Work", "personal"])]);
        app.key_filter = Some("WORK".into());
        let hits: Vec<&str> = app
            .keys_in_provider_filtered()
            .into_iter()
            .map(|i| app.keys[i].alias.as_str())
            .collect();
        assert_eq!(hits, ["Work"]);
        // 全集语义不受过滤影响
        assert_eq!(app.keys_in_provider().len(), 2);
    }

    #[test]
    fn key_filter_matches_group_name() {
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1", "a2"])]);
        with_groups(&mut app);
        app.key_filter = Some("个人".into());
        let hits: Vec<&str> = app
            .keys_in_provider_filtered()
            .into_iter()
            .map(|i| app.keys[i].alias.as_str())
            .collect();
        assert_eq!(hits, ["a2"]);
    }

    #[test]
    fn key_filter_no_match_yields_empty_but_full_list_stays() {
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1", "a2"])]);
        app.key_filter = Some("zzz".into());
        assert!(app.keys_in_provider_filtered().is_empty());
        assert_eq!(app.keys_in_provider().len(), 2);
        // 空关键字视为未过滤
        app.key_filter = Some(String::new());
        assert_eq!(app.keys_in_provider_filtered().len(), 2);
    }

    #[test]
    fn selected_clamps_when_filter_shrinks_list() {
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1", "a2", "a3"])]);
        app.selected_key = 2;
        app.clamp_selections();
        assert_eq!(app.selected_key, 2);
        // 过滤只剩 1 条后越界，应钳回 0
        app.key_filter = Some("a1".into());
        app.clamp_selections();
        assert_eq!(app.selected_key, 0);
        assert!(app.selected_key_entry().is_some());
    }

    #[test]
    fn provider_filter_matches_id_or_display_name() {
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1"]), ("beta", &["b1"])]);
        app.set_provider_filter(Some("ALP".into()));
        assert_eq!(app.provider_ids_filtered(), ["alpha"]);
        assert_eq!(app.current_provider_id(), Some("alpha"));
        // 显示名「alpha 假厂商」命中
        app.set_provider_filter(Some("假厂".into()));
        assert_eq!(app.provider_ids_filtered().len(), 2);
        // 导航吃过滤后的列表：只看 beta
        app.set_provider_filter(Some("beta".into()));
        app.select_provider(0);
        app.move_down();
        assert_eq!(app.selected_provider(), 0, "过滤后只有一项，不应移动");
    }

    /// 登录失败的 toast 必须**先说原因**（错误链），日志路径只能跟在后头：
    /// 底栏很窄，路径把原因挤出去等于什么都没说。
    #[test]
    fn failed_login_toast_leads_with_the_cause() {
        let (mut app, _rx, _rx_models) = test_app(&[("openai", &["k"])]);
        app.oauth_login_running = true;
        app.apply_oauth_login(Err("OAuth 回调端口 1455 被占用".into()));
        let toast = app.toast_text().expect("失败必须有提示").to_string();
        assert!(
            toast.starts_with("OAuth 登录失败: OAuth 回调端口 1455 被占用"),
            "{toast}"
        );
        assert!(toast.ends_with('）'), "日志路径应在最后：{toast}");
        assert!(!app.oauth_login_running, "失败也要把登录中状态清掉");
    }

    /// 不可用位置的提示文案（路由分支本身在 src/tui/tests.rs 里走真实按键验证）。
    #[test]
    fn oauth_unavailable_toast_explains_itself() {
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1"])]);
        app.note_oauth_unavailable();
        assert!(
            app.toast_text().unwrap().contains("仅内置 OpenAI"),
            "{:?}",
            app.toast_text()
        );
    }

    /// 切厂商（`j`/`k`）时顺手重算 ★ 现场：用户在别的窗口手改了客户端配置，
    /// 切回来按一下键就该看到真话，不用等 `r` 或 5 分钟自动刷新。
    #[test]
    fn switching_provider_rechecks_what_the_clients_use() {
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1"]), ("beta", &["b1"])]);
        app.focus = Focus::Providers;
        // 启动时读到的现场里没人被用；随后用户在另一个窗口把 codex 切到了 beta
        assert!(app.agents_using("beta.b1").is_empty());
        let home = app.agent_homes[&Agent::Codex].clone();
        std::fs::write(
            home.join("config.toml"),
            "model_provider = \"beta\"\n\n[model_providers.beta]\n\
             base_url = \"https://example.invalid/v1\"\n\
             experimental_bearer_token = \"sk-test-placeholder\"\n",
        )
        .unwrap();

        app.move_down();

        assert_eq!(app.current_provider_id(), Some("beta"), "应切到了 beta");
        assert_eq!(
            app.agents_using("beta.b1"),
            vec![Agent::Codex],
            "切厂商时应重算 ★，不要等 r 或自动刷新"
        );
    }

    /// 两个客户端都回读各自的现场：pi 扫它配置里的每一份凭据（`auth.json` 与 `models.json`），
    /// 用户手改了凭据也要能掉 ★。
    #[test]
    fn pi_is_detected_from_its_own_config_too() {
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1"])]);
        let home = app.agent_homes[&Agent::Pi].clone();
        // 两样都要能认：用户自己起的 provider 名（没有 apim- 前缀）+ 凭据在 auth.json 里。
        // （settings.json 不参与判定，这里故意不写它。）
        std::fs::write(
            home.join("models.json"),
            "{ \"providers\": { \"my-relay\": { \"baseUrl\": \"https://example.invalid/v1\" } } }",
        )
        .unwrap();
        std::fs::write(
            home.join("auth.json"),
            "{ \"my-relay\": { \"type\": \"api_key\", \"key\": \"sk-test-placeholder\" } }",
        )
        .unwrap();

        app.refresh_active_keys();
        assert_eq!(app.agents_using("alpha.a1"), vec![Agent::Pi]);

        // 用户在 pi 里换成了别的凭据（/login 或手改 auth.json）
        std::fs::write(
            home.join("auth.json"),
            "{ \"my-relay\": { \"type\": \"api_key\", \"key\": \"sk-hand-written\" } }",
        )
        .unwrap();
        app.refresh_active_keys();
        assert!(app.agents_using("alpha.a1").is_empty());
    }

    #[test]
    fn search_cancel_restores_original_filter() {
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1", "a2"])]);
        app.key_filter = Some("a1".into());
        app.open_search();
        assert!(matches!(app.modal, Modal::Search { .. }));
        if let Modal::Search { edit, .. } = &mut app.modal {
            edit.insert("xyz");
        }
        app.apply_live_filter();
        assert_eq!(app.key_filter.as_deref(), Some("a1xyz"));
        app.cancel_search();
        assert_eq!(app.key_filter.as_deref(), Some("a1"));
        assert!(matches!(app.modal, Modal::None));
    }

    #[test]
    fn search_apply_writes_trimmed_value_and_closes() {
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1"])]);
        app.open_search();
        if let Modal::Search { edit, .. } = &mut app.modal {
            edit.insert("  a  ");
        }
        app.apply_search();
        assert_eq!(app.key_filter.as_deref(), Some("a"));
        assert!(matches!(app.modal, Modal::None));
        // 空输入 = 清除过滤
        app.open_search();
        if let Modal::Search { edit, .. } = &mut app.modal {
            edit.backspace();
            edit.backspace();
            edit.backspace();
        }
        app.apply_search();
        assert_eq!(app.key_filter, None);
    }

    #[test]
    fn clear_filter_prefers_focused_side_then_other() {
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1"])]);
        app.key_filter = Some("a".into());
        app.set_provider_filter(Some("b".into()));
        app.focus = Focus::Keys;
        assert!(app.clear_filter());
        assert_eq!(app.key_filter, None);
        assert_eq!(app.provider_filter(), Some("b"));
        assert!(app.clear_filter(), "焦点侧没有过滤时应清另一侧");
        assert!(!app.clear_filter());
        assert!(!app.has_filter());
    }

    // ---- 分页（模型 / 非模型）---------------------------------------------

    /// 按类型造两个厂商：模型页与非模型页各自成列表，互不混入。
    fn two_kind_app() -> App {
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1"]), ("deepl", &["main"])]);
        app.recipes.get_mut("deepl").unwrap().kind = ProviderKind::NonModel;
        app.rebuild_provider_list();
        app
    }

    #[test]
    fn providers_are_split_across_tabs_by_kind() {
        let mut app = two_kind_app();
        assert_eq!(app.tab, ProviderKind::Model, "默认停在模型页");
        assert_eq!(app.provider_ids(), ["alpha"]);
        assert_eq!(app.current_provider_id(), Some("alpha"));
        app.switch_tab();
        assert_eq!(app.tab, ProviderKind::NonModel);
        assert_eq!(app.provider_ids(), ["deepl"]);
        assert_eq!(app.current_provider_id(), Some("deepl"));
    }

    /// 分页各自记着选中项与过滤词：切来切去都停在原处，过滤词不泄到另一页。
    #[test]
    fn each_tab_keeps_its_own_selection_and_filter() {
        let (mut app, _rx, _rx_models) =
            test_app(&[("alpha", &["a1"]), ("beta", &["b1"]), ("deepl", &["main"])]);
        app.recipes.get_mut("deepl").unwrap().kind = ProviderKind::NonModel;
        app.rebuild_provider_list();
        app.select_provider(1); // 模型页选 beta
        app.set_provider_filter(Some("beta".into()));
        app.switch_tab(); // → 非模型页
        assert_eq!(app.current_provider_id(), Some("deepl"));
        assert_eq!(app.provider_filter(), None, "过滤词不跨页");
        assert_eq!(app.provider_ids_filtered(), ["deepl"]);

        app.switch_tab(); // 切回模型页
        assert_eq!(app.current_provider_id(), Some("beta"));
        assert_eq!(app.provider_filter(), Some("beta"));
    }

    /// 有密钥的厂商排前面，其余按 id 排序；两个分页各自按这套规则排。
    #[test]
    fn rebuilt_lists_put_providers_with_keys_first() {
        let (mut app, _rx, _rx_models) = test_app(&[("zeta", &["z1"])]);
        for id in ["alpha", "mu", "nu"] {
            app.recipes.insert(
                id.into(),
                Recipe {
                    id: id.into(),
                    name: id.into(),
                    kind: ProviderKind::Model,
                    base_url: "https://example.invalid".into(),
                    homepage: None,
                    models_url: None,
                    supports_groups: false,
                    vars: HashMap::new(),
                    auth: Auth::default(),
                    health: None,
                    balance: None,
                    origin: None,
                },
            );
        }
        app.rebuild_provider_list();
        assert_eq!(app.provider_ids(), ["zeta", "alpha", "mu", "nu"]);
    }

    /// 保存 / 复制厂商后分页跟着厂商类型走，否则新建的厂商落在看不见的那一页。
    #[tokio::test]
    async fn saving_a_provider_switches_to_its_tab() {
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1"])]);
        app.config_dir = test_config_dir("save-tab");
        app.focus = Focus::Providers;
        app.switch_tab(); // 停在非模型页
        assert_eq!(app.tab, ProviderKind::NonModel);

        let mut form = crate::form::provider_add(ProviderKind::Model);
        form.fields[0] = crate::form::Field::text("ID", "beta");
        form.fields[1] = crate::form::Field::text("名称", "Beta");
        form.fields[2] = crate::form::Field::text("Base URL", "https://beta.example.invalid");
        app.save_provider_form(&form, None);

        assert_eq!(
            app.tab,
            ProviderKind::Model,
            "新建的模型厂商应让分页跳回模型页"
        );
        assert_eq!(app.current_provider_id(), Some("beta"));
    }

    /// 勾上「非模型」保存：类型落盘，探活路径（表单里还带着默认 /models）一律忽略。
    #[tokio::test]
    async fn saving_non_model_provider_drops_health() {
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1"])]);
        app.config_dir = test_config_dir("save-non-model");
        app.focus = Focus::Providers;
        app.switch_tab();

        let mut form = crate::form::provider_add(ProviderKind::NonModel);
        form.fields[0] = crate::form::Field::text("ID", "deepl");
        form.fields[1] = crate::form::Field::text("名称", "DeepL");
        form.fields[2] = crate::form::Field::text("Base URL", "https://api.deepl.example/v2");
        app.save_provider_form(&form, None);

        let recipe = &app.recipes["deepl"];
        assert_eq!(recipe.kind, ProviderKind::NonModel);
        assert!(
            recipe.health.is_none(),
            "非模型厂商不探活：探活路径的值即使非空也要丢掉"
        );
        // 落盘也带上了类型（YAML 里写 kind: non_model）
        let yaml = std::fs::read_to_string(app.config_dir.join("recipes/deepl.yaml")).unwrap();
        assert!(yaml.contains("kind: non_model"), "{yaml}");
    }

    /// 类型只在创建时定：编辑时就算把勾选框翻过来，保存后类型也不变。
    #[tokio::test]
    async fn editing_never_changes_the_kind() {
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1"])]);
        app.config_dir = test_config_dir("edit-kind");
        app.focus = Focus::Providers;
        let mut form = crate::form::provider_edit(&app.recipes["alpha"].clone(), "/models");
        assert!(
            !form.toggle(crate::form::PF_NON_MODEL),
            "模型厂商只读显示不勾"
        );
        // 绕过只读直接改投保状态（模拟脏路径）：保存仍要沿用原类型
        form.fields[crate::form::PF_NON_MODEL] = crate::form::Field::toggle("非模型", true, "");
        app.save_provider_form(&form, Some("alpha"));
        assert_eq!(app.recipes["alpha"].kind, ProviderKind::Model);
    }

    /// 非当前分页的列表缩水时也要把它的选中项钳回来：切回去别变成「有厂商但没选中」。
    #[test]
    fn inactive_tab_selection_is_clamped_when_its_list_shrinks() {
        let (mut app, _rx, _rx_models) = test_app(&[
            ("alpha", &["a1"]),
            ("deepl", &["main"]),
            ("tavily", &["main"]),
        ]);
        for id in ["deepl", "tavily"] {
            app.recipes.get_mut(id).unwrap().kind = ProviderKind::NonModel;
        }
        app.rebuild_provider_list();
        app.switch_tab(); // 非模型页：[deepl, tavily]
        app.select_provider(1);
        app.switch_tab(); // 回模型页，非模型页仍记着选中 tavily

        // 在模型页把 tavily 删掉（模拟在另一页删厂商 / 撤销）
        app.recipes.remove("tavily");
        app.rebuild_provider_list();

        app.switch_tab(); // 切回非模型页
        assert_eq!(app.selected_provider(), 0, "选中下标应被钳回列表范围");
        assert_eq!(app.current_provider_id(), Some("deepl"));
    }

    /// 非模型厂商的服务对象不同：`m`（模型列表）直接提示，不开弹窗也不发请求。
    #[test]
    fn open_models_refuses_for_non_model_provider() {
        let mut app = two_kind_app();
        app.focus = Focus::Keys;
        app.switch_tab();
        app.open_models();
        assert!(matches!(app.modal, Modal::None), "非模型厂商不该开模型弹窗");
        assert_eq!(
            app.toast_text(),
            Some("deepl 假厂商 是非模型厂商，没有模型列表")
        );
    }

    /// AUTH 行（内置 openai 分页）是可选中的**最后一行**：j/k 能走上去，
    /// clamp 不会把它吃掉；别的厂商不多出这个位置。
    #[test]
    fn auth_row_is_the_last_selectable_row_on_openai() {
        let (mut app, _rx, _rx_models) = test_app(&[("openai", &["api-key"])]);
        app.focus = Focus::Keys;
        app.selected_key = 0;
        assert_eq!(app.auth_row_index(), Some(1));
        assert!(!app.auth_row_selected());
        assert!(app.selected_key_entry().is_some());

        app.move_down();
        assert_eq!(app.selected_key, 1);
        assert!(app.auth_row_selected(), "密钥行下面那一行就是 AUTH");
        assert!(app.selected_key_entry().is_none(), "AUTH 不是密钥");
        app.move_down(); // 已经到底
        assert_eq!(app.selected_key, 1);
        app.move_up();
        assert_eq!(app.selected_key, 0);

        // 别的厂商没有 AUTH 行，也就没有多出来的那一个位置
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1"])]);
        app.focus = Focus::Keys;
        assert_eq!(app.auth_row_index(), None);
        app.move_down();
        assert_eq!(app.selected_key, 0);
        assert!(!app.auth_row_selected());
    }

    /// openai 面板没有密钥时，AUTH 行就是唯一那一行，也是默认光标位。
    #[test]
    fn auth_row_is_the_only_row_when_openai_has_no_keys() {
        let (mut app, _rx, _rx_models) = test_app(&[("openai", &[])]);
        app.focus = Focus::Keys;
        assert_eq!(app.auth_row_index(), Some(0));
        assert!(app.auth_row_selected());
        assert!(app.selected_key_entry().is_none());
    }

    /// AUTH 行按 `x` 不是「导入密钥」：走 Codex 官方路那条。没有凭据时给的是登录提示，
    /// 而不是开一个要拉模型列表的面板。
    #[test]
    fn x_on_the_auth_row_imports_the_oauth_credential_instead_of_a_key() {
        let (mut app, _rx, _rx_models) = test_app(&[("openai", &["api-key"])]);
        app.config_dir = test_config_dir("auth-row-x");
        app.focus = Focus::Keys;
        app.selected_key = 1; // AUTH 行
        app.open_import();
        assert!(matches!(app.modal, Modal::None), "官方路不需要模型面板");
        assert_eq!(
            app.toast_text(),
            Some("还没有 OAuth 凭据：先按 o 完成 OpenAI Codex 授权")
        );

        // 密钥行照旧开面板（AUTH 的旁路不能把原来的导入改掉）
        app.selected_key = 0;
        app.open_import();
        assert!(matches!(app.modal, Modal::Import(_)));
    }

    /// AUTH 行上 c/i/d/m/e 不该装作「没有密钥」：给一句明确的提示（那一行只有 x 有含义）。
    #[test]
    fn key_actions_on_the_auth_row_say_what_is_supported() {
        let (mut app, _rx, _rx_models) = test_app(&[("openai", &["api-key"])]);
        app.focus = Focus::Keys;
        app.selected_key = 1;
        let actions: [fn(&mut App); 5] = [
            App::copy_selected,
            App::open_inspector,
            App::open_delete,
            App::open_edit,
            App::open_models,
        ];
        for action in actions {
            app.toast = None;
            action(&mut app);
            assert_eq!(
                app.toast_text(),
                Some("AUTH 行是 Codex OAuth：只支持 x（导入到 Codex）")
            );
            assert!(matches!(app.modal, Modal::None));
        }
    }

    /// 「codex 现在走哪条路」是回读现场算的：`refresh_active_keys` 后跟着 `~/.codex` 变。
    #[test]
    fn codex_route_is_read_back_from_the_client_config() {
        let (mut app, _rx, _rx_models) = test_app(&[("openai", &["api-key"])]);
        let home = app.codex_home();
        app.refresh_active_keys();
        assert_eq!(
            app.codex_route,
            crate::clients::codex::CodexRoute::SignedOut
        );

        std::fs::write(
            home.join("config.toml"),
            "model_provider = \"deepseek\"\n[model_providers.deepseek]\nbase_url = \"https://x.invalid/v1\"\n",
        )
        .unwrap();
        app.refresh_active_keys();
        assert_eq!(
            app.codex_route,
            crate::clients::codex::CodexRoute::Provider("deepseek".into())
        );

        std::fs::write(home.join("config.toml"), "model = \"gpt-6\"\n").unwrap();
        std::fs::write(
            home.join("auth.json"),
            r#"{"auth_mode":"chatgpt","tokens":{"access_token":"a.b.c"}}"#,
        )
        .unwrap();
        app.refresh_active_keys();
        assert_eq!(
            app.codex_route,
            crate::clients::codex::CodexRoute::OfficialOauth
        );
    }

    /// 内置 `openai` 不能复制（副本登不上它的 OAuth，也没 AUTH 行）：`y` 给明确提示，不落文件。
    #[test]
    fn duplicating_the_builtin_openai_is_refused() {
        let (mut app, _rx, _rx_models) = test_app(&[("openai", &["api-key"])]);
        app.config_dir = test_config_dir("dup-openai");
        app.focus = Focus::Providers;
        app.duplicate_selected_provider();
        let toast = app.toast_text().unwrap_or_default();
        assert!(toast.contains("复制失败"), "{toast}");
        assert!(toast.contains("不能复制"), "{toast}");
        assert!(!app.recipes.contains_key("openai-copy"));
    }

    /// AUTH 行也算在 clamp 的上限里：光标停在它上面不会被拽回最后一把密钥。
    #[test]
    fn clamp_selections_keeps_the_auth_row_selected() {
        let (mut app, _rx, _rx_models) = test_app(&[("openai", &["api-key"])]);
        app.focus = Focus::Keys;
        app.selected_key = 1;
        app.clamp_selections();
        assert_eq!(app.selected_key, 1, "AUTH 行（openai 分页）也是合法选中位");
        assert!(app.auth_row_selected());

        // 过滤到一把都没剩：AUTH 行仍然可选（它就是唯一那一行），光标不会被清掉
        app.key_filter = Some("没有这把".into());
        app.clamp_selections();
        assert_eq!(app.selected_key, 0);
        assert!(app.auth_row_selected());
    }

    /// 非模型厂商不能导入客户端（约定 15）：手改 openai 的 `kind` 后，AUTH 行按 `x` 也要拦住。
    #[test]
    fn x_on_the_auth_row_refuses_a_non_model_openai() {
        let (mut app, _rx, _rx_models) = test_app(&[("openai", &["api-key"])]);
        app.config_dir = test_config_dir("auth-row-non-model");
        // TUI 与 CLI 都不给改类型，只有手写 YAML 能走到这一步
        app.recipes.get_mut("openai").unwrap().kind = ProviderKind::NonModel;
        app.rebuild_provider_list();
        app.focus_provider("openai");
        app.focus = Focus::Keys;
        app.selected_key = app.auth_row_index().expect("非模型页也画 AUTH 行");
        assert!(app.auth_row_selected());

        app.open_import();
        assert!(
            matches!(app.modal, Modal::None),
            "非模型厂商不该走官方路导入"
        );
        assert_eq!(
            app.toast_text(),
            Some("openai 假厂商 是非模型厂商，不能导入到客户端")
        );
    }
}
