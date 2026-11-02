#!/usr/bin/env python3
"""生成 THIRD_PARTY_NOTICES.md：Rust 依赖（按 Windows 目标解析，不含 dev 依赖）+ 前端运行时依赖 + 随包分发的二进制组件。

用法：python3 scripts/third_party_notices.py <输出.md> [--target x86_64-pc-windows-gnu]
"""
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TAURI = ROOT / "src-tauri"


def cargo_packages(target: str):
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--filter-platform", target, "--locked"], cwd=TAURI))
    pkgs = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = meta["resolve"]["root"]
    seen, stack = set(), [root]
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for d in nodes[pid]["deps"]:
            kinds = {k.get("kind") for k in d["dep_kinds"]}
            if kinds - {"dev"}:
                stack.append(d["pkg"])
    seen.discard(root)
    out = [pkgs[i] for i in seen]
    out.sort(key=lambda p: (p["name"].lower(), p["version"]))
    return out


def npm_packages():
    tree = json.loads(subprocess.run(["npm", "ls", "--omit=dev", "--all", "--json"], cwd=ROOT,
                                     capture_output=True, text=True).stdout or "{}")
    found = {}

    def walk(deps):
        for name, info in (deps or {}).items():
            ver = info.get("version")
            if ver and (name, ver) not in found:
                pj = ROOT / "node_modules" / name / "package.json"
                meta = json.loads(pj.read_text(encoding="utf-8")) if pj.exists() else {}
                lic = meta.get("license") or (meta.get("licenses") or [{}])[0].get("type") or "见包内说明"
                repo = meta.get("repository")
                if isinstance(repo, dict):
                    repo = repo.get("url")
                found[(name, ver)] = {"name": name, "version": ver, "license": lic, "repository": clean_repo(repo or meta.get("homepage") or "")}
            walk(info.get("dependencies"))

    walk(tree.get("dependencies"))
    return sorted(found.values(), key=lambda p: p["name"].lower())


def clean_repo(url: str) -> str:
    url = (url or "").strip()
    url = re.sub(r"^git\+", "", url)
    url = re.sub(r"\.git$", "", url)
    url = url.replace("git://", "https://").replace("ssh://git@", "https://")
    if url.startswith("github:"):
        url = "https://github.com/" + url[7:]
    return url


SPDX_FILES = {
    "MIT": ["LICENSE-MIT", "LICENSE-MIT.md", "LICENSE.md", "LICENSE", "LICENSE.txt"],
    "Apache-2.0": ["LICENSE-APACHE", "LICENSE-APACHE.md", "LICENSE-APACHE-2.0"],
    "BSD-3-Clause": ["LICENSE", "LICENSE-BSD", "LICENSE.md"],
    "BSD-2-Clause": ["LICENSE", "LICENSE-BSD", "LICENSE.md"],
    "ISC": ["LICENSE", "LICENSE.md", "LICENSE-ISC"],
    "Zlib": ["LICENSE-ZLIB", "LICENSE-ZLIB.md", "LICENSE", "LICENSE.md"],
    "Unicode-3.0": ["LICENSE", "LICENSE-UNICODE", "LICENSE.md"],
    "MPL-2.0": ["LICENSE", "LICENSE.md", "LICENSE-MPL"],
    "BSL-1.0": ["LICENSE", "LICENSE_1_0.txt", "LICENSE-BOOST"],
    "CC0-1.0": ["LICENSE", "LICENSE-CC0", "LICENSE.md"],
    "Unlicense": ["UNLICENSE", "LICENSE", "LICENSE.md"],
    "0BSD": ["LICENSE", "LICENSE-0BSD", "LICENSE.md"],
    "CDLA-Permissive-2.0": ["LICENSE", "LICENSE-CDLA-PERMISSIVE-2.0", "LICENSE.md"],
}


def license_ids(expr: str):
    return sorted({t for t in re.split(r"[\s()/]+|\bOR\b|\bAND\b|\bWITH\b", expr or "") if t and t not in ("OR", "AND", "WITH", "LLVM-exception")})


