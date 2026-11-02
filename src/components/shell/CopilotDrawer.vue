<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useUiStore } from "@/stores/ui";
import { useCopilotStore } from "@/stores/copilot";
import { useSearchStore } from "@/stores/search";
import { useWatchStore } from "@/stores/watch";
import { useAppStore } from "@/stores/app";
import type { CopilotAction, CopilotMessage } from "@/api/types";
import { findNav } from "@/nav";
import type { CopilotContextChip } from "@/stores/copilot";
import { handleMdClick, renderMarkdown, stripActions } from "@/utils/markdown";
import Icon from "@/components/ui/Icon.vue";

const ui = useUiStore();
const cp = useCopilotStore();
const search = useSearchStore();
const watchStore = useWatchStore();
const app = useAppStore();
const router = useRouter();
const route = useRoute();
const text = ref("");
const skillMenu = ref(false);
const listEl = ref<HTMLElement | null>(null);
const inputEl = ref<HTMLTextAreaElement | null>(null);
let release: (() => void) | null = null;

watch(
  () => ui.copilotOpen,
  async (v) => {
    if (v) {
      void cp.init();
      release = ui.pushEsc(() => (ui.copilotOpen = false));
      await nextTick();
      inputEl.value?.focus();
      scroll();
    } else {
      release?.();
      release = null;
    }
  },
);

function scroll() {
  nextTick(() => listEl.value?.scrollTo({ top: listEl.value.scrollHeight, behavior: "smooth" }));
}
watch(() => [cp.messages.length, cp.streaming?.text.length], scroll);

const streamingHtml = computed(() => (cp.streaming ? renderMarkdown(stripActions(cp.streaming.text)) : ""));
const activeSkill = computed(() => cp.skills.find((s) => s.id === cp.skill) ?? null);
/** 技能：3 个常用（按角色排序）直接显示，其余收进「技能」下拉 */
const topSkills = computed(() => cp.sortedSkills.slice(0, 3));
const moreSkills = computed(() => cp.sortedSkills.slice(3));
const moreActive = computed(() => moreSkills.value.find((s) => s.id === cp.skill) ?? null);

/** 可附加的页面上下文：只有附加后 Copilot 才能看到（不会自动上传页面内容） */
const here = computed(() => findNav(route.path));
/** 自动附带：所在页面 + 当前搜索（结果页上附前 8 条精简结果） */
const autoSeen = computed(() => {
  const parts = [here.value ? here.value.item.label : "当前页面"];
  const run = search.current;
  if (run) parts.push(`当前搜索「${run.instruction.length > 10 ? run.instruction.slice(0, 10) + "…" : run.instruction}」`);
  return parts.join(" · ");
});
const suggestion = computed<CopilotContextChip | null>(() => {
  if (cp.context) return null;
  if (route.path.startsWith("/track/watchlist") && watchStore.items.length) {
    return {
      label: `我的追踪清单（${watchStore.items.length} 件）`,
      data: {
        kind: "watchlist",
        items: watchStore.items.slice(0, 30).map((w) => ({ id: w.listingId, watchId: w.id, title: w.title, platformId: w.platformId, lastPrice: w.lastPrice, currency: w.currency, targetPrice: w.targetPrice, lowest: w.lowest, highest: w.highest, changePct: w.changePct, deal: w.deal?.label ?? null, myPrice: w.myPrice ?? null })),
      },
    };
  }
  if (route.path.startsWith("/track/monitors") && watchStore.monitors.length) {
    return {
      label: `我的监控（${watchStore.monitors.length} 个）`,
      data: { kind: "monitors", items: watchStore.monitors.map((m) => ({ name: m.name, instruction: m.instruction, bestPrice: m.bestPrice, currency: m.currency, targetPrice: m.targetPrice, runCount: m.runCount })) },
    };
  }
  return null;
});

