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
//! 四条不变量各一个 `#[test]`（独立失败）：前三条的口径写在 `AGENTS.md` 的「目录结构」那段里，
//! 第四条（体量 ratchet）的口径写在它的两个常量上。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// 四条不变量各自的名字：失败信息里要写清「违反哪条规则」。
const RULE_EXISTS: &str = "目录树里的路径必须真实存在";
const RULE_LISTED: &str = "src/ 下每个非测试源文件都要进目录树";
const RULE_TOP_LEVEL: &str = "仓库根下的非隐藏目录都要进目录树";
const RULE_BUDGET: &str = "AGENTS.md 的体量只允许变小或保持（ratchet）";
const RULE_CLI_SURFACE: &str = "apim help 里的每个子命令都要在 README 里出现";
const RULE_SKILL_NO_SURFACE: &str = "SKILL 不定义命令面（只允许引用 README 已写的）";
const RULE_BANNED_COMMAND: &str = "那条安装禁令只许待在它的家里";
/// 那条禁令的字面串（`R4` 只认它，写法与 `AGENTS.md` 约定 10 一致）。
const BANNED_COMMAND: &str = "cargo install --path .";

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

/// AGENTS.md 的行数上限（ratchet：**只允许变小或保持**）。
///
/// 它每次运行都进上下文，所以这是唯一一条「持续变紧」的不变量。这个数不是拿来好看的：想让
/// AGENTS.md 长大，就得先回答「删掉什么」—— 或者把细节挪进它自己的家（Doc map 里那些）留一行指针。
const AGENTS_LINES_MAX: usize = 181;

/// AGENTS.md 的字符数上限（ratchet）。
///
/// 行数单独守不住：把一段长文拆成十行反而更容易「看起来没变多」，而**真正花掉的是字符 / token**。
/// 所以两个一起钉。
const AGENTS_CHARS_MAX: usize = 12508;

/// 4 · AGENTS.md 的体量只允许变小或保持（ratchet）。
///
/// 红了不要「把上限调大」—— 那等于把 ratchet 变成橡皮筋。要么删掉等量的旧内容，要么把细节搬到它的
/// 家（`docs/clients/*.md`、`docs/provider-kinds.md`、`docs/adr/`…）之后留一行指针。
#[test]
fn agents_md_stays_within_its_budget() {
    let agents = fs::read_to_string(repo_root().join("AGENTS.md")).expect("读 AGENTS.md");
    let lines = agents.lines().count();
    let chars = agents.chars().count();
    let mut problems = Vec::new();
    if lines > AGENTS_LINES_MAX {
        let over = lines - AGENTS_LINES_MAX;
        problems.push(problem(
            "AGENTS.md",
            RULE_BUDGET,
            &format!("行数 {lines} 超过上限 {AGENTS_LINES_MAX}（多了 {over} 行）"),
            "删掉等量的旧内容，或把细节挪进它的家后留一行指针；**不要**直接调大上限",
        ));
    }
    if chars > AGENTS_CHARS_MAX {
        let over = chars - AGENTS_CHARS_MAX;
        problems.push(problem(
            "AGENTS.md",
            RULE_BUDGET,
            &format!("字符数 {chars} 超过上限 {AGENTS_CHARS_MAX}（多了 {over} 字符）"),
            "同上；注意把一段长文拆成多行并不会省字符 —— 字符才是常驻上下文的成本",
        ));
    }
    assert!(problems.is_empty(), "{}", report(problems));
}

/// 一段文本里出现的「`apim` 后面那个词」—— 命令面的 token。
///
/// 语法驱动的解析：`apim` + 空格之后，取连续的 `[a-z0-9-]` 前缀（`--version` 这类开关也算，
/// 它同样要能在 README 里查到）；`apim —`（破折号）那种自然取到空串、被丢掉。
fn apim_tokens(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (at, _) in text.match_indices("apim ") {
        let token: String = text[at + 5..]
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
            .collect();
        if !token.is_empty() {
            out.insert(token);
        }
    }
    out
}

/// 别名 → 规范名。守卫按**规范名**要求 README：别名只是另一种拼法，重复要求它等于重复一条
/// 规矩（`apim help` 末尾的「别名」那一行也印证了这层关系）。
fn canonical_command(token: &str) -> &str {
    match token {
        "keys" => "key",
        "upgrade" => "update",
        other => other,
    }
}

