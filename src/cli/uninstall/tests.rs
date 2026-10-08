//! `apim uninstall` 的沙盒测试：都在临时目录里造文件，绝不动真实的 apim / ~/.config。
//!
//! 认渠道靠路径字符串，所以「临时目录里的假 apim」就是 install.sh 渠道（`Binary`）——
//! 但 `target/` 下的路径被认成开发构建，所以 `run_with` 用的目录要在仓库外（系统临时目录），
//! 而 `None` 渠道那条分支正好用 `target/` 下的目录来测。

use super::cleanup;
use super::*;

/// 仓库内的临时目录：`detect_channel` 会把它认成开发构建（`None` 渠道）。
fn tmp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("apim-uninstall-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// 仓库外的临时目录：假 apim 放这儿才会被认成 install.sh 渠道（`Binary`）。
fn tmp_external(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("apim-uninstall-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// `target/debug/…` 下的临时目录：`detect_channel` 把这里认成开发构建（`None` 渠道）。
fn tmp_dev_build(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/debug")
        .join(format!("apim-uninstall-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn parse(items: &[&str]) -> Args {
    Args::parse(&items.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
}

/// 造一套「裸二进制 + 配置目录（含密钥）」的现场，返回 (exe, config_dir)。
fn fake_install(name: &str) -> (PathBuf, PathBuf) {
    let dir = tmp_external(name);
    let exe = dir.join("apim");
    fs::write(&exe, "#!/bin/sh\n").unwrap();
    let config_dir = dir.join("cfg").join("apim");
    fs::create_dir_all(config_dir.join("recipes")).unwrap();
    fs::write(config_dir.join("secrets.toml"), "sk-nope").unwrap();
    (exe, config_dir)
}

#[test]
fn dry_run_deletes_nothing() {
    let (exe, config_dir) = fake_install("dry-run");
    run_with(
        &parse(&["--yes", "--purge", "--dry-run"]),
        &exe,
        &config_dir,
    )
    .unwrap();
    assert!(exe.exists(), "--dry-run 不能删程序");
    assert!(
        config_dir.join("secrets.toml").exists(),
        "--dry-run 不能删配置（密钥）"
    );
}

#[test]
fn purges_binary_and_config_dir() {
    let (exe, config_dir) = fake_install("purge-both");
    run_with(&parse(&["--yes", "--purge"]), &exe, &config_dir).unwrap();
    assert!(fs::symlink_metadata(&exe).is_err(), "程序应当被删");
    assert!(!config_dir.exists(), "--purge 应当删掉配置目录");
}

/// 不加 --purge：程序删掉，密钥必须原样留着。
#[test]
fn keeps_keys_without_purge() {
    let (exe, config_dir) = fake_install("keep-keys");
    run_with(&parse(&["--yes"]), &exe, &config_dir).unwrap();
    assert!(fs::symlink_metadata(&exe).is_err(), "程序应当被删");
    assert_eq!(
        fs::read_to_string(config_dir.join("secrets.toml")).unwrap(),
        "sk-nope"
    );
}

/// --json 是非交互模式：没有 --yes 就报错，而且什么都不删。
#[test]
fn json_without_yes_is_refused() {
    let (exe, config_dir) = fake_install("json-no-yes");
    assert!(run_with(&parse(&["--json"]), &exe, &config_dir).is_err());
    assert!(exe.exists());
}

/// --purge 目标不合法（名字不是 apim）时必须在动手之前失败：程序还在。
#[test]
fn refuses_foreign_purge_target_before_removing_binary() {
    let (exe, config_dir) = fake_install("foreign-purge");
    let foreign = config_dir.parent().unwrap().join("not-apim");
    fs::create_dir_all(&foreign).unwrap();
    assert!(run_with(&parse(&["--yes", "--purge"]), &exe, &foreign).is_err());
    assert!(exe.exists(), "先校验后动手：目标不合法就不该删程序");
}

/// 开发构建（`target/debug/` 下）一条渠道都不算，连 --yes 也不删。
#[test]
fn never_removes_dev_builds() {
    let dir = tmp_dev_build("dev-build");
    let exe = dir.join("apim");
    fs::write(&exe, "#!/bin/sh\n").unwrap();
    run_with(&parse(&["--yes"]), &exe, &dir.join("cfg").join("apim")).unwrap();
    assert!(exe.exists(), "target/debug 下的开发构建不删");
}

/// `--json` 在非渠道场景也要给出完整形状（channel/command 为 null）并报错退出。
#[test]
fn json_shape_is_stable_for_dev_builds() {
    let dir = tmp_dev_build("json-dev-build");
    let exe = dir.join("apim");
    fs::write(&exe, "#!/bin/sh\n").unwrap();
    let err = run_with(
        &parse(&["--json", "--yes"]),
        &exe,
        &dir.join("cfg").join("apim"),
    );
    assert!(err.is_err(), "非渠道场景用 --json 要报错，别静默 exit 0");
    assert!(exe.exists());
}

/// `--purge` 删掉整个配置目录（密钥也在里面）。
#[test]
fn purge_removes_config_dir() {
    let dir = tmp("purge").join("apim");
    fs::create_dir_all(dir.join("recipes")).unwrap();
    fs::write(dir.join("secrets.toml"), "sk-nope").unwrap();
    assert_eq!(
        cleanup::purge_config_dir(&dir).unwrap(),
        Some(dir.display().to_string())
    );
    assert!(!dir.exists(), "配置目录应当被删掉");
}

/// 目录名不是 apim（APIM_CONFIG_DIR 指到别处）= 拒绝，且原样保留。
#[test]
fn purge_refuses_foreign_dir_name() {
    let dir = tmp("purge-foreign").join("not-apim");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("keep.txt"), "keep").unwrap();
    assert!(cleanup::purge_config_dir(&dir).is_err());
    assert!(dir.join("keep.txt").exists(), "拒绝后必须原样保留");
}

/// 配置目录是软链（dotfiles 常这么管）时拒绝删除 —— 免得「报告已删密钥、真身还在」，
/// 或者反过来把用户的 dotfiles 仓库删了。
#[cfg(unix)]
#[test]
fn purge_refuses_symlinked_config_dir() {
    let base = tmp_external("purge-symlink");
    let real = base.join("dotfiles").join("apim");
    fs::create_dir_all(&real).unwrap();
    fs::write(real.join("secrets.toml"), "sk-nope").unwrap();
    let link = base.join("apim");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    assert!(cleanup::purge_config_dir(&link).is_err());
    assert!(real.join("secrets.toml").exists(), "软链目标必须原样保留");
    assert!(fs::symlink_metadata(&link).is_ok(), "软链本身也不能删");
}

#[test]
fn purge_missing_dir_is_noop() {
    assert_eq!(
        cleanup::purge_config_dir(&tmp("purge-missing").join("apim")).unwrap(),
        None
    );
}

/// 软链（`~/.local/bin/apim` → 别处）要连链一起删：只删真身会留下断链。
#[cfg(unix)]
#[test]
fn removes_binary_and_its_symlink() {
    let base = tmp("symlink");
    let real = base.join("real").join("apim");
    fs::create_dir_all(real.parent().unwrap()).unwrap();
    fs::write(&real, "#!/bin/sh\n").unwrap();
    let link = base.join("bin").join("apim");
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert_eq!(cleanup::binary_targets(&link).len(), 2, "软链 + 真身都要删");

    cleanup::remove_binary(&link).unwrap();
    assert!(fs::symlink_metadata(&real).is_err(), "真身应当被删");
    assert!(fs::symlink_metadata(&link).is_err(), "软链不该留下断链");
    // 幂等：再删一次不报错
    cleanup::remove_binary(&link).unwrap();
}

/// 多级软链（`~/.local/bin/apim` → `/usr/local/bin/apim` → 别处）中间那一跳也要删，
/// 否则中间会留下断链。
#[cfg(unix)]
#[test]
fn removes_multi_hop_symlink_chain() {
    let base = tmp("symlink-chain");
    let real = base.join("opt").join("apim");
    fs::create_dir_all(real.parent().unwrap()).unwrap();
    fs::write(&real, "#!/bin/sh\n").unwrap();
    let mid = base.join("mid").join("apim");
    fs::create_dir_all(mid.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&real, &mid).unwrap();
    let top = base.join("top").join("apim");
    fs::create_dir_all(top.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&mid, &top).unwrap();
    assert_eq!(cleanup::binary_targets(&top).len(), 3, "三跳都要动");

    cleanup::remove_binary(&top).unwrap();
    for path in [&top, &mid, &real] {
        assert!(
            fs::symlink_metadata(path).is_err(),
            "{} 应当被删",
            path.display()
        );
    }
}

/// 软链指向「不叫 apim」的别的程序时只删链，真身不动（怕误删用户的脚本）。
#[cfg(unix)]
#[test]
fn keeps_symlink_target_that_is_not_apim() {
    let base = tmp("symlink-foreign-target");
    let other = base.join("my-own-wrapper");
    fs::write(&other, "#!/bin/sh\n").unwrap();
    let link = base.join("apim");
    std::os::unix::fs::symlink(&other, &link).unwrap();

    cleanup::remove_binary(&link).unwrap();
    assert!(fs::symlink_metadata(&link).is_err(), "链本身要删掉");
    assert!(other.exists(), "不叫 apim 的真身不能删");
}

/// 单跳实名（非软链）的裸二进制只报一条路径，别把 `/var` → `/private/var` 这类
/// 系统目录软链解析出来的另一个名字也算成「删了两个文件」。
#[test]
fn plain_binary_has_one_target() {
    let path = tmp("plain").join("apim");
    fs::write(&path, "x").unwrap();
    assert_eq!(cleanup::binary_targets(&path), vec![path]);
}

/// 文件名不是 apim 时拒绝删除（防误删）。
#[test]
fn refuses_unexpected_binary_name() {
    let path = tmp("unexpected-name").join("somethingelse");
    fs::write(&path, "x").unwrap();
    assert!(cleanup::remove_binary(&path).is_err());
    assert!(path.exists());
}

/// Windows 上裸二进制叫 apim.exe，名字校验要认。
#[test]
fn accepts_apim_exe_name() {
    assert!(cleanup::is_expected_name(Path::new("/x/apim.exe")));
    assert!(cleanup::is_expected_name(Path::new("/x/APIM.EXE")));
    assert!(!cleanup::is_expected_name(Path::new("/x/apim.cmd")));
}

#[test]
fn uninstall_commands_match_channel() {
    let exe = Path::new("/usr/local/bin/apim");
    assert_eq!(
        uninstall_command(Channel::Npm, exe),
        "npm uninstall -g apim-cli"
    );
    assert_eq!(
        uninstall_command(Channel::Homebrew, exe),
        "brew uninstall apim"
    );
    // Windows 上走 `del`，非 Windows 走 `rm`（同一分支的两种提示）
    #[cfg(not(windows))]
    assert_eq!(
        uninstall_command(Channel::Binary, exe),
        "rm /usr/local/bin/apim"
    );
    #[cfg(windows)]
    assert_eq!(
        uninstall_command(Channel::Binary, exe),
        "del \"/usr/local/bin/apim\""
    );
}

/// Codex 残留要报全 apim 写过的三处：模型目录 JSON、config.toml 里的指针、
/// 以及含明文密钥的 `config.toml.apim.bak` 备份。
#[test]
fn codex_leftovers_follows_apim_files() {
    let home = tmp("codex-home");
    assert!(
        cleanup::codex_leftovers_in(&home).is_empty(),
        "没导入过就没有残留"
    );

    fs::write(home.join("apim-models.json"), "{}").unwrap();
    fs::write(
        home.join("config.toml"),
        "model_catalog_json = \"apim-models.json\"\n",
    )
    .unwrap();
    fs::write(
        home.join("config.toml.apim.bak"),
        "experimental_bearer_token = \"sk-x\"\n",
    )
    .unwrap();
    fs::write(
        home.join("auth.json.apim.bak"),
        r#"{"auth_mode":"chatgpt","tokens":{"refresh_token":"rt"}}"#,
    )
    .unwrap();
    let found = cleanup::codex_leftovers_in(&home);
    assert_eq!(
        found.len(),
        4,
        "目录文件、config.toml 指针、两份备份各一条：{found:?}"
    );
    assert!(
        found.iter().any(|item| item.contains("apim.bak")),
        "含密钥的备份必须报出来：{found:?}"
    );
    // 官方路留下的 auth.json 备份里是上一份 ChatGPT 凭据（refresh token），不能漏报
    assert!(
        found.iter().any(|item| item.contains("auth.json.apim.bak")),
        "官方路的 ChatGPT 凭据备份必须报出来：{found:?}"
    );
}

/// Pi 残留要报 apim 写过的两处：models.json 里的 `providers.apim-*` 条目，
/// 以及含明文 apiKey 的 `models.json.apim.bak` 备份。
#[test]
fn pi_leftovers_follows_apim_files() {
    let dir = tmp("pi-agent");
    assert!(
        cleanup::pi_leftovers_in(&dir).is_empty(),
        "没导入过就没有残留"
    );

    fs::write(
        dir.join("models.json"),
        r#"{ "providers": { "apim-ikun": { "apiKey": "sk-x" } } }"#,
    )
    .unwrap();
    fs::write(dir.join("models.json.apim.bak"), r#"{ "providers": {} }"#).unwrap();
    let found = cleanup::pi_leftovers_in(&dir);
    assert_eq!(found.len(), 2, "条目与备份各一条：{found:?}");
    assert!(
        found.iter().any(|item| item.contains("apim.bak")),
        "含密钥的备份必须报出来：{found:?}"
    );
}

/// models.json 带 `//` 注释（pi 能读、apim 的 JSON 解析器不能）时也不能漏报残留 —— 退回文本判断。
#[test]
fn pi_leftovers_survives_a_jsonc_models_json() {
    let dir = tmp("pi-agent-jsonc");
    fs::write(
        dir.join("models.json"),
        "{\n  // 手写的注释\n  \"providers\": { \"apim-ikun\": { \"apiKey\": \"sk-x\" } }\n}\n",
    )
    .unwrap();
    let found = cleanup::pi_leftovers_in(&dir);
    assert_eq!(found.len(), 1, "含明文 apiKey 的条目必须报出来：{found:?}");
}

/// JSONC（带 `//` 注释）里只有用户自己的 provider → 退回文本判断也不能误报。
#[test]
fn pi_leftovers_jsonc_without_apim_providers_is_quiet() {
    let dir = tmp("pi-agent-jsonc-own");
    fs::write(
        dir.join("models.json"),
        "{\n  // 手写的注释\n  \"providers\": { \"sensenova\": { \"apiKey\": \"sk-y\" } }\n}\n",
    )
    .unwrap();
    assert!(cleanup::pi_leftovers_in(&dir).is_empty());
}

/// models.json 里只有用户自己的 provider（没有 `apim-` 前缀）= 不算残留。
#[test]
fn pi_leftovers_ignores_user_own_providers() {
    let dir = tmp("pi-agent-own");
    fs::write(
        dir.join("models.json"),
        r#"{ "providers": { "sensenova": { "apiKey": "sk-y" } } }"#,
    )
    .unwrap();
    assert!(cleanup::pi_leftovers_in(&dir).is_empty());
}

/// 只有 config.toml 但没有 apim 的指针（用户自己写的文件）= 不算残留。
#[test]
fn codex_leftovers_ignores_user_own_config() {
    let home = tmp("codex-own-config");
    fs::write(home.join("config.toml"), "model = \"gpt-5\"\n").unwrap();
    assert!(cleanup::codex_leftovers_in(&home).is_empty());
}
