//! 文档守卫：仓库级的「文档 ↔ 代码」不变量，跑 `cargo test` 就会执行。
//!
//! **为什么是集成测试而不是 CI 脚本**：`tests/` 下的集成测试一次搭上三条既有执行路径 ——
//! 本地 `cargo test`、CI 的 `cargo test --all`、以及 MSRV job 的 `cargo check --locked
//! --all-targets`，不需要新增 CI 步骤。
//!
//! **怎么发现文件**：只走这几处**显式路径**，绝不整仓递归 —— 本机 `.delta/worktrees/` 下有整仓
//! 副本（含同名 `AGENTS.md` 与 `docs/**`），`target/` 里也有大量产物；整仓扫描会把它们当成仓库
//! 内容、给出假红。本文件碰的东西：`AGENTS.md`（读）、`src/`（递归）、仓库根（非递归读一层）。
//!
//! 三条不变量各一个 `#[test]`（独立失败），口径写在 `AGENTS.md` 的「目录结构」那段里。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// 三条不变量各自的名字：失败信息里要写清「违反哪条规则」。
const RULE_EXISTS: &str = "目录树里的路径必须真实存在";
const RULE_LISTED: &str = "src/ 下每个非测试源文件都要进目录树";
const RULE_TOP_LEVEL: &str = "仓库根下的非隐藏目录都要进目录树";

/// 仓库根。集成测试的 cwd 不保证是仓库根，取编译期常量（本仓已有同款先例）。
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 像路径的一个 token：以 ASCII 字母 / 数字 / `.` / `_` 开头，其余只含 `[A-Za-z0-9._/-]`。
///
/// 用来把「注解续行」和「注解里的中文」挡在条目之外。
fn is_path_token(token: &str) -> bool {
    let is_tail = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/');
    match token.chars().next() {
        Some(first) if first.is_ascii_alphanumeric() || first == '.' || first == '_' => {}
        _ => return false,
    }
    token.chars().all(is_tail)
}

/// 一行里所有**像路径的 token**。
///
/// 注解里的文件名常常粘着全角冒号（`一键导入面板：mod.rs`）或与别的词拼在一起，所以不能只按空白切：
/// 按「非路径字符」切，再把像路径的片段留下来。
fn path_tokens(line: &str) -> BTreeSet<String> {
    line.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/')))
        .filter(|token| !token.is_empty() && is_path_token(token))
        .map(str::to_owned)
        .collect()
}

/// 统一的失败信息：**哪个文件、哪条规则、怎么修**。
///
/// 守卫一半的价值在「能变红」，另一半在「红了修得动」：只丢一句 `assert` 失败会让人先去读守卫
/// 源码，那比不检查还贵。
fn problem(file: &str, rule: &str, detail: &str, fix: &str) -> String {
    format!("{file}：违反「{rule}」\n  {detail}\n  怎么修：{fix}")
}

/// 把一串问题收成一条 assert 消息（空 = 通过）。
fn report(problems: Vec<String>) -> String {
    problems.join("\n\n")
}

/// `AGENTS.md` 的目录结构：`## 目录结构` 之后**第一个**围栏代码块的内容。
fn directory_tree(agents: &str) -> String {
    let heading = agents
        .find("## 目录结构")
        .expect("AGENTS.md 里应有「## 目录结构」一节（守卫按它取树）");
    let after = &agents[heading..];
    let open = after.find("```").expect("目录结构一节里应有围栏代码块") + 3;
    let body = &after[open..];
    let close = body.find("```").expect("目录结构的围栏代码块应有收尾 ```");
    body[..close].to_string()
}

/// 树里的一行：**深度** + 路径**名字** + 去掉树形前缀后的整行（名字 + 注解）。
struct TreeEntry {
    depth: usize,
    name: String,
    line: String,
}

