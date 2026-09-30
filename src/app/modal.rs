//! 弹窗生命周期：打开、分发保存/删除确认、取消。模型列表弹窗也在本文件。

use std::path::PathBuf;
use std::time::Instant;

use super::import::{ImportFlow, ImportStep};
use super::{App, Focus, TaskMsg};
use crate::clipboard;
use crate::form::{self, Form, LineEdit};
use crate::probe::{self, ModelEntry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    Key,
    Provider,
}

/// 检查器的目标：存 ID，数据在渲染时从 App 现查（自动刷新后内容同步更新）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorTarget {
    Provider(String),
    Key(String),
}

/// 模型列表弹窗的状态机：打开即 Loading，拉取结果落地转 Done/Error。
/// Done.items 恒为全量列表；selected 是「过滤后视图」里的下标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelsStatus {
    Loading,
    Done { items: Vec<String>, selected: usize },
    Error { message: String },
}

/// 模型名过滤：大小写不敏感的包含匹配。空关键字 = 全量。
pub fn filter_models(items: &[String], filter: &str) -> Vec<String> {
    if filter.is_empty() {
        return items.to_vec();
    }
    let needle = filter.to_lowercase();
    items
        .iter()
        .filter(|m| m.to_lowercase().contains(&needle))
        .cloned()
        .collect()
}

/// `/` 搜索弹窗的目标列表：按焦点决定过滤密钥还是厂商。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchTarget {
    Key,
    Provider,
}

#[derive(Debug)]
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
    /// 只读详情检查器（i 键打开）。reveal_token 只对 Key 目标有意义。
    Inspector {
        target: InspectorTarget,
        reveal_token: bool,
    },
    /// 模型列表浏览：迟到的拉取结果按 key_id 匹配，弹窗已关/已换密钥则丢弃。
    /// filter 为搜索关键字（`/` 聚焦输入，实时过滤），searching = 输入框聚焦中。
    Models {
        key_id: String,
        status: ModelsStatus,
        filter: String,
        searching: bool,
    },
    /// 一键导入到客户端（x 键打开）：选客户端 → 勾选模型 → 写入 → 校验。
    Import(ImportFlow),
    /// 实时过滤搜索：边输入边改 key_filter / provider_filter，
    /// original 记打开前的旧值，Esc 恢复。
    Search {
        target: SearchTarget,
        edit: LineEdit,
        original: Option<String>,
    },
}

