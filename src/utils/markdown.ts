import MarkdownIt from "markdown-it";
import { openExternal } from "@/api";

const md = new MarkdownIt({ html: false, linkify: true, breaks: true, typographer: false });

// 所有链接用系统浏览器打开
const defaultLink = md.renderer.rules.link_open ?? ((tokens, idx, options, _env, self) => self.renderToken(tokens, idx, options));
md.renderer.rules.link_open = (tokens, idx, options, env, self) => {
  tokens[idx]?.attrSet("data-external", "1");
  tokens[idx]?.attrSet("rel", "noopener");
  return defaultLink(tokens, idx, options, env, self);
};

/** 去掉 Copilot 回答中的 ```actions …``` 代码块（流式时未闭合也去掉）。 */
export function stripActions(text: string): string {
  const i = text.search(/```\s*actions/i);
  return i >= 0 ? text.slice(0, i).trimEnd() : text;
}

export function renderMarkdown(text: string): string {
  return md.render(text || "");
}

/** 在渲染容器上委托处理外链点击 */
export function handleMdClick(e: MouseEvent): void {
  const a = (e.target as HTMLElement | null)?.closest("a");
  if (!a) return;
  const href = a.getAttribute("href") || "";
  e.preventDefault();
  if (/^https?:\/\//i.test(href)) void openExternal(href);
}
