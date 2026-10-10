#!/usr/bin/env python3
"""两个方向的 cfg 审计（apim）。

A 方向：unix-only 的符号 / trait 方法，是否都在「非 Windows」的门里。
B 方向：带 unix 门的项，是否被「Windows 上会编的代码」引用。

B 方向的做法（不靠编译器，因为 Windows target 本机编不动 —— ring 要 Windows SDK）：
  1. 从 src/main.rs 沿 `mod x;` 声明走一遍，遇到 `#[cfg(unix)] mod x;` 就把整棵子树标成
     「Windows 不编」（`--all-targets` 会编 `#[cfg(test)]`，所以测试模块也走）。
  2. 把每个 `#[cfg(...)]` 属性 + 它装饰的项/语句算成一个「区域」，按缩进判边界；
     区域分三类：unix（`cfg(unix)`/`cfg(all(unix…))`）、windows（`cfg(windows)`/`cfg(not(unix))`）、其它。
  3. 对每个 unix 门项的名字，在「Windows 会编的文件」里搜引用；如果这个名字**没有**非 unix 的
     同名兄弟（`cfg(not(unix))` 那种成对实现），且引用点不在 unix 区域里 → 报出来。
  行内注释（`//` / `///`）不算引用。

用法：audit_cfg.py <仓库根>
"""
import pathlib
import re
import sys

root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
SRC = root / "src"

ATTR = re.compile(r"^[ \t]*#\[(?P<body>.*)\][ \t]*$")


def cfg_kind(body):
    """属性体 → 'unix' | 'windows' | 'other' | None（不是 cfg 就 None）"""
    if not body.startswith("cfg("):
        return None
    b = body.replace(" ", "")
    if "not(unix)" in b or "windows" in b:
        return "windows"
    if "unix" in b or "linux" in b or "target_os=\"macos\"" in b:
        return "unix"
    return "other"


def mod_children(path):
    """该文件里的 `mod x;` → [(名字, cfg 类别), …]"""
    txt = path.read_text(encoding="utf-8")
    out = []
    pat = re.compile(
        r"^(?P<attrs>(?:[ \t]*#\[[^\n]*\][ \t]*\n)*)[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?mod[ \t]+(\w+)[ \t]*;",
        re.M,
    )
    for m in pat.finditer(txt):
        kinds = [cfg_kind(a) for a in re.findall(r"#\[([^\n]*)\]", m.group("attrs"))]
        out.append((m.group(2), "unix" if "unix" in kinds else ("windows" if "windows" in kinds else None)))
    return out


def resolve(path, name):
    base = path.parent
    if path.name in ("mod.rs", "main.rs", "lib.rs"):
        cands = [base / name / "mod.rs", base / f"{name}.rs"]
    else:
        cands = [base / path.stem / name / "mod.rs", base / path.stem / f"{name}.rs"]
    return next((c for c in cands if c.exists()), None)


def regions(path):
    """[(起始行, 结束行, 类别), …]（1-based，含端点）"""
    lines = path.read_text(encoding="utf-8").splitlines()
    out, i = [], 0
    while i < len(lines):
        m = ATTR.match(lines[i])
        kind = cfg_kind(m.group("body")) if m else None
        if kind is None:
            i += 1
            continue
        indent = len(lines[i]) - len(lines[i].lstrip())
        j = i + 1  # 跳过后续属性/注释/空行，找到被装饰的项
        while j < len(lines) and (
            lines[j].strip().startswith("#[") or lines[j].strip().startswith("//") or not lines[j].strip()
        ):
            j += 1
        item_indent = (len(lines[j]) - len(lines[j].lstrip())) if j < len(lines) else indent
        k = j + 1
        while k < len(lines):
            t = lines[k]
            if t.strip() and (len(t) - len(t.lstrip())) <= item_indent:
                break
            k += 1
        out.append((i + 1, k, kind))
        i += 1  # 不跳过：要能看见嵌在别的区域里的门（如 cfg(test) mod tests 里的 cfg(unix)）
    return out


def kind_at(regs, line):
    """某一行落在哪种区域里（最内层）；没落进任何区域 → None（= 默认，Windows 也会编）"""
    inner = [r for r in regs if r[0] <= line <= r[1]]
    if not inner:
        return None
    return min(inner, key=lambda r: r[1] - r[0])[2]