def license_texts(cargo):
    texts = {}
    for spdx, names in SPDX_FILES.items():
        for p in cargo:
            if spdx not in license_ids(p.get("license") or ""):
                continue
            d = Path(p["manifest_path"]).parent
            for n in names:
                f = d / n
                if f.exists():
                    t = f.read_text(encoding="utf-8", errors="replace").strip()
                    # 只收与该协议相符的文本（避免把双协议包的另一份当成它）
                    probe = {"MIT": "Permission is hereby granted", "Apache-2.0": "Apache License", "BSD-3-Clause": "Redistribution and use",
                             "BSD-2-Clause": "Redistribution and use", "ISC": "Permission to use, copy, modify", "Zlib": "This software is provided 'as-is'",
                             "Unicode-3.0": "UNICODE", "MPL-2.0": "Mozilla Public License", "BSL-1.0": "Boost Software License",
                             "CC0-1.0": "CC0", "Unlicense": "This is free and unencumbered", "0BSD": "Permission to use, copy, modify",
                             "CDLA-Permissive-2.0": "Community Data License Agreement"}.get(spdx, "")
                    if probe.lower() in t.lower():
                        texts[spdx] = (p["name"], t)
                        break
            if spdx in texts:
                break
    return texts


def main():
    out = Path(sys.argv[1])
    target = "x86_64-pc-windows-gnu"
    if "--target" in sys.argv:
        target = sys.argv[sys.argv.index("--target") + 1]
    cargo = cargo_packages(target)
    npm = npm_packages()
    texts = license_texts(cargo)
    counts = {}
    for p in cargo:
        counts[p.get("license") or "未声明"] = counts.get(p.get("license") or "未声明", 0) + 1
    L = []
    L.append("# 第三方组件声明 · Third-Party Notices\n")
    L.append("Hawwwker 货客使用并随发布包分发了以下第三方开源组件。各组件版权归其作者所有，按各自许可证授权。")
    L.append("Hawwwker includes the third-party components listed below; each is the property of its respective authors and is licensed under its own terms.\n")
    L.append(f"- Rust crates（目标 `{target}`，不含仅用于开发/测试的依赖）：**{len(cargo)}** 个")
    L.append(f"- 前端运行时 npm 包（打包进界面）：**{len(npm)}** 个")
    L.append("- 随包分发的二进制组件：见第 1 节\n")
    L.append("## 1. 随包分发的二进制 / 内嵌组件\n")
    L.append("| 组件 | 用途 | 许可证 |\n|---|---|---|")
    L.append("| Microsoft Edge WebView2 Loader (`WebView2Loader.dll`) | Windows 界面渲染加载器（来自 WebView2 SDK，经 webview2-com-sys 提供） | Microsoft WebView2 SDK License（允许随应用再分发） |")
    L.append("| Microsoft Edge WebView2 Runtime | 界面运行时，安装包在缺失时下载微软官方引导程序；不随包分发 | Microsoft Software License Terms |")
    L.append("| SQLite（经 rusqlite / libsqlite3-sys bundled 静态编入） | 本地数据库 | Public Domain |")
    L.append("| NSIS 安装/启动器存根与插件（System、nsDialogs、StartMenu、LangDLL、NSISdl） | 安装包与便携版启动器 | zlib/libpng License |")
    L.append("| nsis_tauri_utils.dll | Tauri 安装包辅助插件 | MIT OR Apache-2.0 |")
    L.append("| MinGW-w64 运行时（静态链接部分） | Windows GNU 目标运行时支持 | ZPL-2.1 / MIT / Public Domain（含 GCC Runtime Library Exception） |\n")
    L.append("## 2. 前端运行时依赖（npm）\n")
    L.append("| 包 | 版本 | 许可证 | 来源 |\n|---|---|---|---|")
    for p in npm:
        L.append(f"| {p['name']} | {p['version']} | {p['license']} | {p['repository']} |")
    L.append("\n## 3. Rust 依赖（crates.io）\n")
    L.append("许可证分布：" + "；".join(f"`{k}` × {v}" for k, v in sorted(counts.items(), key=lambda kv: -kv[1])) + "\n")
    L.append("| Crate | 版本 | 许可证 | 来源 |\n|---|---|---|---|")
    for p in cargo:
        repo = clean_repo(p.get("repository") or p.get("homepage") or f"https://crates.io/crates/{p['name']}")
        L.append(f"| {p['name']} | {p['version']} | {p.get('license') or p.get('license_file') or '未声明'} | {repo} |")
    L.append("\n## 4. 许可证全文\n")
    L.append("以下为上述组件所用许可证的标准文本（各项目自己的版权行见其源代码仓库）。\n")
    for spdx in sorted(texts):
        name, t = texts[spdx]
        L.append(f"### {spdx}\n\n（文本取自 `{name}`）\n\n```text\n{t}\n```\n")
    out.write_text("\n".join(L) + "\n", encoding="utf-8")
    print(f"wrote {out}: {len(cargo)} crates, {len(npm)} npm packages, {len(texts)} license texts")


if __name__ == "__main__":
    main()