impl App {
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
                let providers = self.provider_ids_filtered();
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
            Focus::Providers => self.open_edit_provider(),
            Focus::Keys => self.open_edit_key(),
        }
    }

    fn open_edit_provider(&mut self) {
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
        self.modal = Modal::Form {
            kind: FormKind::Provider,
            form: form::provider_edit(recipe, &health_path),
            original: Some(id),
        };
    }

    fn open_edit_key(&mut self) {
        let Some(key) = self.selected_key_entry().cloned() else {
            self.toast = Some(("没有可编辑的密钥".into(), Instant::now()));
            return;
        };
        let providers = self.provider_ids_filtered();
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

    /// `/`：按当前焦点打开搜索弹窗（密钥表 / 厂商栏），输入框预填现有过滤值。
    pub fn open_search(&mut self) {
        let (target, original) = match self.focus {
            Focus::Providers => (SearchTarget::Provider, self.provider_filter.clone()),
            Focus::Keys => (SearchTarget::Key, self.key_filter.clone()),
        };
        self.modal = Modal::Search {
            target,
            edit: LineEdit::new(original.clone().unwrap_or_default()),
            original,
        };
    }

    /// 输入过程中实时写回过滤值：弹窗背后的列表边输边变。空输入 = 无过滤。
    pub fn apply_live_filter(&mut self) {
        let Modal::Search { target, edit, .. } = &self.modal else {
            return;
        };
        let target = *target;
        let value = edit.value.trim().to_string();
        let value = (!value.is_empty()).then_some(value);
        match target {
            SearchTarget::Key => self.key_filter = value,
            SearchTarget::Provider => self.provider_filter = value,
        }
        self.clamp_selections();
    }

    /// Enter：应用当前输入并关闭弹窗（实时过滤已写回，这里只负责关）。
    pub fn apply_search(&mut self) {
        self.apply_live_filter();
        self.modal = Modal::None;
    }

    /// Esc：恢复打开前的过滤值并关闭弹窗。
    pub fn cancel_search(&mut self) {
        if let Modal::Search {
            target, original, ..
        } = &self.modal
        {
            let (target, original) = (*target, original.clone());
            match target {
                SearchTarget::Key => self.key_filter = original,
                SearchTarget::Provider => self.provider_filter = original,
            }
            self.clamp_selections();
        }
        self.modal = Modal::None;
    }

    /// 搜索弹窗打开时的粘贴入口（表单弹窗的粘贴走 form()）。
    pub fn search_paste(&mut self, text: &str) {
        if let Modal::Search { edit, .. } = &mut self.modal {
            edit.insert(text);
        } else {
            return;
        }
        self.apply_live_filter();
    }

    pub fn form(&mut self) -> Option<&mut Form> {
        match &mut self.modal {
            Modal::Form { form, .. } => Some(form),
            _ => None,
        }
    }

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

    pub(crate) fn set_form_error(&mut self, msg: &str) {
        if let Some(form) = self.form() {
            form.error = Some(msg.into());
        }
    }

    pub fn confirm_delete(&mut self) {
        match &self.modal {
            Modal::ConfirmDeleteKey { .. } => self.confirm_delete_key(),
            Modal::ConfirmDeleteProvider { .. } => self.confirm_delete_provider(),
            Modal::None
            | Modal::Form { .. }
            | Modal::Inspector { .. }
            | Modal::Models { .. }
            | Modal::Search { .. }
            | Modal::Import(_) => {}
        }
    }

    /// 焦点处按 i：打开对应详情检查器（厂商栏→当前厂商；密钥栏→选中密钥）。
    pub fn open_inspector(&mut self) {
        match self.focus {
            Focus::Providers => {
                let Some(id) = self.current_provider_id().map(String::from) else {
                    return;
                };
                self.modal = Modal::Inspector {
                    target: InspectorTarget::Provider(id),
                    reveal_token: false,
                };
            }
            Focus::Keys => {
                let Some(key) = self.selected_key_entry() else {
                    self.toast = Some(("没有可查看的密钥".into(), Instant::now()));
                    return;
                };
                self.modal = Modal::Inspector {
                    target: InspectorTarget::Key(key.id()),
                    reveal_token: false,
                };
            }
        }
    }

    /// 检查器里按 r：仅密钥详情且有 token 时在 遮掩/完整 间切换。
    pub fn inspector_toggle_reveal(&mut self) {
        let Modal::Inspector { target, .. } = &self.modal else {
            return;
        };
        let InspectorTarget::Key(id) = target else {
            return;
        };
        let has_token = self
            .keys
            .iter()
            .any(|k| &k.id() == id && !k.token.is_empty());
        if !has_token {
            return;
        }
        if let Modal::Inspector { reveal_token, .. } = &mut self.modal {
            *reveal_token = !*reveal_token;
        }
    }

    /// 检查器里按 c：复制完整 token（仅密钥详情）。toast 只带别名，不带密钥原文。
    pub fn inspector_copy_token(&mut self) {
        let Modal::Inspector { target, .. } = &self.modal else {
            return;
        };
        let InspectorTarget::Key(id) = target else {
            return;
        };
        let Some(key) = self.keys.iter().find(|k| &k.id() == id) else {
            return;
        };
        let alias = key.alias.clone();
        let token = key.token.clone();
        match clipboard::copy(&token) {
            Ok(()) => self.toast = Some((format!("已复制 {alias} 的完整密钥"), Instant::now())),
            Err(err) => self.toast = Some((format!("复制失败: {err}"), Instant::now())),
        }
    }

    // ---- 模型列表弹窗 ----------------------------------------------------

    /// 密钥栏按 `m`：用当前选中的这把 key 拉取它的模型列表。
    /// 模型可见性随 key（分组）不同而不同，所以按 key 而不是按厂商取第一把。
    pub fn open_models(&mut self) {
        let Some(key) = self.selected_key_entry().cloned() else {
            // 与 c/e/i/d 同款：没得选就提示，别让按键无声无息
            self.toast = Some(("没有可查询的密钥".into(), Instant::now()));
            return;
        };
        let Some(recipe) = self.recipes.get(&key.provider).cloned() else {
            return;
        };
        let key_id = key.id();
        self.modal = Modal::Models {
            key_id: key_id.clone(),
            status: ModelsStatus::Loading,
            filter: String::new(),
            searching: false,
        };
        let client = self.client.clone();
        let tx = self.tx_task.clone();
        tokio::spawn(async move {
            let result = probe::fetch_models(&client, &recipe, &key.token).await;
            let _ = tx.send(TaskMsg::Models(key_id, result));
        });
    }

    /// 模型列表拉取结果落地：一键导入面板优先（它打开时会顶掉浏览弹窗），
    /// 否则交给模型浏览弹窗；两边都按 key_id 匹配，不匹配就丢弃
    /// （弹窗可能已被关掉或换了把密钥打开）。
    pub fn apply_models(&mut self, key_id: String, result: Result<Vec<ModelEntry>, String>) {
        if self.import_awaiting(&key_id, ImportStep::Models) {
            self.import_receive_models(key_id, result);
            return;
        }
        let Modal::Models {
            key_id: modal_key_id,
            status,
            ..
        } = &mut self.modal
        else {
            return;
        };
        if *modal_key_id != key_id {
            return;
        }
        *status = match result {
            Ok(entries) => ModelsStatus::Done {
                items: entries.into_iter().map(|entry| entry.id).collect(),
                selected: 0,
            },
            Err(message) => ModelsStatus::Error { message },
        };
    }

    /// 模型弹窗是否处于搜索输入态。
    pub(crate) fn models_is_searching(&self) -> bool {
        matches!(
            &self.modal,
            Modal::Models {
                searching: true,
                ..
            }
        )
    }

    /// 当前过滤关键字（测试与渲染用）。
    #[cfg(test)]
    pub(crate) fn models_search_text(&self) -> &str {
        match &self.modal {
            Modal::Models { filter, .. } => filter,
            _ => "",
        }
    }

    /// `/`：进入搜索输入态（仅 Done 态有意义，其他态无操作）。
    pub fn models_start_search(&mut self) {
        if let Modal::Models { searching, .. } = &mut self.modal {
            *searching = true;
        }
    }

    /// 搜索输入一个字符：实时过滤，选中项回到过滤后列表顶部。
    pub fn models_search_char(&mut self, c: char) {
        if let Modal::Models {
            status: ModelsStatus::Done { selected, .. },
            filter,
            ..
        } = &mut self.modal
        {
            filter.push(c);
            *selected = 0;
        }
    }

    /// 搜索输入退格。
    pub fn models_search_backspace(&mut self) {
        if let Modal::Models {
            status: ModelsStatus::Done { selected, .. },
            filter,
            ..
        } = &mut self.modal
        {
            filter.pop();
            *selected = 0;
        }
    }

    /// 退出搜索输入态：过滤结果保留，j/k 继续在过滤后的列表里移动。
    pub fn models_exit_search(&mut self) {
        if let Modal::Models { searching, .. } = &mut self.modal {
            *searching = false;
        }
    }

    /// 模型弹窗内 j/k：移动选中项（按过滤后列表的长度钳制）。
    pub fn move_models_selection(&mut self, delta: isize) {
        if let Modal::Models {
            status: ModelsStatus::Done { items, selected },
            filter,
            ..
        } = &mut self.modal
        {
            let len = filter_models(items, filter).len() as isize;
            if len == 0 {
                return;
            }
            *selected = ((*selected as isize + delta).clamp(0, len - 1)) as usize;
        }
    }

    /// 模型弹窗按 `c`：复制当前选中的模型名（过滤后视图里选中的那个）。
    /// 过滤后当前选中的模型名。`copy_selected_model` 与单测共用，
    /// 让单测只验「选哪个」而不碰真实剪贴板（无显示环境会失败）。
    pub fn selected_model_name(&self) -> Option<String> {
        let (items, filter, selected) = match &self.modal {
            Modal::Models {
                status: ModelsStatus::Done { items, selected },
                filter,
                ..
            } => (items, filter, *selected),
            _ => return None,
        };
        filter_models(items, filter).get(selected).cloned()
    }

    pub fn copy_selected_model(&mut self) {
        let Some(name) = self.selected_model_name() else {
            return;
        };
        match clipboard::copy(&name) {
            Ok(()) => self.toast = Some((format!("已复制 {name}"), Instant::now())),
            Err(err) => self.toast = Some((format!("复制失败: {err}"), Instant::now())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::KeyEntry;
    use crate::recipe::Recipe;
    use std::collections::HashMap;

    fn one_key_app() -> App {
        let mut recipes = HashMap::new();
        recipes.insert(
            "p".to_string(),
            Recipe {
                id: "p".into(),
                name: "P".into(),
                base_url: "https://p.example".into(),
                homepage: None,
                models_url: None,
                supports_groups: false,
                vars: HashMap::new(),
                auth: Default::default(),
                health: None,
                balance: None,
                origin: None,
            },
        );
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let (tx_task, _rx_task) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App {
            recipes,
            keys: vec![KeyEntry {
                provider: "p".into(),
                alias: "main".into(),
                group: None,
                token: "sk-full-token-value".into(),
            }],
            provider_ids: Vec::new(),
            selected_provider: 0,
            selected_key: 0,
            focus: Focus::Keys,
            states: HashMap::new(),
            toast: None,
            last_refresh: None,
            modal: Modal::None,
            probe_seq: HashMap::new(),
            next_probe_seq: 0,
            key_filter: None,
            provider_filter: None,
            tx,
            tx_task,
            client: crate::probe::client().expect("client"),
            next_auto_refresh: Instant::now() + std::time::Duration::from_secs(300),
            config_dir: crate::app::tests::test_config_dir("modal"),
            codex: None,
            restart_codex_daemon: false,
            undo_stack: std::collections::VecDeque::new(),
        };
        app.rebuild_provider_list();
        app
    }

    #[test]
    fn inspector_reveal_resets_on_reopen_and_provider_target_ignores_r() {
        let mut app = one_key_app();

        // 打开 → 显隐 → 关闭 → 再打开：reveal 必须回落为默认遮掩
        app.focus = Focus::Keys;
        app.open_inspector();
        app.inspector_toggle_reveal();
        assert!(matches!(
            &app.modal,
            Modal::Inspector {
                reveal_token: true,
                ..
            }
        ));
        app.cancel_modal();
        app.open_inspector();
        assert!(
            matches!(
                &app.modal,
                Modal::Inspector {
                    reveal_token: false,
                    ..
                }
            ),
            "重开检查器必须回到遮掩态"
        );

        // Provider 目标按 r 是无操作（不切换、不 panic）
        app.cancel_modal();
        app.focus = Focus::Providers;
        app.open_inspector();
        app.inspector_toggle_reveal();
        assert!(matches!(
            &app.modal,
            Modal::Inspector {
                reveal_token: false,
                ..
            }
        ));
    }

    #[test]
    fn inspector_copy_requires_key_target() {
        let mut app = one_key_app();
        // Provider 目标按 c：无操作（不复制、无 toast）
        app.focus = Focus::Providers;
        app.open_inspector();
        app.inspector_copy_token();
        assert!(app.toast.is_none(), "厂商详情按 c 不应触发复制");
    }

    fn done(items: &[&str]) -> ModelsStatus {
        ModelsStatus::Done {
            items: items.iter().map(|s| (*s).to_string()).collect(),
            selected: 0,
        }
    }

    /// 测试里只关心模型名时用这个造条目（不声明端点能力）。
    fn entries(names: &[&str]) -> Vec<ModelEntry> {
        names
            .iter()
            .map(|name| ModelEntry {
                id: (*name).to_string(),
                responses: None,
            })
            .collect()
    }

    /// 断言当前是 Models 弹窗并拆出 (key_id, status)。
    fn models_modal(app: &App) -> (&str, &ModelsStatus) {
        match &app.modal {
            Modal::Models { key_id, status, .. } => (key_id, status),
            other => panic!("期望 Models 弹窗，实际 {other:?}"),
        }
    }

    /// 过滤后视图里当前选中的模型名。
    fn models_selected(app: &App) -> Option<String> {
        match &app.modal {
            Modal::Models {
                status: ModelsStatus::Done { items, selected },
                filter,
                ..
            } => filter_models(items, filter).get(*selected).cloned(),
            _ => None,
        }
    }

    /// 过滤后视图的完整列表。
    fn models_visible(app: &App) -> Vec<String> {
        match &app.modal {
            Modal::Models {
                status: ModelsStatus::Done { items, .. },
                filter,
                ..
            } => filter_models(items, filter),
            _ => Vec::new(),
        }
    }

    fn models_loading(key_id: &str) -> Modal {
        Modal::Models {
            key_id: key_id.into(),
            status: ModelsStatus::Loading,
            filter: String::new(),
            searching: false,
        }
    }

    #[test]
    fn models_search_filters_and_esc_keeps_filter() {
        let (mut app, _rx, _rx_models) = crate::app::tests::test_app(&[("alpha", &["a1"])]);
        app.modal = models_loading("alpha.a1");
        app.apply_models(
            "alpha.a1".into(),
            Ok(entries(&["claude-4", "gpt-5", "Claude-3"])),
        );
        assert!(!app.models_is_searching());

        // / 进入搜索态，输入实时过滤（大小写不敏感），选中回到顶部
        app.models_start_search();
        assert!(app.models_is_searching());
        app.models_search_char('c');
        app.models_search_char('L');
        assert_eq!(app.models_search_text(), "cL");
        // items 恒为全量，过滤是视图层
        let (_, status) = models_modal(&app);
        let (items, selected) = match status {
            ModelsStatus::Done { items, selected } => (items, *selected),
            other => panic!("应为 Done: {other:?}"),
        };
        assert_eq!(
            filter_models(items, app.models_search_text()),
            vec!["claude-4", "Claude-3"],
            "输入应实时过滤"
        );
        assert_eq!(selected, 0);

        // j 在过滤后的列表里移动
        app.move_models_selection(1);
        assert_eq!(models_selected(&app).as_deref(), Some("Claude-3"));

        // Esc 退出聚焦但保留过滤；再按 j 仍在过滤列表内移动
        app.models_exit_search();
        assert!(!app.models_is_searching());
        app.move_models_selection(-1);
        assert_eq!(models_selected(&app).as_deref(), Some("claude-4"));

        // 重新 / 聚焦可继续编辑；退格清空后过滤恢复全量
        app.models_start_search();
        app.models_search_backspace();
        assert_eq!(app.models_search_text(), "c");
        app.models_search_backspace();
        assert_eq!(app.models_search_text(), "");
        assert_eq!(
            models_visible(&app),
            vec!["claude-4", "gpt-5", "Claude-3"],
            "清空过滤恢复全量"
        );
        assert_eq!(models_selected(&app).as_deref(), Some("claude-4"));
    }

    #[test]
    fn models_copy_uses_filtered_selection() {
        let (mut app, _rx, _rx_models) = crate::app::tests::test_app(&[("alpha", &["a1"])]);
        app.modal = models_loading("alpha.a1");
        app.apply_models("alpha.a1".into(), Ok(entries(&["claude-4", "gpt-5"])));
        app.models_start_search();
        app.models_search_char('g');
        // 只断言「选中哪一个」，不碰真实剪贴板（CI / Linux 无显示环境会失败）
        assert_eq!(
            app.selected_model_name().as_deref(),
            Some("gpt-5"),
            "复制的是过滤后选中的模型"
        );
    }

    #[test]
    fn apply_models_updates_matching_modal_and_drops_mismatch() {
        let (mut app, _rx, _rx_models) = crate::app::tests::test_app(&[("alpha", &["a1"])]);
        app.modal = models_loading("alpha.a1");
        // key_id 匹配：Loading → Done，选中从 0 开始
        app.apply_models("alpha.a1".into(), Ok(entries(&["m1", "m2"])));
        let (id, status) = models_modal(&app);
        assert_eq!(id, "alpha.a1");
        assert_eq!(status, &done(&["m1", "m2"]));
        // key_id 不匹配（迟到结果）：丢弃，状态不变
        app.apply_models("alpha.b2".into(), Err("late".into()));
        assert_eq!(models_modal(&app).1, &done(&["m1", "m2"]));
        // 匹配的错误结果：转 Error
        app.apply_models("alpha.a1".into(), Err("HTTP 401".into()));
        assert!(
            matches!(models_modal(&app).1, ModelsStatus::Error { .. }),
            "错误结果应落地 Error"
        );
        // 弹窗已关：结果丢弃，不复活
        app.modal = Modal::None;
        app.apply_models("alpha.a1".into(), Ok(entries(&["m1"])));
        assert!(matches!(app.modal, Modal::None));
    }

    #[tokio::test]
    async fn open_models_sets_loading_and_delivers_result() {
        let (mut app, _rx, mut rx_task) = crate::app::tests::test_app(&[("alpha", &["a1"])]);
        app.focus = Focus::Keys;
        app.open_models();
        let (id, status) = models_modal(&app);
        assert_eq!(id, "alpha.a1");
        assert_eq!(status, &ModelsStatus::Loading);
        // spawn 出去的 fetch 打 example.invalid 必失败，结果经通道回来后落地 Error
        let msg = tokio::time::timeout(std::time::Duration::from_secs(10), rx_task.recv())
            .await
            .expect("应收到模型拉取结果")
            .expect("channel 不应关闭");
        let TaskMsg::Models(back_id, result) = msg else {
            panic!("本该是模型列表消息");
        };
        assert_eq!(back_id, "alpha.a1");
        assert!(result.is_err(), "假厂商拉模型应失败: {result:?}");
        app.apply_models(back_id, result);
        assert!(
            matches!(models_modal(&app).1, ModelsStatus::Error { .. }),
            "错误结果应落地 Error"
        );
    }

    #[test]
    fn open_models_without_key_toasts() {
        // 厂商没有密钥时密钥栏无行可选：不开弹窗，但与 c/e/i/d 同样给出提示
        let (mut app, _rx, _rx_models) = crate::app::tests::test_app(&[("alpha", &[])]);
        app.focus = Focus::Keys;
        app.open_models();
        assert!(matches!(app.modal, Modal::None), "没有密钥不应开弹窗");
        assert!(app.toast.is_some(), "没有密钥应提示");
    }
}