# ---------------------------------------------------------------- Windows 编译集
compiled, excluded = set(), set()


def walk(p):
    p = pathlib.Path(p)
    if p in compiled or p in excluded:
        return
    compiled.add(p)
    for name, kind in mod_children(p):
        child = resolve(p, name)
        if child is None:
            print(f"  ! 解析不了 mod {name}（在 {p}）")
            continue
        mark(child) if kind == "unix" else walk(child)


def mark(p):
    p = pathlib.Path(p)
    if p in excluded:
        return
    excluded.add(p)
    for name, _ in mod_children(p):
        child = resolve(p, name)
        if child:
            mark(child)


walk(SRC / "main.rs")
print("=== Windows 上会被编译的测试模块 ===")
print("  编译:", ", ".join(sorted(str(f.relative_to(SRC)) for f in compiled if "tests" in f.parts)) or "（无）")
print("  不编:", ", ".join(sorted(str(f.relative_to(SRC)) for f in excluded)) or "（无）")

# ---------------------------------------------------------------- 收集 unix 门项
gated = {}   # 名字 → [(文件, 行)]
for f in sorted(SRC.rglob("*.rs")):
    regs = regions(f)
    lines = f.read_text(encoding="utf-8").splitlines()
    for i, line in enumerate(lines):
        m = re.match(
            r"[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?(?:async[ \t]+)?(fn|const|static|struct|enum|trait|type|mod)[ \t]+(\w+)",
            line,
        )
        if m and kind_at(regs, i + 1) == "unix":
            gated.setdefault(m.group(2), []).append((f, i + 1))

# 非 unix 的同名兄弟（cfg(not(unix)) 成对实现）→ 引用它不算问题
siblings = set()
for f in sorted(SRC.rglob("*.rs")):
    regs = regions(f)
    for i, line in enumerate(f.read_text(encoding="utf-8").splitlines()):
        m = re.match(
            r"[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?(?:async[ \t]+)?(fn|const|static|struct|enum|trait|type|mod)[ \t]+(\w+)",
            line,
        )
        if m and kind_at(regs, i + 1) in (None, "windows"):
            siblings.add(m.group(2))

print(f"\n=== B 方向：{len(gated)} 个 unix 门项（其中 {len(gated) - len(set(gated) - siblings)} 个有非 unix 兄弟）===")
bad_b = []
for name in sorted(gated):
    if name in siblings:
        continue
    where = ", ".join(f"{f.relative_to(SRC)}:{ln}" for f, ln in gated[name])
    for f in sorted(compiled):
        regs = regions(f)
        for i, line in enumerate(f.read_text(encoding="utf-8").splitlines()):
            s = line.strip()
            if s.startswith("//") or not re.search(rf"\b{re.escape(name)}\b", line):
                continue
            if kind_at(regs, i + 1) != "unix":
                bad_b.append((name, where, f"{f.relative_to(SRC)}:{i + 1}: {s[:78]}"))
for name, where, hit in bad_b:
    print(f"\n  ⚠ {name}（门在 {where}）被未带门的行引用：\n      {hit}")
print(f"\n  B 方向未带门的引用：{len(bad_b)} 处" + ("（应为 0 ✓）" if not bad_b else " ✗"))

# ---------------------------------------------------------------- A 方向
print("\n=== A 方向：unix-only 的符号与 trait 方法是否都在非 Windows 的门里 ===")
sym = re.compile(
    r"std::os::unix|std::os::fd|libc::|PermissionsExt|from_mode|\.mode\(\)|set_mode|unix::fs::symlink|OpenOptionsExt"
)
bad_a = []
for f in sorted(SRC.rglob("*.rs")):
    regs = regions(f)
    for i, line in enumerate(f.read_text(encoding="utf-8").splitlines()):
        if line.strip().startswith("//") or not sym.search(line):
            continue
        if kind_at(regs, i + 1) not in ("unix", "windows"):
            bad_a.append(f"{f.relative_to(SRC)}:{i + 1}: {line.strip()[:78]}")
print("  未带门的引用：", "\n    " + "\n    ".join(bad_a) if bad_a else "（无 ✓）")
