//! 真正开写与回执落地：请求经 `crate::clients` 适配层交给所选客户端，
//! 面板本身不认识任何客户端细节（写哪里、怎么校验、怎么重载都由 `Agent` 决定）。

use std::time::Instant;

use crate::app::{App, Modal, TaskMsg};
use crate::clients::{Agent, ImportReport, ImportRequest, RestartReport};

use super::ImportStep;

/// 面板 → 客户端适配层的调用点。
///
/// 生产环境为 `None`（直接走 `Agent::import`）；测试替换它，用来断言
/// 「请求确实交给了所选的那个客户端」，而不用真去写 `~/.codex` / `~/.pi` 或起客户端进程。
pub(crate) type ImportRunner =
    std::sync::Arc<dyn Fn(Agent, &ImportRequest) -> Result<ImportReport, String> + Send + Sync>;

/// 后台导入任务的结果回执。
#[derive(Debug)]
pub struct ImportOutcome {
    /// 这次导入交给哪个客户端（提示文案按它来；★ 则回读客户端现场得出）。
    pub agent: Agent,
    pub key_id: String,
    /// 发起这次导入时面板的代际号（面板换了一代就不再关它、只落一条提示）。
    pub seq: u64,
    pub result: Result<ImportReport, String>,
    /// 让客户端重新加载的结果（`killed=0` 表示当时没在跑或已被禁用）。
    pub restart: RestartReport,
}

impl App {
    /// 真正开写：后台线程跑客户端导入 + 校验（codex 那一路还要重启它的 daemon）。
    pub(super) fn start_import(&mut self, models: Vec<String>, default_model: String) {
        let Some(flow) = self.import_flow() else {
            return;
        };
        let key_id = flow.key_id.clone();

        let Some(key) = self.keys.iter().find(|key| key.id() == key_id).cloned() else {
            self.toast = Some(("这把密钥已不存在".into(), Instant::now()));
            self.modal = Modal::None;
            return;
        };
        let Some(recipe) = self.recipes.get(&key.provider).cloned() else {
            self.toast = Some((
                format!("厂商 {} 的协议不存在", key.provider),
                Instant::now(),
            ));
            self.modal = Modal::None;
            return;
        };
        if let Some(flow) = self.import_flow_mut() {
            flow.step = ImportStep::Working;
            flow.error = None;
        }

        let request = ImportRequest {
            provider_id: recipe.id.clone(),
            provider_name: recipe.name.clone(),
            base_url: recipe.base_url.clone(),
            api_key: key.token.clone(),
            models,
            default_model,
        };
        let agent = self
            .import_flow()
            .map(|flow| flow.agent)
            .unwrap_or(Agent::Codex);
        let fallback_key_id = key_id.clone();
        let import_seq = self.import_flow().map(|flow| flow.seq).unwrap_or_default();
        let restart_daemon = self.restart_codex_daemon;
        let runner = self.import_runner.clone();
        let tx = self.tx_task.clone();
        tokio::spawn(async move {
            let outcome = tokio::task::spawn_blocking(move || {
                // 所有写盘动作都经客户端适配层：面板不认识任何客户端细节
                let result = match &runner {
                    Some(runner) => runner(agent, &request),
                    None => agent.import(&request),
                };
                // 写成功才重新加载：客户端只在进程启动时读一次配置
                let restart = if result.is_ok() && restart_daemon && agent.needs_reload() {
                    agent.reload().unwrap_or_default()
                } else {
                    RestartReport::default()
                };
                ImportOutcome {
                    agent,
                    key_id,
                    seq: import_seq,
                    result,
                    restart,
                }
            })
            .await
            .unwrap_or_else(|err| ImportOutcome {
                agent,
                key_id: fallback_key_id,
                seq: import_seq,
                result: Err(format!("导入任务异常终止：{err}")),
                restart: RestartReport::default(),
            });
            let _ = tx.send(TaskMsg::Import(Box::new(outcome)));
        });
    }

    /// 导入结果落地：成功就给成功反馈并关面板，失败留下面板显示原因。
    pub fn import_result(&mut self, outcome: ImportOutcome) {
        // 代际也要对上：面板关掉再开、对同一把密钥重来时，旧回执不该关掉新面板
        let waiting = self
            .import_flow()
            .is_some_and(|flow| flow.key_id == outcome.key_id && flow.seq == outcome.seq);
        match outcome.result {
            Ok(report) => {
                // ★ 回读客户端现场得出（不存 apim 侧的台账）：这里刚写完配置，重算一定准
                self.refresh_active_keys();
                let mut note = format!(
                    "已导入 {}：{} · {} 个模型 · 默认 {} · 表名 {}",
                    outcome.agent.label(),
                    outcome.key_id,
                    report.models.len(),
                    report.model,
                    report.provider_key
                );
                // 客户端自己报的补充（codex 报思考强度，pi 没有）：面板不猜，客户端才知道自己写了什么
                if let Some(detail) = &report.detail {
                    note.push_str(&format!(" · {detail}"));
                }
                if !report.backups.is_empty() {
                    let names: Vec<String> =
                        report.backups.iter().map(|path| file_name(path)).collect();
                    note.push_str(&format!(" · 旧配置备份为 {}", names.join(" / ")));
                }
                // 客户端的配置只在进程启动时读一次，必须重新加载才能让它的选择器刷新
                if outcome.restart.killed > 0 {
                    note.push_str(&format!(
                        " · 已重启 {} 守护进程({} 个)，重开它即可看到新模型",
                        outcome.agent.label(),
                        outcome.restart.killed
                    ));
                } else if outcome.restart.ambiguous > 0 {
                    // 形态不标准的没敢自动杀：说清楚，让他自己重启
                    note.push_str(&format!(
                        " · 没找到标准形态的 {} 守护进程（有 {} 个类似进程没敢动），若它正开着请手动重启",
                        outcome.agent.label(),
                        outcome.restart.ambiguous
                    ));
                } else {
                    note.push_str(&format!(" · {}", outcome.agent.reload_hint()));
                }
                self.toast = Some((note, Instant::now()));
                if waiting {
                    self.modal = Modal::None;
                }
            }
            Err(message) => {
                if waiting {
                    if let Some(flow) = self.import_flow_mut() {
                        flow.step = ImportStep::Failed;
                        flow.error = Some(message);
                    }
                } else {
                    self.toast = Some((format!("导入失败：{message}"), Instant::now()));
                }
            }
        }
    }
}

/// 取路径的文件名（提示条上只显示文件名，别撑爆一行）。
fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("备份")
        .to_string()
}