/// 树的每一行 → `TreeEntry`。
///
/// 名字 = 去掉树形前缀后的第一个空白分隔 token：树里的路径没有空格，所以这一个 token 就是路径名
/// （注解一律跟在它后面，可能只隔一个空格）。深度靠前缀算：每个 `│   ` / `    ` 缩进单元 +1，
/// `├── ` / `└── ` 再 +1；顶层条目（`src/`、`docs/`…）没有前缀，深度 0。
fn tree_entries(block: &str) -> Vec<TreeEntry> {
    let mut out = Vec::new();
    for line in block.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut rest = line;
        let mut depth = 0usize;
        while let Some(stripped) = rest
            .strip_prefix("│   ")
            .or_else(|| rest.strip_prefix("    "))
        {
            rest = stripped;
            depth += 1;
        }
        if let Some(stripped) = rest
            .strip_prefix("├── ")
            .or_else(|| rest.strip_prefix("└── "))
        {
            rest = stripped;
            depth += 1;
        }
        let Some(name) = rest.split_whitespace().next() else {
            continue;
        };
        // 树里有一行是**注解的续行**（例：`（tests/ 按 flow / apply 分）`）：它没有树形分支，
        // 首 token 也不是路径 —— 靠这条把它挡在条目之外。
        if !is_path_token(name) {
            continue;
        }
        out.push(TreeEntry {
            depth,
            name: name.to_string(),
            line: rest.to_string(),
        });
    }
    out
}

/// 树里提到的所有路径（相对仓库根），例：`src/app/import/flow.rs`、`docs/TODO.md`。
///
/// 栈里存的是**各层祖先的名字**（目录，均以 `/` 结尾），而不是它自己的完整路径 —— 后者会让
/// 同一层的第二个兄弟把上一个兄弟的路径拼进来（`src/src/ui/mod.rs`）。
fn tree_paths(entries: &[TreeEntry]) -> BTreeSet<String> {
    let mut dirs: Vec<String> = Vec::new();
    let mut paths = BTreeSet::new();
    for entry in entries {
        dirs.truncate(entry.depth);
        let full = format!("{}{}", dirs.concat(), entry.name);
        paths.insert(full);
        if entry.name.ends_with('/') {
            dirs.push(entry.name.clone());
        }
    }
    paths
}

/// 目录条目 → **它那一行上出现过的 token**（按空白与 `/` 切）。
///
/// 树的注解本来就会顺手点名自家文件（例：`update/` 那行写着「channel.rs 认渠道 / install_sh.rs …」），
/// 所以「被点名」既可以是独立的子条目，也可以写在父目录那一行里 —— 两种都算「这个模块进了索引」。
fn tree_dir_tokens(entries: &[TreeEntry]) -> std::collections::BTreeMap<String, BTreeSet<String>> {
    let mut dirs: Vec<String> = Vec::new();
    let mut out = std::collections::BTreeMap::new();
    for entry in entries {
        dirs.truncate(entry.depth);
        let full = format!("{}{}", dirs.concat(), entry.name);
        if entry.name.ends_with('/') {
            let tokens = path_tokens(&entry.line);
            out.insert(full.clone(), tokens);
            dirs.push(entry.name.clone());
        }
    }
    out
}

/// `src/` 下每个**非测试**源文件的路径（相对 `src/`），例：`app/import/flow.rs`。
///
/// 「测试」= 路径里有 `tests` 目录段，或文件名是 `tests.rs` —— 与「目录结构」那段写的覆盖口径
/// 一致：测试代码按目录归拢，不逐个进树。
fn src_source_files(src: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut todo = vec![src.to_path_buf()];
    while let Some(dir) = todo.pop() {
        let entries =
            fs::read_dir(&dir).unwrap_or_else(|err| panic!("读 {} 失败：{err}", dir.display()));
        for entry in entries {
            let path = entry.expect("读目录项").path();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if path.is_dir() {
                if name != "tests" {
                    todo.push(path);
                }
                continue;
            }
            if !name.ends_with(".rs") || name == "tests.rs" {
                continue;
            }
            let rel = path
                .strip_prefix(src)
                .expect("src/ 下的路径")
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel);
        }
    }
    out
}

