# apim

Manage model-provider API keys from the terminal: add, edit, delete, check health, check balance, copy.

**English** | [简体中文](README.zh-CN.md)

[![Release](https://github.com/tututuhehehe/apim-cli/actions/workflows/release.yml/badge.svg)](https://github.com/tututuhehehe/apim-cli/actions/workflows/release.yml)
[![CI](https://github.com/tututuhehehe/apim-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/tututuhehehe/apim-cli/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

<p align="center">
  <img src="docs/images/tui-main.png" alt="apim TUI: provider list with live balance and latency on the left, key table on the right, balance detail at the bottom" width="900">
</p>

> Note: the TUI and the `apim help` text are currently in Chinese. The CLI output and configuration formats are language-neutral. Localization contributions are welcome.

## Install

**One-liner (macOS / Linux)** — detects your platform, downloads the matching prebuilt binary, verifies its sha256 and installs it:

```bash
curl -fsSL https://raw.githubusercontent.com/tututuhehehe/apim-cli/main/install.sh | sh
```

`APIM_INSTALL_DIR` sets the destination (default: `/usr/local/bin` when writable, otherwise `~/.local/bin`); `APIM_VERSION` pins a version (e.g. `v0.1.0`; default is the latest release).

**Manual download** — grab the archive for your platform from [Releases](https://github.com/tututuhehehe/apim-cli/releases). Assets are named `apim-<tag>-<target>.tar.gz` (`.zip` on Windows), each with a matching `.sha256` checksum. Extract it and put `apim` on your `PATH`.

```bash
# macOS Apple Silicon example
tag=v0.1.0
base="https://github.com/tututuhehehe/apim-cli/releases/download/$tag"
curl -fsSL -O "$base/apim-$tag-aarch64-apple-darwin.tar.gz"
tar -xzf "apim-$tag-aarch64-apple-darwin.tar.gz"
sudo mv apim /usr/local/bin/
```

| Platform | Asset suffix |
|---|---|
| macOS Apple Silicon | `aarch64-apple-darwin` |
| macOS Intel | `x86_64-apple-darwin` |
| Linux x64 | `x86_64-unknown-linux-gnu` |
| Linux arm64 | `aarch64-unknown-linux-gnu` |
| Windows x64 | `x86_64-pc-windows-msvc` (experimental; balance scripts require `sh`) |

**From source**:

```bash
git clone https://github.com/tututuhehehe/apim-cli.git
cd apim-cli
cargo install --path .
```

Rust users can also run `cargo binstall apim` to fetch the prebuilt binary — the crate is configured for it, and it starts working once the crate is published to crates.io.

## Usage

```bash
apim            # no arguments: launch the TUI
apim help       # CLI usage
apim --version  # version
```

## Keybindings

`a` / `e` / `d` / `c` act on the focused pane: the provider list on the left, or the key table on the right.

| Key | Provider list (left) | Key table (right) |
|---|---|---|
| `a` | Add provider | Add key |
| `e` | Edit provider (name / base URL / homepage / health path / balance) | Edit key (alias / group / token) |
| `d` | Delete provider (remove all its keys first) | Delete key |
| `c` | Copy base URL | Copy key |
| `y` | Duplicate the provider: copy the whole recipe into a new provider (id becomes `<id>-copy`, name gets a "副本" / copy suffix); an external balance script is copied to its own file so the two providers evolve independently. Keys in `secrets.toml` are not copied | — |
| `i` | Provider details (auth / endpoints / balance script / origin / vars — values are hidden) | Key details (`r` toggles the full token, `c` copies) |
| `Enter` | Open the provider homepage (console) in your default browser | — |
| `m` | — | Fetch the model list with **the selected key** (visibility depends on the key/group; in the dialog `/` focuses the search box for live filtering, `Esc` leaves search back to the list (filter kept), `j`/`k` scroll, `c` copies a model name, `Esc` closes) |
| `j` / `k` | Move up/down | Move up/down |
| `Tab` / `h` / `l` | Switch panes (`h` = provider list, `l` = key table; no-op at the edge) | Same |
| `/` | Filter providers (matches id or display name) | Filter keys (matches alias or group) |
| `r` | Refresh health and balance | Same |
| `Ctrl+Z` | Undo the last write (providers/keys added, edited or deleted since the app opened, including files created by duplication); press repeatedly to step back | Same |
| `q` / `Esc` | Quit | Quit |

Press `/` to open the search box; filtering is live and case-insensitive, and each list keeps its own filter. `Enter` applies and closes it, `Esc` cancels and restores the previous value. While a filter is active the status bar shows `筛选: xxx (n/m)` ("filter: xxx") and `j`/`k` move only within the filtered rows; with no dialog open, `Esc` clears the filter instead of quitting (`q` still quits immediately). Note: while a filter is active, an item you rename or add that does not match it is hidden from view (the data is safe) — press `Esc` to clear the filter and see it again.

## CRUD

**Add a key (`a` in the right pane)**: fill in the alias, an optional group, and the key; switch providers with `←`/`→`. You can paste the key with `⌘V`. `Enter` saves, writes to disk immediately, and probes it.

**Add a provider (`a` in the left pane)**: fill in the ID (lowercase letters / digits / `-`; keys reference it via `provider`), a display name, the base URL, and three optional fields:

- **Homepage URL** — the provider's console, opened in your default browser with `Enter` on the provider; empty means unset (`Enter` will tell you). **Do not paste a one-click login link that contains a token** — the homepage shows up in the list and in `provider ls` output.
- **Health path** — defaults to `/models`, appended to the base URL; empty disables health checks.
- **Script path** — the balance script (see [Custom balance scripts](#custom-balance-scripts-the-one-and-only-balance-mechanism)); empty disables balance queries.

Saving creates `~/.config/apim/recipes/<id>.yaml`, after which `a` lets you add keys to it. You do not have to write the script yourself: paste `docs/quota-script-prompt.md` into any AI agent along with the provider's official query docs, and it will produce a script following apim's contract plus a verification command.

**Edit (`e`)**: the key form is pre-filled; changing the alias renames the key. In the provider form the ID is locked; if the health/script path is unchanged, saving leaves hand-written YAML untouched (inline scripts, `vars` access tokens, etc. are preserved). Clearing the script path removes the script balance. **After you save a changed provider config, that provider's cached readings and in-flight probes are invalidated and it is re-probed immediately with the new config** (so the panel never shows numbers computed from stale config); undoing a provider config change re-probes too.

**Delete (`d`)**: both ask for confirmation. A provider that still has keys is refused — delete the keys first. Built-in providers (DeepSeek / OpenAI / Moonshot AI / OpenRouter) cannot be deleted, but you can override them with `e` (this writes a YAML with the same id under your config dir).

**Duplicate (provider list, `y`)**: copies the selected provider wholesale — auth / vars / health path / balance binding (including any access token stored in `vars`); the id becomes `<source>-copy` (or `-copy-2`, … if taken) and the name gets a "副本" (copy) suffix. If it has an external balance script, the **file itself** is copied into `~/.config/apim/scripts/` (named after the new id, e.g. `glm-quota.sh` → `glm-copy-quota.sh`; an existing name is bumped to `-2`, `-3`, never overwritten), and the new provider points at the copy — afterwards the two scripts are independent. No script, or an inline `run`, means no file to copy. Keys in `secrets.toml` are **not** copied (what is duplicated is the protocol config, not credentials) — press `a` to give the new provider its own key.

**Undo (`Ctrl+Z`)**: every write since the app opened is recorded (provider/key add, edit and delete, plus YAML/script files created by duplication). Press `Ctrl+Z` on the main screen to step back one action at a time — memory and disk roll back together, and a toast at the bottom says what was undone (dialogs do not respond). Read-only actions (probing, copying to the clipboard, opening the homepage, browsing/searching) are not recorded; history is per-session and cleared when you quit the TUI.

**Inspect**: the status column shows the probe result (`● 可用` available / `● 失败` failed / `● 无额度` no balance), and the balance panel at the bottom right shows the selected key's balance. Press `i` for the details inspector: in the provider pane you see the whole recipe (auth / endpoints / balance script / origin / vars — variable values are shown only as `••••`); in the key pane you see everything, and `r` toggles the full token right there (no clipboard round-trip), while `c` copies. Refresh cadence: **every provider is probed once on open, then all of them are refreshed every 5 minutes** (health + balance together); switching providers reads the cache and triggers no request; `r` manually refreshes the current provider, and a newly saved key is probed immediately.

Inside a form:

| Key | Action |
|---|---|
| `Tab` / `↑` / `↓` | Next / previous field |
| `←` / `→` | Move the cursor; on the "provider" row, switch provider |
| `Enter` | Save |
| `Esc` | Cancel |

An empty required field, a duplicate ID, a base URL that does not start with `http(s)://`, and so on show a red message at the bottom and nothing is written.

## CLI (AI / script friendly)

The TUI is for humans, the CLI is for machines: after `cargo install --path .` every operation is available from the command line (`apim help` lists everything). They share the same data — a CLI change is immediately visible in the TUI and vice versa.

| Command | Description |
|---|---|
| `apim provider ls [--json]` | List providers (including balance binding mode and key count) |
| `apim provider add <id> --name <name> --base-url <URL> [--homepage <URL>\|none] [--health <path>\|none] [--script <path>\|none]` | Create a provider |
| `apim provider set <id> [--name <name>] [--base-url <URL>] [--homepage <URL>\|none] [--health <path>\|none] [--script <path>\|none]` | Update a provider (only the fields you pass) |
| `apim provider rm <id> [--force]` | Delete a provider (refused if it has keys; `--force` deletes them too; built-ins cannot be deleted) |
| `apim provider copy <src-id> [new-id] [--name <name>]` | Duplicate a provider (auth/vars/health/balance included; keys in `secrets.toml` are not); an external balance script is copied to its own file (named after the new id, existing names bumped to `-2`); the new id defaults to `<src-id>-copy` and auto-increments if taken |
| `apim key ls [<provider>] [--json]` | List keys (tokens masked) |
| `apim key add <provider> <alias> [--group <group>]` | Add a key; if it already exists, update its token |
| `apim key set <provider.alias> [--alias <new-alias>] [--group <group>\|none]` | Change alias / group |
| `apim key rm <provider.alias>` | Delete a key |
| `apim status [<provider>] [--json]` | Real health check + balance (runs the bound script) |
| `apim copy <provider.alias> [--base-url]` | Copy the key / base URL to the clipboard |
| `apim use <provider.alias>` | Print `export OPENAI_API_KEY=... OPENAI_BASE_URL=...` (for `eval $(apim use x)`) |

**Key safety**: tokens always come from stdin, never from command-line arguments (so they never leak into `ps` or your shell history):

```bash
echo 'your-key' | apim key add glm main
```

### Binding a balance script

A provider's balance query = one bound script (the recipe's `balance.kind: script` + a `command:` pointing at an executable). Three equivalent entry points:

1. **TUI**: the "script path" field in the provider form.
2. **CLI**: `apim provider add/set ... --script <path>` to bind, `--script none` to unbind (CLI `set` is an explicit instruction and replaces the whole thing; it does not do the TUI's "leave it alone if unchanged" dance).
3. **Write the YAML directly**: the AI-authored route — see `docs/quota-script-prompt.md`.

Put scripts in `~/.config/apim/scripts/<provider-id>-quota.sh` (a convention, not a requirement). Once bound, `apim status <provider>` and the TUI balance panel run the same script with identical results; the script's I/O contract (environment injection, one panel line per stdout line, non-zero exit = error) is described under [Custom balance scripts](#custom-balance-scripts-the-one-and-only-balance-mechanism) below.

### End-to-end: adding a new provider with AI

```bash
# 1. Paste docs/quota-script-prompt.md into an AI, plus the provider's official query docs
#    -> the AI produces ~/.config/apim/scripts/<id>-quota.sh (and can run the install commands)
# 2. The AI registers the provider and binds the script:
apim provider add glm --name "GLM Coding Plan" --base-url https://open.bigmodel.cn \
  --health /api/monitor/usage/quota/limit --script ~/.config/apim/scripts/glm-quota.sh
# 3. You configure the key yourself (it never passes through the AI):
echo 'your-key' | apim key add glm main
# 4. The AI verifies:
apim status glm --json
```

## Where the data lives

Everything lives under `~/.config/apim/`. TUI edits write these two files directly (mode 600), and you can edit them by hand as well:

- `config.toml` — alias and group (no tokens)
- `secrets.toml` — the actual tokens, keyed by `"provider.alias"`

Hand-edit example:

```toml
# ~/.config/apim/config.toml
[[keys]]
provider = "deepseek"
alias = "default"
# group = "personal"   # optional
```

```toml
# ~/.config/apim/secrets.toml
[tokens]
"deepseek.default" = "sk-your-key"
```

```bash
mkdir -p ~/.config/apim
chmod 600 ~/.config/apim/config.toml ~/.config/apim/secrets.toml
```

Set `APIM_CONFIG_DIR` to relocate the whole config directory (used by the tests).

## Adding a provider

Built-in providers work out of the box: **DeepSeek, OpenAI, Moonshot AI and OpenRouter** are compiled into the binary — no recipe needed. Select one in the left pane and press `a` to add a key. (OpenAI's balance endpoint is only available to some accounts; an HTTP error in the balance panel is expected otherwise.) Dropping a YAML with the same id into `~/.config/apim/recipes/` overrides the built-in definition.

For a custom provider the easiest path is `a` in the left pane; saving the form generates `~/.config/apim/recipes/<id>.yaml`.

For complex providers (custom auth headers, multi-level JSON parsing) you can write the YAML directly into the same directory — see `recipes/deepseek.yaml`:

```bash
mkdir -p ~/.config/apim/recipes
cp recipes/deepseek.yaml ~/.config/apim/recipes/my-relay.yaml
```

YAML generated by the TUI form and hand-written YAML are equivalent; on edit the form only overwrites the fields it knows about, so hand-written headers and parsing rules survive.

### new-api style relays (balance needs an access token)

On most new-api panels `/v1/dashboard/billing/subscription` either returns fake numbers or rejects the API key. The real balance is at `/api/user/self`, but it only accepts an **access token** (the one generated in personal settings, not the `sk-` key). Write a script (`~/.config/apim/scripts/<id>-quota.sh`):

```sh
#!/bin/sh
# Put the access token in the recipe's vars: {access_token: ...}; apim injects APIM_VAR_ACCESS_TOKEN
RESP="$(curl -sS --max-time 10 \
  -H "Authorization: Bearer ${APIM_VAR_ACCESS_TOKEN:?}" \
  "${APIM_BASE_URL}/api/user/self")"
printf '%s' "$RESP" | jq -r '"remaining $\(.data.quota / 500000 | floor)  (used $\(.data.used_quota / 500000 | floor))"'
```

Bind it and store the access token in the recipe:

```yaml
vars:
  access_token: your-access-token
health:
  url: '{base_url}/v1/models'
balance:
  kind: script
  command: ~/.config/apim/scripts/<id>-quota.sh
```

Health checks still use each key's own `sk-` key; the balance uses the access token in `vars` (balance is account-level, so all keys of the same account show the same value). The file contains a token — keep it at mode 600 and do not share it.

### Model-list endpoint (`m`)

With a key selected in the key table, press `m` to fetch the model list visible to **that key** (visibility depends on the key/group). Endpoints are tried in order: an explicit `models_url` in the recipe → the health path (when it ends in `/models`) → `{base_url}/models` → `{base_url}/v1/models`, falling through on 404 (other errors are returned as-is, preserving the real cause). Almost every OpenAI-compatible provider needs no configuration; for non-standard paths like GLM, add one line to the recipe:

```yaml
models_url: '{base_url}/api/paas/v4/models'
```

### Custom balance scripts (the one and only balance mechanism)

**Every balance query is a script**: apim runs a script with the key and shows its stdout line by line in the balance panel (the first line is highlighted). How the script queries and computes is entirely up to you — a single request against a well-behaved API (DeepSeek / Moonshot / OpenRouter …) is a few lines of shell + jq, while multi-request, date-aware ones (GLM Coding Plan) fit just as well.

```yaml
balance:
  kind: script
  command: ~/.config/apim/scripts/glm-quota.sh   # external executable (shebang respected)
  # run: |                                        # or an inline script run via shell -c
  #   echo "remaining 45/100"
  timeout_secs: 15                                # default 15
```

Contract:

- apim injects the environment variables `APIM_TOKEN` (the key), `APIM_BASE_URL`, `APIM_ALIAS`, `APIM_PROVIDER`, plus `APIM_VAR_<UPPERNAME>` for each entry in the recipe's `vars`. The key travels only via the environment, never as a command-line argument (invisible to `ps`).
- exit 0: each stdout line becomes a panel line, the first highlighted; non-zero exit / timeout: stderr (truncated) is shown as a red error.
- The script may use anything on your machine (curl, jq, python, …) — it is essentially "how to query this provider's balance", expressed as config. Full GLM example: `~/.config/apim/scripts/glm-quota.sh` + `~/.config/apim/recipes/glm.yaml`.

You can configure it in the TUI too: it is the "script path" field in the provider form; if the path is unchanged on edit, hand-written `run:` / `shell:` / `timeout_secs` are preserved, and clearing it removes the script balance.

Let an AI write it: paste `docs/quota-script-prompt.md` into any agent along with the provider's official query docs (documentation / curl examples); it will produce the script + recipe and a verification command — the key is only passed through an environment variable during verification, never handed to the AI.

The recipes of the four built-ins (DeepSeek / OpenAI / Moonshot / OpenRouter) are compiled into the binary, but the scripts they reference live in `~/.config/apim/scripts/` and are **not** bundled with the binary. On a fresh machine, have an AI regenerate them following `docs/quota-script-prompt.md`, or copy the scripts directory from another machine.

## License

[MIT](LICENSE) © 2026 tututuhehehe
