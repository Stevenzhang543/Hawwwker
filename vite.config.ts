import { defineConfig, type Plugin } from "vite";
import vue from "@vitejs/plugin-vue";
import { fileURLToPath, URL } from "node:url";
import { existsSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const host = process.env.TAURI_DEV_HOST;

/**
 * 网页预览版（npm run build:web）：把 JS/CSS 内联进单个 index.html，双击即可在浏览器里打开（file:// 不允许加载外部模块脚本）。
 * 网页版没有 Tauri 内核，自动使用内置模拟数据演示全部界面。
 */
function inlineSingleFile(): Plugin {
  let outDir = "dist-web";
  return {
    name: "hawwwker-inline-single-file",
    apply: "build",
    enforce: "post",
    configResolved(c) {
      outDir = resolve(c.root, c.build.outDir);
    },
    // 在文件写盘之后再内联，确保 vite 的占位符（如 __VITE_PRELOAD__）都已替换
    closeBundle() {
      const htmlPath = join(outDir, "index.html");
      if (!existsSync(htmlPath)) return;
      let html = readFileSync(htmlPath, "utf8");
      html = html.replace(/<script type="module" crossorigin src="\.\/([^"]+)"><\/script>/g, (_m, file: string) => {
        const p = join(outDir, file);
        const code = readFileSync(p, "utf8").replace(/<\/script/gi, "<\\/script");
        rmSync(p);
        return `<script type="module">${code}</script>`;
      });
      html = html.replace(/<link rel="stylesheet" crossorigin href="\.\/([^"]+)">/g, (_m, file: string) => {
        const p = join(outDir, file);
        const css = readFileSync(p, "utf8");
        rmSync(p);
        return `<style>${css}</style>`;
      });
      writeFileSync(htmlPath, html);
      const assets = join(outDir, "assets");
      if (existsSync(assets) && readdirSync(assets).length === 0) rmSync(assets, { recursive: true });
    },
  };
}

// https://v2.tauri.app/start/frontend/vite/
export default defineConfig(({ mode }) => ({
  plugins: mode === "web" ? [vue(), inlineSingleFile()] : [vue()],
  base: mode === "web" ? "./" : "/",
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome110" : "es2022",
    minify: process.env.TAURI_ENV_DEBUG ? false : "esbuild",
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    chunkSizeWarningLimit: 1200,
    ...(mode === "web"
      ? { outDir: "dist-web", cssCodeSplit: false, assetsInlineLimit: 100_000_000, rollupOptions: { output: { inlineDynamicImports: true } } }
      : {}),
  },
}));