/// 跑真二进制的 `apim help`，取它的 stdout。
///
/// `CARGO_BIN_EXE_apim` 是 cargo 给集成测试的编译期常量（刚编出来的那个二进制），所以这条
/// 守卫测的是**外部行为**，不读源码文本、不依赖内部函数。
fn apim_help() -> String {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_apim"))
        .arg("help")
        .output()
        .expect("跑 apim help（守卫按它的 stdout 取命令面）");
    assert!(
        out.status.success(),
        "apim help 退出码非 0：{:?}",
        out.status
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// 5 · `apim help` 里的命令面都在两份 README 里出现过。
///
/// 命令面的**唯一事实来源是代码**（`apim help`），README 是人读的那一份；这条防的是「加了
/// 子命令、README 没跟上」。断言只比 **token 集合**（别名并进规范名），不比对措辞、行号或
/// 段落顺序；也**不按小节标题定位**哪些行算数 —— `README.zh-CN.md` 的标题是译过的，按标题
/// 定位等于把守卫焊死在中文字符串上（改个标题就假红）。
#[test]
fn the_cli_surface_is_documented_in_the_readme() {
    let root = repo_root();
    let required: BTreeSet<String> = apim_tokens(&apim_help())
        .iter()
        .map(|token| canonical_command(token).to_string())
        .collect();
    let mut problems = Vec::new();
    for file in ["README.md", "README.zh-CN.md"] {
        let readme = fs::read_to_string(root.join(file))
            .unwrap_or_else(|err| panic!("读 {file} 失败：{err}"));
        let documented: BTreeSet<String> = apim_tokens(&readme)
            .iter()
            .map(|token| canonical_command(token).to_string())
            .collect();
        for token in &required {
            if !documented.contains(token) {
                problems.push(problem(
                    file,
                    RULE_CLI_SURFACE,
                    &format!("`apim help` 里有子命令 `{token}`，{file} 里一次都没出现"),
                    &format!("在 README 的 CLI 一节补一行 `apim {token} ...`（两版同步，约定 7）"),
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", report(problems));
}

/// 把反斜杠续行接起来：shell 里一条命令可以拆成多行，而「这一行有没有调 `apim`」要按**逻辑行**判。
fn join_continuations(text: &str) -> String {
    text.replace("\\\n", " ")
}

/// 一行里的所有 `--flag`。
fn flags_in(line: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (at, _) in line.match_indices("--") {
        let flag: String = line[at..]
            .chars()
            .take_while(|c| *c == '-' || c.is_ascii_lowercase() || c.is_ascii_digit())
            .collect();
        if flag.len() > 2 {
            out.insert(flag);
        }
    }
    out
}

/// 「调了 `apim` 的行」：逻辑行里出现 `apim <词>`。只有这种行上的动词与开关归 R2 管 ——
/// 别把整份 SKILL 的开关都拿来比：里面还有 curl（`--connect-timeout` / `--max-time`）、
/// cargo（`--all-targets` / `--ignored`）与 codex（`--profile`）的开关，它们不是 apim 的命令面。
fn apim_call_lines(text: &str) -> Vec<String> {
    join_continuations(text)
        .lines()
        .filter(|line| !apim_tokens(line).is_empty())
        .map(str::to_owned)
        .collect()
}

/// 6 · SKILL 不定义命令面（`R2`）。
///
/// **边界（重要）**：子集检查只能抓「发明了 README 里没有的命令 / 开关」与「两份说法互相
/// 矛盾」，**抓不到遗漏** —— SKILL 少写了一整个 `apim auth` 不会让这条红（那是「删掉第二份」
/// 要解决的问题，不是集合关系能发现的）。所以这条的定位是**兜底**：SKILL 里允许出现示例命令，
/// 但不许出现 README 没有的动词 / 开关。
#[test]
fn the_skill_does_not_redefine_the_cli_surface() {
    let root = repo_root();
    let skill = fs::read_to_string(root.join(".agents/skills/apim/SKILL.md")).expect("读 SKILL.md");
    let readme = fs::read_to_string(root.join("README.md")).expect("读 README.md");
    let mut problems = Vec::new();

    // R2b：必须留一条指向 README CLI 一节的指针（读 skill 的人要被送过去）
    let has_pointer = skill
        .lines()
        .any(|line| line.contains("README.md") && line.contains("CLI"));
    if !has_pointer {
        problems.push(problem(
            ".agents/skills/apim/SKILL.md",
            RULE_SKILL_NO_SURFACE,
            "没有指向 `README.md` CLI 一节的指针（命令面由此不再有第二个家）",
            "在 SKILL 顶部补一句：CLI 的子命令与开关以 `README.md` 的 *CLI* 一节为准",
        ));
    }

    // R2a：调用行上的动词与开关 ⊆ README 调用行上的
    let readme_lines = apim_call_lines(&readme);
    let readme_verbs: BTreeSet<String> = readme_lines
        .iter()
        .flat_map(|line| apim_tokens(line))
        .map(|token| canonical_command(&token).to_string())
        .collect();
    let readme_flags: BTreeSet<String> = readme_lines
        .iter()
        .flat_map(|line| flags_in(line))
        .collect();
    for line in apim_call_lines(&skill) {
        for verb in apim_tokens(&line) {
            let verb = canonical_command(&verb).to_string();
            let short = snippet_around(&line, &verb);
            if !readme_verbs.contains(&verb) {
                problems.push(problem(
                    ".agents/skills/apim/SKILL.md",
                    RULE_SKILL_NO_SURFACE,
                    &format!("`{short}` 用了 README 里没有的子命令 `{verb}`"),
                    &format!("要么删掉这条示例，要么先在 README 的 CLI 一节把 `apim {verb}` 写上"),
                ));
            }
        }
        for flag in flags_in(&line) {
            let short = snippet_around(&line, &flag);
            if !readme_flags.contains(&flag) {
                problems.push(problem(
                    ".agents/skills/apim/SKILL.md",
                    RULE_SKILL_NO_SURFACE,
                    &format!("`{short}` 用了 README 里没有的开关 `{flag}`"),
                    "要么删掉这条示例，要么先在 README 的 CLI 一节写上这个开关",
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", report(problems));
}

/// R4 的白名单：文件（相对仓库根）→ 允许出现几次 + 为什么。
///
/// 没在这里的文件**一次都不许出现** —— 它是「本机装了一份」的环境事实，写在别处就是抄一份会
/// 过期的副本（SKILL 就这么错过一版：把它当成「跑本地代码」的说明教了一遍）。
fn banned_command_allowance(path: &str) -> Option<(usize, &'static str)> {
    match path {
        "README.md" | "README.zh-CN.md" => {
            Some((usize::MAX, "用户手册：「从源码安装」那一步要用它"))
        }
        "AGENTS.md" => Some((1, "约定 10 就是那条禁令（只许一处）")),
        "docs/RELEASING.md" => Some((
            1,
            "渠道速查表要点名「源码」这个渠道，用来说明它**不在** apim update 覆盖范围内",
        )),
        _ => None,
    }
}

/// 长期文档的 `.md` 清单（相对仓库根）。
///
/// **显式根目录**，不整仓递归：仓库根（非递归）、`docs/`、`.agents/`。
/// **刻意不含 `.scratch/`** —— 那是进行中的工作区，spec / ticket 本来就要讨论这条禁令
/// （本特性的 spec 里就写着它），拿它当“文档”去卡只会自伤。
fn durable_markdown_files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut roots = vec![
        (root.to_path_buf(), false),
        (root.join("docs"), true),
        (root.join(".agents"), true),
    ];
    for (dir, recursive) in roots.drain(..) {
        let mut todo = vec![dir.clone()];
        while let Some(current) = todo.pop() {
            let Ok(entries) = fs::read_dir(&current) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if recursive {
                        todo.push(path);
                    }
                    continue;
                }
                if path.extension().is_some_and(|ext| ext == "md") {
                    out.push(
                        path.strip_prefix(root)
                            .expect("仓库内的路径")
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
    }
    out.sort();
    out
}

/// 7 · 那条安装禁令只许待在它的家里（`R4`）。
#[test]
fn the_dev_install_command_stays_in_its_own_home() {
    let root = repo_root();
    let mut problems = Vec::new();
    for file in durable_markdown_files(&root) {
        let text = fs::read_to_string(root.join(&file))
            .unwrap_or_else(|err| panic!("读 {file} 失败：{err}"));
        let found = text.matches(BANNED_COMMAND).count();
        if found == 0 {
            continue;
        }
        match banned_command_allowance(&file) {
            Some((allowed, why)) if found <= allowed => {
                let _ = why; // 白名单带理由是为了下一个人读得懂，不是为了运行时输出
            }
            Some((allowed, why)) => problems.push(problem(
                &file,
                RULE_BANNED_COMMAND,
                &format!("出现 {found} 次，白名单只允许 {allowed} 次（{why}）"),
                "删掉多余的那几处，改成指向 README 的安装一节或 AGENTS.md 约定 10",
            )),
            None => problems.push(problem(
                &file,
                RULE_BANNED_COMMAND,
                &format!("出现了 `{BANNED_COMMAND}`，但这里不是它的家"),
                "删掉它（要说「怎么跑本地代码」就指向 AGENTS.md 约定 10；要说「这个渠道不自动更新」就放进 docs/RELEASING.md 的渠道表）",
            )),
        }
    }
    assert!(problems.is_empty(), "{}", report(problems));
}

/// 失败信息里引用的那行可能很长：截**出错那段的前后窗口**，而不是行首 —— 否则读的人看不到
/// 出问题的那个 token（shell 示例动辄一整行）。
fn snippet_around(line: &str, needle: &str) -> String {
    let line = line.trim();
    const WINDOW: usize = 36;
    let chars: Vec<char> = line.chars().collect();
    let Some(at) = line
        .find(needle)
        .map(|byte_at| line[..byte_at].chars().count())
    else {
        return format!("{}…", chars.iter().take(WINDOW * 2).collect::<String>());
    };
    let start = at.saturating_sub(WINDOW);
    let end = (at + needle.chars().count() + WINDOW).min(chars.len());
    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        chars[start..end].iter().collect::<String>(),
        if end < chars.len() { "…" } else { "" },
    )
}