function send() {
  const t = text.value.trim();
  if (!t) return;
  text.value = "";
  void cp.send(t);
}
function onKey(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
    e.preventDefault();
    send();
  }
}
function pickSkill(id: string) {
  skillMenu.value = false;
  cp.skill = cp.skill === id ? null : id;
  const s = cp.skills.find((x) => x.id === id);
  if (cp.skill && s && !text.value) text.value = s.starter;
  inputEl.value?.focus();
}

function execOf(m: CopilotMessage, i: number) {
  return cp.exec[m.id]?.[i] ?? null;
}
/** 自动执行的动作 / 已执行过的动作显示状态行，其余显示为按钮 */
function autoActs(m: CopilotMessage) {
  return m.actions.map((a, i) => ({ a, i, e: execOf(m, i) })).filter((x) => x.a.auto || x.e);
}
function buttonActs(m: CopilotMessage) {
  return m.actions.map((a, i) => ({ a, i, e: execOf(m, i) })).filter((x) => !x.a.auto && !x.e);
}
const run = computed(() => cp.activeRun);
const runDone = computed(() => (run.value ? Object.values(run.value.platforms).filter((p) => ["done", "error", "skipped", "login_required"].includes(p.state)).length : 0));
const runTotal = computed(() => (run.value ? Object.keys(run.value.platforms).length : 0));
const runCount = computed(() => {
  const r = run.value;
  if (!r) return 0;
  return r.results.length || Object.values(r.partial ?? {}).reduce((n, l) => n + l.length, 0);
});
async function cancelRun() {
  if (!run.value) return;
  try {
    await search.cancel(run.value.runId);
  } catch (e) {
    ui.toast(String((e as { message?: string }).message ?? e), "error");
  }
}
async function viewRun() {
  if (!run.value) return;
  search.currentId = run.value.runId;
  if (!route.path.startsWith("/discover/search")) await router.push("/discover/search");
}
const actIcon: Record<CopilotAction["type"], string> = { search: "search", watch: "tag", open: "external", navigate: "arrow", monitor: "clock", login: "login", setting: "gear", results: "list" };

async function reset() {
  if (await ui.confirm("清空对话？", "将删除本会话的全部消息。", "清空")) await cp.reset();
}
</script>