/// 1 · 目录树里写下的每个路径都**真实存在**。
///
/// 防的是「文件搬走了 / 改名了 / 删了，树还写着」—— 那会让后来者按树去找一个不存在的文件。
#[test]
fn every_path_in_the_directory_tree_exists() {
    let root = repo_root();
    let agents = fs::read_to_string(root.join("AGENTS.md")).expect("读 AGENTS.md");
    let mut problems = Vec::new();
    for path in tree_paths(&tree_entries(&directory_tree(&agents))) {
        if !root.join(path.trim_end_matches('/')).exists() {
            problems.push(problem(
                "AGENTS.md",
                RULE_EXISTS,
                &format!("「目录结构」里的 `{path}` 在磁盘上不存在"),
                "从树里删掉它，或把名字改成磁盘上的真实路径",
            ));
        }
    }
    assert!(problems.is_empty(), "{}", report(problems));
}

/// 2 · `src/` 下每个非测试源文件都被树点名。
///
/// 防的是「新加了模块、树里没有」—— `clients/pi` 当初就是整块缺席的那一类。
#[test]
fn every_source_module_is_listed_in_the_directory_tree() {
    let root = repo_root();
    let agents = fs::read_to_string(root.join("AGENTS.md")).expect("读 AGENTS.md");
    let entries = tree_entries(&directory_tree(&agents));
    let listed = tree_paths(&entries);
    let dir_tokens = tree_dir_tokens(&entries);
    let mut problems = Vec::new();
    for rel in src_source_files(&root.join("src")) {
        let path = format!("src/{rel}");
        let (parent, base) = match path.rfind('/') {
            Some(at) => (&path[..=at], &path[at + 1..]),
            None => ("src/", path.as_str()),
        };
        // 两种「被点名」都算：独立的子条目，或写在父目录那一行的注解里
        if listed.contains(&path)
            || dir_tokens
                .get(parent)
                .is_some_and(|tokens| tokens.contains(base))
        {
            continue;
        }
        let fix = if dir_tokens.contains_key(parent) {
            format!("在 `{parent}` 那一行里点名 `{base}`，或给它补一行 `├── {base}   <一句话职责>`")
        } else {
            format!("父目录 `{parent}` 与它下面的文件都没进树：补上 `{parent}` 一行并点名 `{base}`")
        };
        problems.push(problem(
            "AGENTS.md",
            RULE_LISTED,
            &format!("「目录结构」没点名 `{path}`"),
            &fix,
        ));
    }
    assert!(problems.is_empty(), "{}", report(problems));
}

/// 3 · 仓库根下每个非隐藏目录（除 `target/`）都被树点名。
///
/// 防的是「新加了一个顶层目录、树里没有」—— 本票自己就撞上了：加了 `tests/`。
/// `.` 开头的目录是开发工具与本机状态（`.git` / `.agents` / `.scratch` / `.pi` / `.delta`…），
/// `target/` 是构建产物：两类都不进「源码布局」这张索引。
#[test]
fn every_top_level_directory_is_listed() {
    let root = repo_root();
    let agents = fs::read_to_string(root.join("AGENTS.md")).expect("读 AGENTS.md");
    let listed = tree_paths(&tree_entries(&directory_tree(&agents)));
    let mut problems = Vec::new();
    for entry in fs::read_dir(&root).expect("读仓库根") {
        let path = entry.expect("读目录项").path();
        if !path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if name.starts_with('.') || name == "target" {
            continue;
        }
        let named = format!("{name}/");
        if !listed.contains(&named) {
            problems.push(problem(
                "AGENTS.md",
                RULE_TOP_LEVEL,
                &format!("仓库根下有目录 `{named}`，「目录结构」没点名"),
                &format!("在树里补一行：`{named}   <一句话职责>`"),
            ));
        }
    }
    assert!(problems.is_empty(), "{}", report(problems));
}
