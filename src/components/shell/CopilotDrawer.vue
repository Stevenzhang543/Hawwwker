<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { useUiStore } from "@/stores/ui";
import { useCopilotStore } from "@/stores/copilot";
import { useSearchStore } from "@/stores/search";
import { useWatchStore } from "@/stores/watch";
import { useAppStore } from "@/stores/app";
import { openExternal, toAppError } from "@/api";
import type { CopilotAction } from "@/api/types";
import { handleMdClick, renderMarkdown, stripActions } from "@/utils/markdown";
import Icon from "@/components/ui/Icon.vue";

const ui = useUiStore();
const cp = useCopilotStore();
const search = useSearchStore();
const watchStore = useWatchStore();
const app = useAppStore();
const router = useRouter();
const text = ref("");
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
  cp.skill = cp.skill === id ? null : id;
  const s = cp.skills.find((x) => x.id === id);
  if (cp.skill && s && !text.value) text.value = s.starter;
  inputEl.value?.focus();
}

async function act(a: CopilotAction) {
  try {
    if (a.type === "search") {
      await router.push("/discover/search");
      await search.start(a.instruction);
      ui.toast("已按 Copilot 建议开始搜索", "ai", 2200, a.instruction);
    } else if (a.type === "watch") {
      await watchStore.add({ listingId: a.listingId, targetPrice: a.targetPrice ?? null });
      ui.toast("已加入价格追踪", "success");
    } else if (a.type === "open") {
      await openExternal(a.url);
    } else if (a.type === "navigate") {
      await router.push(a.route);
    }
  } catch (e) {
    ui.toast(toAppError(e).message, "error", 4000);
  }
}
const actIcon: Record<CopilotAction["type"], string> = { search: "search", watch: "tag", open: "external", navigate: "arrow" };

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
          <small>{{ app.ai.model || "未连接模型" }} · 回答中的动作需要你点击才会执行</small>
        </div>
        <button class="icon-btn" title="清空对话" @click="reset"><Icon name="refresh" :size="16" /></button>
        <button class="icon-btn" title="关闭（Esc）" @click="ui.copilotOpen = false"><Icon name="x" /></button>
      </header>

      <div class="skills">
        <button v-for="s in cp.sortedSkills" :key="s.id" v-ripple class="skill" :class="{ on: cp.skill === s.id }" :title="s.description" @click="pickSkill(s.id)">
          <Icon :name="s.icon" :size="15" />{{ s.name }}
        </button>
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
          <template v-else>
            <div class="md selectable" v-html="renderMarkdown(stripActions(m.content))"></div>
            <div v-if="m.actions.length" class="acts">
              <button v-for="(a, i) in m.actions" :key="i" v-ripple class="btn sm" @click="act(a)">
                <Icon :name="actIcon[a.type]" :size="14" />{{ a.label }}
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

      <footer class="composer">
        <div v-if="cp.context || activeSkill" class="ctx">
          <span v-if="activeSkill" class="chip purple"><Icon :name="activeSkill.icon" />{{ activeSkill.name }}<button class="x" @click="cp.skill = null"><Icon name="x" :size="11" /></button></span>
          <span v-if="cp.context" class="chip blue"><Icon name="tag" />{{ cp.context.label }}<button class="x" @click="cp.setContext(null)"><Icon name="x" :size="11" /></button></span>
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