<template>
  <Transition name="drawer">
    <aside v-if="ui.copilotOpen" class="cp glass-3">
      <header class="cp-head">
        <span class="cp-logo"><Icon name="bot" :size="18" /></span>
        <div class="cp-title">
          <b>Copilot</b>
          <small>{{ app.ai.model || "未连接模型" }} · 说"用 app 搜索…"我会直接操作，其余动作点按钮执行</small>
        </div>
        <button class="icon-btn" title="清空对话" @click="reset"><Icon name="refresh" :size="16" /></button>
        <button class="icon-btn" title="关闭（Esc）" @click="ui.copilotOpen = false"><Icon name="x" /></button>
      </header>

      <div class="skills">
        <button v-for="s in topSkills" :key="s.id" v-ripple class="skill" :class="{ on: cp.skill === s.id }" :title="s.description" @click="pickSkill(s.id)">
          <Icon :name="s.icon" :size="15" />{{ s.name }}
        </button>
        <div v-if="moreSkills.length" class="more" @mouseleave="skillMenu = false">
          <button v-ripple class="skill" :class="{ on: !!moreActive }" :aria-expanded="skillMenu" @click="skillMenu = !skillMenu">
            <Icon :name="moreActive?.icon ?? 'grid'" :size="15" />{{ moreActive?.name ?? `技能 ${moreSkills.length}` }}<Icon name="down" :size="13" />
          </button>
          <Transition name="pop">
            <div v-if="skillMenu" class="skill-menu glass-3">
              <button v-for="s in moreSkills" :key="s.id" class="sm-item" :class="{ on: cp.skill === s.id }" @click="pickSkill(s.id)">
                <Icon :name="s.icon" :size="16" />
                <span><b>{{ s.name }}</b><small>{{ s.description }}</small></span>
                <Icon v-if="cp.skill === s.id" name="check" :size="14" class="ok" />
              </button>
            </div>
          </Transition>
        </div>
      </div>

      <div ref="listEl" class="msgs" @click="handleMdClick">
        <div v-if="!cp.messages.length && !cp.streaming" class="welcome">
          <div class="w-orb breathe"><Icon name="spark" :size="26" /></div>
          <h3>今天想让我帮你做什么？</h3>
          <p class="muted small">可以让我构思礼物、比较同款在各平台的到手价、判断一个品类的行情，或者帮商品写标题。</p>
          <div class="starters">
            <button v-for="s in cp.sortedSkills.slice(0, 4)" :key="s.id" class="starter" @click="cp.skill = s.id; cp.send(s.starter)">
              <Icon :name="s.icon" :size="15" />
              <span>{{ s.starter }}</span>
            </button>
          </div>
        </div>

        <div v-for="m in cp.messages" :key="m.id" class="msg" :class="m.role">
          <div v-if="m.role === 'user'" class="bubble selectable">{{ m.content }}</div>
          <div v-else-if="m.role === 'event'" class="event"><Icon name="zap" :size="12" />{{ m.content }}</div>
          <template v-else>
            <div class="md selectable" v-html="renderMarkdown(stripActions(m.content))"></div>
            <div v-if="autoActs(m).length" class="execs">
              <div v-for="x in autoActs(m)" :key="x.i" class="exec" :class="x.e?.status ?? 'pending'">
                <span class="e-ico">
                  <span v-if="!x.e || x.e.status === 'running'" class="spin"></span>
                  <Icon v-else-if="x.e.status === 'done'" name="check" :size="13" :stroke="2.6" />
                  <Icon v-else name="warn" :size="13" />
                </span>
                <span class="e-text"><b>{{ x.a.auto ? "自动执行" : "已执行" }}</b> · <Icon :name="actIcon[x.a.type]" :size="12" />{{ x.a.label }}<small v-if="x.e"> — {{ x.e.note }}</small><small v-else> — 等待执行</small></span>
                <button v-if="x.e?.status === 'failed'" class="retry" @click="cp.clickAction(m, x.i)">重试</button>
              </div>
            </div>
            <div v-if="buttonActs(m).length" class="acts">
              <button v-for="x in buttonActs(m)" :key="x.i" v-ripple class="btn sm" @click="cp.clickAction(m, x.i)">
                <Icon :name="actIcon[x.a.type]" :size="14" />{{ x.a.label }}
              </button>
            </div>
          </template>
        </div>

        <div v-if="cp.sending && cp.streaming" class="msg assistant">
          <div v-if="cp.streaming.text" class="md selectable" v-html="streamingHtml"></div>
          <span class="caret"></span>
        </div>
        <div v-else-if="cp.sending" class="msg assistant">
          <span class="thinking"><i></i><i></i><i></i></span>
        </div>
        <p v-if="cp.error" class="banner" style="margin-top: 8px"><Icon name="warn" />{{ cp.error }}</p>
      </div>

      <Transition name="blur">
        <div v-if="run" class="runcard">
          <div class="rc-head">
            <span class="rc-ico" :class="run.status"><span v-if="run.status === 'running'" class="spin"></span><Icon v-else :name="run.status === 'done' ? 'check' : 'warn'" :size="14" /></span>
            <div class="rc-text">
              <b>{{ run.status === "running" ? run.stageLabel || "正在搜索…" : run.status === "done" ? "搜索完成，正在请 Copilot 点评" : "搜索已结束" }}</b>
              <small>{{ run.instruction }}</small>
            </div>
            <button v-if="run.status === 'running'" class="btn ghost sm" @click="cancelRun"><Icon name="stop" :size="13" />取消</button>
            <button class="btn sm" @click="viewRun"><Icon name="arrow" :size="13" />查看</button>
          </div>
          <div class="rc-bar"><i :style="{ width: Math.round((run.progress || 0) * 100) + '%' }"></i></div>
          <small class="rc-meta">平台 {{ runDone }}/{{ runTotal }} · 已出 {{ runCount }} 条{{ run.status === "running" && !run.results.length && runCount ? "（初步）" : "" }}</small>
        </div>
      </Transition>

      <footer class="composer">
        <div class="ctx">
          <span class="ctx-label" :title="`你在：${here ? here.group.label + ' · ' + here.item.label : '—'}。Copilot 只看到你的输入、角色和下面附加的上下文。`"><Icon name="eye" :size="12" />Copilot 看到</span>
          <span v-if="activeSkill" class="chip purple"><Icon :name="activeSkill.icon" />{{ activeSkill.name }}<button class="x" title="取消技能" @click="cp.skill = null"><Icon name="x" :size="11" /></button></span>
          <span v-if="cp.context" class="chip blue"><Icon name="tag" />{{ cp.context.label }}<button class="x" title="移除上下文" @click="cp.setContext(null)"><Icon name="x" :size="11" /></button></span>
          <button v-else-if="suggestion" class="chip outline click" title="附加后 Copilot 才能看到这些内容" @click="cp.setContext(suggestion)"><Icon name="plus" />附上{{ suggestion.label }}</button>
          <span class="faint tiny" :title="'自动附带：你所在的页面与当前搜索的概要（结果页上附前 8 条精简结果），用于理解“这些结果”“当前页面”。'">{{ autoSeen }}</span>
        </div>
        <div class="input-row">
          <textarea ref="inputEl" v-model="text" rows="2" placeholder="问点什么…（Enter 发送，Shift+Enter 换行）" @keydown="onKey"></textarea>
          <button v-ripple class="send" :disabled="!text.trim() || cp.sending" title="发送" @click="send"><Icon name="send" :size="17" /></button>
        </div>
      </footer>
    </aside>
  </Transition>
