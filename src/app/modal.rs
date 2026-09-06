//! 弹窗生命周期：打开、分发保存/删除确认、取消。模型列表弹窗也在本文件。

use std::path::PathBuf;
use std::time::Instant;

use super::{App, Focus};
use crate::clipboard;
use crate::form::{self, Form, LineEdit};
use crate::probe;

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelsStatus {
    Loading,
    Done { items: Vec<String>, selected: usize },
    Error { message: String },
}

impl ModelsStatus {
    /// Done 状态下移动选中项（delta 正负皆可），两端钳制；非 Done 或空列表不动。
    pub fn move_selection(&mut self, delta: isize) {
        if let ModelsStatus::Done { items, selected } = self {
            let len = items.len() as isize;
            if len == 0 {
                return;
            }
            *selected = ((*selected as isize + delta).clamp(0, len - 1)) as usize;
        }
    }

    /// Done 状态下当前选中的模型名。
    pub fn selected_item(&self) -> Option<&str> {
        match self {
            ModelsStatus::Done { items, selected } => items.get(*selected).map(String::as_str),
            _ => None,
        }
    }
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
    Models {
        key_id: String,
        status: ModelsStatus,
    },
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
            | Modal::Search { .. } => {}
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
        };
        let client = self.client.clone();
        let tx = self.tx_models.clone();
        tokio::spawn(async move {
            let result = probe::fetch_models(&client, &recipe, &key.token).await;
            let _ = tx.send((key_id, result));
        });
    }

    /// 模型列表拉取结果落地：弹窗还开着且 key_id 匹配才更新，否则丢弃
    /// （弹窗可能已被关掉或换了把密钥打开）。
    pub fn apply_models(&mut self, key_id: String, result: Result<Vec<String>, String>) {
        let Modal::Models {
            key_id: modal_key_id,
            status,
        } = &mut self.modal
        else {
            return;
        };
        if *modal_key_id != key_id {
            return;
        }
        *status = match result {
            Ok(items) => ModelsStatus::Done { items, selected: 0 },
            Err(message) => ModelsStatus::Error { message },
        };
    }

    /// 模型弹窗内 j/k：移动选中项。
    pub fn move_models_selection(&mut self, delta: isize) {
        if let Modal::Models { status, .. } = &mut self.modal {
            status.move_selection(delta);
        }
    }

    /// 模型弹窗按 `c`：复制当前选中的模型名。
    pub fn copy_selected_model(&mut self) {
        let name = match &self.modal {
            Modal::Models { status, .. } => status.selected_item().map(str::to_string),
            _ => None,
        };
        let Some(name) = name else {
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
    use std::collections::{HashMap, HashSet};

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
        let (tx_models, _rx_models) = tokio::sync::mpsc::unbounded_channel();
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
            inflight: HashSet::new(),
            key_filter: None,
            provider_filter: None,
            tx,
            tx_models,
            client: crate::probe::client().expect("client"),
            next_auto_refresh: Instant::now() + std::time::Duration::from_secs(300),
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

    /// 断言当前是 Models 弹窗并拆出 (key_id, status)。
    fn models_modal(app: &App) -> (&str, &ModelsStatus) {
        match &app.modal {
            Modal::Models { key_id, status } => (key_id, status),
            other => panic!("期望 Models 弹窗，实际 {other:?}"),
        }
    }

    fn models_loading(key_id: &str) -> Modal {
        Modal::Models {
            key_id: key_id.into(),
            status: ModelsStatus::Loading,
        }
    }

    #[test]
    fn move_selection_clamps_at_both_ends() {
        let mut status = done(&["a", "b", "c"]);
        status.move_selection(1);
        assert_eq!(status.selected_item(), Some("b"));
        status.move_selection(10);
        assert_eq!(status.selected_item(), Some("c"));
        status.move_selection(-10);
        assert_eq!(status.selected_item(), Some("a"));
        status.move_selection(-1); // 已在顶端，再上不动
        assert_eq!(status.selected_item(), Some("a"));
    }

    #[test]
    fn move_selection_noop_when_empty_or_not_done() {
        let mut empty = ModelsStatus::Done {
            items: Vec::new(),
            selected: 0,
        };
        empty.move_selection(1);
        assert_eq!(empty.selected_item(), None);
        let mut loading = ModelsStatus::Loading;
        loading.move_selection(1);
        assert_eq!(loading, ModelsStatus::Loading);
    }

    #[test]
    fn apply_models_updates_matching_modal_and_drops_mismatch() {
        let (mut app, _rx, _rx_models) = crate::app::tests::test_app(&[("alpha", &["a1"])]);
        app.modal = models_loading("alpha.a1");
        // key_id 匹配：Loading → Done，选中从 0 开始
        app.apply_models("alpha.a1".into(), Ok(vec!["m1".into(), "m2".into()]));
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
        app.apply_models("alpha.a1".into(), Ok(vec!["m1".into()]));
        assert!(matches!(app.modal, Modal::None));
    }

    #[tokio::test]
    async fn open_models_sets_loading_and_delivers_result() {
        let (mut app, _rx, mut rx_models) = crate::app::tests::test_app(&[("alpha", &["a1"])]);
        app.focus = Focus::Keys;
        app.open_models();
        let (id, status) = models_modal(&app);
        assert_eq!(id, "alpha.a1");
        assert_eq!(status, &ModelsStatus::Loading);
        // spawn 出去的 fetch 打 example.invalid 必失败，结果经通道回来后落地 Error
        let (back_id, result) =
            tokio::time::timeout(std::time::Duration::from_secs(10), rx_models.recv())
                .await
                .expect("应收到模型拉取结果")
                .expect("channel 不应关闭");
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
