#!/usr/bin/env bash
# Hawwwker 发布打包脚本：生成 releases/v<版本>/ 下的全部发布文件。
#
#   scripts/release.sh 26.02 [证据目录]
#
# 命名规范（每个版本都一样）：
#   releases/v<版本>/
#     README.md · RELEASE_NOTES.md · SHA256SUMS.txt · THIRD_PARTY_NOTICES.md
#     Hawwwker-v<版本>-release-evidence.zip      测试日志、截图、评测文档、构建日志
#     Hawwwker-v<版本>-source.zip                源代码（不含 node_modules / target / dist）
#     Hawwwker-v<版本>-web.zip                   网页预览版（单个 index.html，模拟数据）
#     Hawwwker-v<版本>-windows-x64-portable.exe  便携版（免安装）
#     Hawwwker-v<版本>-windows-x64-setup.exe     安装包
#
# 环境：在 Linux 上交叉编译 Windows（x86_64-pc-windows-gnu，需要 mingw-w64 与 makensis）；
# 在 Windows 上可改用 scripts\build.bat 生成 MSVC 版安装包，其余步骤相同。
# README.md / RELEASE_NOTES.md 取自 docs/releases/v<版本>/。
set -euo pipefail

V="${1:?用法: scripts/release.sh <版本，如 26.02> [证据目录]}"
EVIDENCE="${2:-}"
NAME=Hawwwker
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${RELEASE_DIR:-$ROOT/../releases/v$V}"
TARGET="${TARGET:-x86_64-pc-windows-gnu}"
REL="$ROOT/src-tauri/target/$TARGET/release"
MAJOR="${V%%.*}"; MINOR="${V#*.}"; FILEVER="$MAJOR.$((10#$MINOR)).0.0"

grep -q "\"version\": \"$MAJOR.$((10#$MINOR)).0\"" "$ROOT/src-tauri/tauri.conf.json" || { echo "tauri.conf.json 版本与 $V 不一致" >&2; exit 1; }
[ -f "$ROOT/docs/releases/v$V/README.md" ] && [ -f "$ROOT/docs/releases/v$V/RELEASE_NOTES.md" ] || { echo "缺少 docs/releases/v$V/README.md 或 RELEASE_NOTES.md" >&2; exit 1; }

mkdir -p "$OUT"
rm -f "$OUT/$NAME-v$V-"* "$OUT/SHA256SUMS.txt"
cd "$ROOT"

echo "==> 1/7 Windows 安装包（$TARGET）"
npx tauri build --target "$TARGET" --bundles nsis
SETUP="$REL/bundle/nsis/${NAME}_$MAJOR.$((10#$MINOR)).0_x64-setup.exe"
[ -f "$SETUP" ] || { echo "找不到本版本的安装包：$SETUP" >&2; exit 1; }
cp "$SETUP" "$OUT/$NAME-v$V-windows-x64-setup.exe"

echo "==> 2/7 便携版"
makensis -V2 -DVERSION="$V" -DFILEVER="$FILEVER" -DSRCDIR="$REL" -DICON="$ROOT/src-tauri/icons/icon.ico" \
  -DOUTFILE="$OUT/$NAME-v$V-windows-x64-portable.exe" "$ROOT/src-tauri/portable/portable.nsi"

echo "==> 3/7 网页预览版"
npm run build:web
WEBTMP="$(mktemp -d)"; mkdir -p "$WEBTMP/$NAME-v$V-web"
cp -r dist-web/. "$WEBTMP/$NAME-v$V-web/"
cat > "$WEBTMP/$NAME-v$V-web/使用说明.txt" <<EOF
Hawwwker 货客 v$V 网页预览版

双击 index.html 即可在浏览器中打开（建议 Chrome / Edge）。
网页版没有本机内核，使用内置的模拟数据演示全部界面与流程：搜索、渐进结果、到手价、价格判断、追踪、监控、洞察、Copilot。
URL 参数：#/... 后加 ?onboarding=1 查看首次引导；?noai=1 查看"AI 未连接"拦截；?ollama=1 模拟检测到本机模型。
真实搜索、登录与盯价请使用 Windows 安装包或便携版。
EOF
(cd "$WEBTMP" && zip -qr -X "$OUT/$NAME-v$V-web.zip" "$NAME-v$V-web")
rm -rf "$WEBTMP"

echo "==> 4/7 源代码"
SRCTMP="$(mktemp -d)"; mkdir -p "$SRCTMP/$NAME-v$V-source"
tar -C "$ROOT" --exclude=node_modules --exclude=dist --exclude=dist-web --exclude=src-tauri/target --exclude=src-tauri/gen \
  --exclude=.git --exclude='*.log' -cf - . | tar -C "$SRCTMP/$NAME-v$V-source" -xf -
(cd "$SRCTMP" && zip -qr -X "$OUT/$NAME-v$V-source.zip" "$NAME-v$V-source")
rm -rf "$SRCTMP"

echo "==> 5/7 第三方声明"
python3 "$ROOT/scripts/third_party_notices.py" "$OUT/THIRD_PARTY_NOTICES.md" --target "$TARGET"

echo "==> 6/7 说明文档与证据包"
cp "$ROOT/docs/releases/v$V/README.md" "$ROOT/docs/releases/v$V/RELEASE_NOTES.md" "$OUT/"
if [ -n "$EVIDENCE" ] && [ -d "$EVIDENCE" ]; then
  (cd "$EVIDENCE/.." && zip -qr -X "$OUT/$NAME-v$V-release-evidence.zip" "$(basename "$EVIDENCE")")
else
  echo "（未提供证据目录，跳过 release-evidence.zip）"
fi

echo "==> 7/7 校验和"
(cd "$OUT" && sha256sum README.md RELEASE_NOTES.md THIRD_PARTY_NOTICES.md "$NAME-v$V-"* > SHA256SUMS.txt)
ls -la "$OUT"