</template>

<style scoped>
.cp {
  position: fixed;
  z-index: 250;
  top: calc(var(--titlebar-h) + 8px);
  right: 10px;
  bottom: 10px;
  width: 420px;
  max-width: calc(100vw - 20px);
  border-radius: var(--r-xl);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.cp-head {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 14px 12px 12px 16px;
  border-bottom: 1px solid var(--line);
}
.cp-logo {
  width: 34px;
  height: 34px;
  border-radius: 11px;
  display: grid;
  place-items: center;
  background: var(--brand-grad);
  color: #fff;
  box-shadow: 0 6px 16px rgba(80, 100, 255, 0.35);
}
.cp-title {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  line-height: 1.3;
}
.cp-title small {
  font-size: 11px;
  color: var(--text-3);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.skills {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding: 10px 14px;
  border-bottom: 1px solid var(--line);
  scrollbar-width: none;
}
.skill {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 30px;
  padding: 0 11px;
  border-radius: 999px;
  border: 1px solid var(--line-2);
  background: var(--glass-1);
  font-size: 12.5px;
  font-weight: 560;
  cursor: pointer;
  white-space: nowrap;
  transition: transform var(--t-mid) var(--spring), background var(--t-fast);
}
.more {
  position: relative;
}
.skill-menu {
  position: absolute;
  left: 0;
  top: 36px;
  width: 300px;
  padding: 6px;
  border-radius: 14px;
  z-index: 40;
  display: flex;
  flex-direction: column;
}
.sm-item {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 9px 10px;
  border: none;
  background: transparent;
  border-radius: 10px;
  cursor: pointer;
  text-align: left;
  color: var(--text);
}
.sm-item:hover {
  background: var(--hover);
}
.sm-item.on {
  color: var(--brand-ink);
}
.sm-item span {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.sm-item b {
  font-size: 13px;
}
.sm-item small {
  font-size: 11.5px;
  color: var(--text-3);
  line-height: 1.35;
}
.sm-item .ok {
  margin-top: 2px;
}
.ctx-label {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  color: var(--text-3);
  margin-right: 2px;
}
.skill:active {
  transform: scale(0.94);
}
.skill.on {
  background: var(--brand-grad);
  color: #fff;
  border-color: transparent;
}
.msgs {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.welcome {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: 8px;
  padding: 18px 6px;
}
.w-orb {
  width: 56px;
  height: 56px;
  border-radius: 18px;
  display: grid;
  place-items: center;
  background: var(--brand-grad);
  color: #fff;
  box-shadow: 0 10px 28px rgba(80, 100, 255, 0.38);
  margin-bottom: 6px;
}
.welcome h3 {
  font-size: 16px;
}
.starters {
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: 100%;
  margin-top: 10px;
}
.starter {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border-radius: 12px;
  border: 1px solid var(--line);
  background: var(--glass-1);
  text-align: left;
  font-size: 12.5px;
  color: var(--text-2);
  cursor: pointer;
  transition: background var(--t-fast), transform var(--t-mid) var(--spring);
}
.starter:hover {
  background: var(--hover);
  color: var(--text);
  transform: translateX(3px);
}
.msg {
  display: flex;
  flex-direction: column;
  animation: rise 420ms var(--spring-soft);
}
.msg.user {
  align-items: flex-end;
}
.bubble {
  max-width: 86%;
  padding: 9px 13px;
  border-radius: 16px 16px 4px 16px;
  background: var(--brand-grad);
  color: #fff;
  font-size: 13.5px;
  white-space: pre-wrap;
}
.md {
  font-size: 13.5px;
  line-height: 1.65;
}
.md :deep(p) {
  margin: 0 0 8px;
}
.md :deep(ul),
.md :deep(ol) {
  margin: 4px 0 8px;
  padding-left: 20px;
}
.md :deep(li) {
  margin: 3px 0;
}
.md :deep(blockquote) {
  margin: 8px 0;
  padding: 8px 12px;
  border-left: 3px solid var(--brand-1);
  background: var(--info-bg);
  border-radius: 0 10px 10px 0;
  color: var(--text-2);
}
.md :deep(table) {
  border-collapse: collapse;
  margin: 8px 0;
  font-size: 12.5px;
  width: 100%;
}
.md :deep(th),
.md :deep(td) {
  border-bottom: 1px solid var(--line);
  padding: 6px 8px;
  text-align: left;
}
.md :deep(code) {
  font-family: var(--mono);
  font-size: 12px;
  background: var(--hover);
  padding: 1px 5px;
  border-radius: 5px;
}
.md :deep(strong) {
  font-weight: 680;
}
.event {
  align-self: center;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 92%;
  padding: 5px 12px;
  border-radius: 999px;
  font-size: 12px;
  color: var(--text-2);
  background: var(--info-bg);
  border: 1px solid var(--line);
}
.msg.event {
  align-items: center;
}
.execs {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 8px;
}
.exec {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 7px 10px;
  border-radius: 12px;
  font-size: 12.5px;
  background: var(--glass-1);
  border: 1px solid var(--line);
}
.exec.done {
  border-color: color-mix(in srgb, var(--ok) 35%, transparent);
}
.exec.failed {
  border-color: color-mix(in srgb, var(--err) 40%, transparent);
  background: var(--err-bg);
}
.e-ico {
  width: 18px;
  height: 18px;
  flex: none;
  display: grid;
  place-items: center;
  border-radius: 50%;
  color: #fff;
  background: var(--brand);
}
.exec.done .e-ico {
  background: var(--ok);
}
.exec.failed .e-ico {
  background: var(--err);
}
.e-text {
  flex: 1;
  min-width: 0;
  line-height: 1.45;
}
.retry {
  border: none;
  background: none;
  color: var(--brand-ink);
  font-size: 12px;
  cursor: pointer;
  padding: 0 2px;
}
.e-text small {
  color: var(--text-3);
}
.e-text :deep(svg) {
  vertical-align: -2px;
  margin: 0 2px;
}
.spin {
  width: 11px;
  height: 11px;
  border-radius: 50%;
  border: 2px solid rgba(255, 255, 255, 0.45);
  border-top-color: #fff;
  animation: cp-spin 0.8s linear infinite;
}
@keyframes cp-spin {
  to {
    transform: rotate(360deg);
  }
}
.runcard {
  margin: 0 12px 8px;
  padding: 10px 12px;
  border-radius: 14px;
  background: var(--glass-2);
  border: 1px solid var(--line-2);
  box-shadow: var(--shadow-1);
}
.rc-head {
  display: flex;
  align-items: center;
  gap: 10px;
}
.rc-ico {
  width: 26px;
  height: 26px;
  flex: none;
  display: grid;
  place-items: center;
  border-radius: 9px;
  color: #fff;
  background: var(--brand-grad);
}
.rc-ico.done {
  background: var(--ok);
}
.rc-ico.error,
.rc-ico.cancelled,
.rc-ico.interrupted {
  background: var(--warn);
}
.rc-text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  line-height: 1.3;
}
.rc-text b {
  font-size: 12.5px;
}
.rc-text small {
  font-size: 11px;
  color: var(--text-3);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.rc-bar {
  height: 4px;
  margin: 8px 0 5px;
  border-radius: 4px;
  background: var(--line);
  overflow: hidden;
}
.rc-bar i {
  display: block;
  height: 100%;
  border-radius: 4px;
  background: var(--brand-grad);
  transition: width var(--t-slow) var(--ease-out);
}
.rc-meta {
  font-size: 11px;
  color: var(--text-3);
}
.acts {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 4px;
}
.caret {
  display: inline-block;
  width: 8px;
  height: 16px;
  border-radius: 2px;
  background: var(--brand);
  animation: pulse 0.9s infinite;
}
.thinking {
  display: inline-flex;
  gap: 4px;
  padding: 10px 0;
}
.thinking i {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--brand);
  animation: bounce 1s infinite;
}
.thinking i:nth-child(2) {
  animation-delay: 0.15s;
}
.thinking i:nth-child(3) {
  animation-delay: 0.3s;
}
@keyframes bounce {
  0%,
  80%,
  100% {
    transform: translateY(0);
    opacity: 0.4;
  }
  40% {
    transform: translateY(-6px);
    opacity: 1;
  }
}
.composer {
  border-top: 1px solid var(--line);
  padding: 10px 12px 12px;
}
.ctx {
  display: flex;
  gap: 6px;
  margin-bottom: 8px;
  flex-wrap: wrap;
}
.ctx .x {
  border: none;
  background: transparent;
  color: inherit;
  cursor: pointer;
  padding: 0;
  display: grid;
  place-items: center;
  opacity: 0.7;
}
.input-row {
  display: flex;
  gap: 8px;
  align-items: flex-end;
}
.input-row textarea {
  flex: 1;
  resize: none;
  border-radius: 14px;
  border: 1px solid var(--field-border);
  background: var(--field);
  padding: 10px 12px;
  outline: none;
  font-size: 13.5px;
  line-height: 1.5;
  max-height: 140px;
}
.input-row textarea:focus {
  border-color: color-mix(in srgb, var(--brand-1) 70%, transparent);
  box-shadow: 0 0 0 4px rgba(61, 123, 255, 0.14);
}
.send {
  width: 42px;
  height: 42px;
  border-radius: 13px;
  border: none;
  background: var(--brand-grad);
  color: #fff;
  display: grid;
  place-items: center;
  cursor: pointer;
  box-shadow: 0 6px 16px rgba(80, 100, 255, 0.35);
  transition: transform var(--t-mid) var(--spring), opacity var(--t-fast);
}
.send:active {
  transform: scale(0.9);
}
.send:disabled {
  opacity: 0.4;
  cursor: not-allowed;
  box-shadow: none;
}
</style>
