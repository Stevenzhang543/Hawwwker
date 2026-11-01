<script setup lang="ts">
/* 命令面板（Ctrl+K）：输入指令回车即搜索；输入页面名跳转；最近搜索；快捷动作 */
import { computed, nextTick, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { NAV } from "@/nav";
import { useUiStore } from "@/stores/ui";
import { useSearchStore } from "@/stores/search";
import { useAppStore } from "@/stores/app";
import { toAppError } from "@/api";
import Icon from "@/components/ui/Icon.vue";

interface Cmd {
  id: string;
  label: string;
  hint: string;
  icon: string;
  group: string;
  run: () => void | Promise<void>;
}

const ui = useUiStore();
const search = useSearchStore();
const app = useAppStore();
const router = useRouter();
const q = ref("");
const sel = ref(0);
const input = ref<HTMLInputElement | null>(null);

watch(
  () => ui.paletteOpen,
  async (v) => {
    if (v) {
      q.value = "";
      sel.value = 0;
      if (!search.history.length) void search.loadHistory();
      await nextTick();
      input.value?.focus();
    }
  },
);

async function runSearch(text: string) {
  ui.paletteOpen = false;
  try {
    await router.push("/discover/search");
    await search.start(text);
    ui.toast("已开始多平台搜索", "ai", 1800, text);
  } catch (e) {
    ui.toast(toAppError(e).message, "error", 4000);
  }
}

const cmds = computed<Cmd[]>(() => {
  const text = q.value.trim();
  const out: Cmd[] = [];
  if (text) out.push({ id: "run", label: `搜索：${text}`, hint: "AI 多平台搜索", icon: "spark", group: "指令", run: () => runSearch(text) });
  const k = text.toLowerCase();
  for (const g of NAV)
    for (const it of g.items)
      if (!k || it.label.includes(text) || g.label.includes(text) || it.desc.includes(text) || it.path.includes(k))
        out.push({ id: it.path, label: `${g.label} · ${it.label}`, hint: it.desc, icon: it.icon, group: "跳转", run: () => void router.push(it.path) });
  const actions: Cmd[] = [
    { id: "copilot", label: "打开 Copilot", hint: "Ctrl J", icon: "bot", group: "动作", run: () => void (ui.copilotOpen = true) },
    { id: "theme", label: app.resolvedTheme === "dark" ? "切换为浅色模式" : "切换为深色模式", hint: "外观", icon: app.resolvedTheme === "dark" ? "sun" : "moon", group: "动作", run: () => app.toggleTheme() },
    { id: "watch", label: "粘贴链接追踪价格", hint: "价格追踪", icon: "tag", group: "动作", run: () => void router.push({ path: "/track/watchlist", query: { add: "1" } }) },
  ];
  out.push(...actions.filter((a) => !k || a.label.includes(text)));
  const recents = search.history.filter((h) => !k || h.instruction.toLowerCase().includes(k)).slice(0, 5);
  for (const h of recents)
    out.push({
      id: "h" + h.id, label: h.instruction, hint: "最近搜索", icon: "history", group: "最近",
      run: async () => {
        await search.loadRun(h.id);
        void router.push("/discover/search");
      },
    });
  return out.slice(0, 14);
});

watch(q, () => (sel.value = 0));

function onKey(e: KeyboardEvent) {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    sel.value = (sel.value + 1) % Math.max(1, cmds.value.length);
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    sel.value = (sel.value - 1 + cmds.value.length) % Math.max(1, cmds.value.length);
  } else if (e.key === "Enter") {
    e.preventDefault();
    exec(cmds.value[sel.value]);
  }
}
function exec(c?: Cmd) {
  if (!c) return;
  ui.paletteOpen = false;
  void c.run();
}
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="ui.paletteOpen" class="pal-back dof-backdrop" @pointerdown.self="ui.paletteOpen = false"></div>
    </Transition>
    <Transition name="pop">
      <div v-if="ui.paletteOpen" class="pal glass-3">
        <div class="pal-input">
          <Icon name="spark" :size="20" class="pal-spark" />
          <input ref="input" v-model="q" placeholder="说出你要找什么，例如：Switch 2 在京东和亚马逊日本哪个便宜" @keydown="onKey" />
          <kbd>Esc</kbd>
        </div>
        <div class="pal-list">
          <template v-for="(c, i) in cmds" :key="c.id">
            <div v-if="i === 0 || cmds[i - 1]?.group !== c.group" class="pal-group">{{ c.group }}</div>
            <button class="pal-item" :class="{ on: i === sel }" @mouseenter="sel = i" @click="exec(c)">
              <span class="pi-ico" :class="{ ai: c.id === 'run' }"><Icon :name="c.icon" :size="16" /></span>
              <span class="pi-label ellipsis">{{ c.label }}</span>
              <span class="pi-hint">{{ c.hint }}</span>
              <Icon v-if="i === sel" name="arrow" :size="14" class="pi-go" />
            </button>
          </template>
          <div v-if="!cmds.length" class="empty" style="padding: 24px">没有匹配项</div>
        </div>
        <div class="pal-foot">
          <span><kbd>↑</kbd><kbd>↓</kbd> 选择</span><span><kbd>Enter</kbd> 执行</span><span><kbd>Ctrl K</kbd> 开关</span>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.pal-back {
  position: fixed;
  inset: 0;
  z-index: 600;
  background: rgba(10, 14, 28, 0.2);
}
.pal {
  position: fixed;
  z-index: 601;
  top: 92px;
  left: 50%;
  translate: -50% 0;
  width: min(680px, calc(100vw - 40px));
  border-radius: 22px;
  overflow: hidden;
}
.pal-input {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 16px 18px;
  border-bottom: 1px solid var(--line);
}
.pal-spark {
  color: var(--brand);
}
.pal-input input {
  flex: 1;
  border: none;
  outline: none;
  background: transparent;
  font-size: 16px;
}
.pal-list {
  max-height: 420px;
  overflow-y: auto;
  padding: 6px;
}
.pal-group {
  padding: 8px 12px 4px;
  font-size: 11.5px;
  color: var(--text-3);
  font-weight: 600;
}
.pal-item {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 9px 12px;
  border: none;
  border-radius: 12px;
  background: transparent;
  cursor: pointer;
  text-align: left;
  transition: background 120ms;
}
.pal-item.on {
  background: var(--hover);
}
.pi-ico {
  width: 30px;
  height: 30px;
  border-radius: 9px;
  display: grid;
  place-items: center;
  background: var(--hover);
  color: var(--text-2);
  flex: none;
}
.pi-ico.ai {
  background: var(--brand-grad);
  color: #fff;
}
.pi-label {
  flex: 1;
  font-size: 13.5px;
  font-weight: 540;
}
.pi-hint {
  font-size: 12px;
  color: var(--text-3);
  white-space: nowrap;
}
.pi-go {
  color: var(--brand);
}
.pal-foot {
  display: flex;
  gap: 16px;
  padding: 10px 18px;
  border-top: 1px solid var(--line);
  font-size: 12px;
  color: var(--text-3);
}
.pal-foot kbd {
  margin-right: 3px;
}
</style>
