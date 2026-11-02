<script setup lang="ts">
/**
 * v26.03 登录门槛：用户指定的平台里有需要登录但没登录的，搜索前在这里处理。
 * - 每个平台：登录（打开官网，自动识别 + 账号页确认）/ 我已登录 / 取消
 * - "建议登录"的平台可以选择不登录也搜
 * - 移除这些平台继续搜索，或取消本次搜索
 */
import { computed, reactive, watch } from "vue";
import { toAppError } from "@/api";
import { LOGIN_LABEL, usePlatformStore } from "@/stores/platforms";
import { useUiStore } from "@/stores/ui";
import GSheet from "@/components/ui/GSheet.vue";
import Icon from "@/components/ui/Icon.vue";

const store = usePlatformStore();
const ui = useUiStore();
/** 选择"不登录也搜"的平台（仅限建议登录） */
const skipLogin = reactive<Record<string, boolean>>({});

const open = computed({
  get: () => !!store.gate,
  set: (v) => {
    if (!v) store.settleGate(null);
  },
});
const rows = computed(() => (store.gate?.checks ?? []).map((c) => ({ check: c, p: store.byId[c.platformId], st: store.logins[c.platformId] })));
watch(
  () => store.gate,
  () => {
    for (const k of Object.keys(skipLogin)) delete skipLogin[k];
  },
);

const inFlow = (id: string) => ["opening", "waiting", "verifying"].includes(store.logins[id]?.flow ?? "");
const rowOk = (id: string) => store.readyFor(id) || (store.byId[id]?.login === "recommended" && !!skipLogin[id]);
const pending = computed(() => rows.value.filter((r) => !rowOk(r.check.platformId)));
const allReady = computed(() => pending.value.length === 0);
const remaining = computed(() => (store.gate?.ids ?? []).filter((id) => !rows.value.some((r) => r.check.platformId === id) || rowOk(id)));

function tone(s?: string) {
  return s === "logged_in" ? "green" : s === "expired" ? "orange" : s === "logged_out" ? "red" : "";
}

async function act(fn: () => Promise<unknown>) {
  try {
    await fn();
  } catch (e) {
    ui.toast(toAppError(e).message, "error", 4000);
  }
}
async function confirmRow(id: string) {
  await act(async () => {
    const s = await store.loginConfirm(id);
    if (s.message && !inFlow(id)) ui.toast(s.message, s.status === "logged_in" ? "success" : "warn", 3200);
  });
}
function start() {
  if (!store.gate) return;
  if (!allReady.value) return;
  store.settleGate(store.gate.ids);
}
function removeBlocked() {
  const left = remaining.value;
  if (!left.length && !store.gate?.othersRemain) {
    ui.toast("移除后没有可以搜索的平台了", "warn", 3000, "请先登录，或在平台选择里换其他平台");
    return;
  }
  store.settleGate(left);
}
</script>

<template>
  <GSheet v-model:open="open" center :width="560" title="先登录再搜索" subtitle="你指定的平台里有需要登录的。登录在官网窗口里由你本人完成，Hawwwker 会自动识别并打开账号页确认。">
    <div class="rows">
      <div v-for="r in rows" :key="r.check.platformId" class="row-item" :class="{ ok: rowOk(r.check.platformId), flow: inFlow(r.check.platformId) }">
        <div class="top">
          <i class="glyph" :style="{ background: r.p?.color ?? '#888' }">{{ r.p?.glyph ?? "?" }}</i>
          <div class="name">
            <b>{{ store.label(r.check.platformId) }}</b>
            <small>{{ r.check.need === "required" ? "必须登录才能搜索" : "建议登录（结果更完整）" }}</small>
          </div>
          <span v-if="store.readyFor(r.check.platformId)" class="chip green"><Icon name="check" />已就绪</span>
          <span v-else class="chip" :class="tone(r.st?.status)">{{ LOGIN_LABEL[r.st?.status ?? "unknown"] }}</span>
        </div>
        <p v-if="r.st && (inFlow(r.check.platformId) || ['success', 'cancelled', 'timeout', 'failed'].includes(r.st.flow)) && r.st.message" class="msg" :class="r.st.flow">
          <Icon v-if="inFlow(r.check.platformId)" name="refresh" :size="13" class="spin" />
          <Icon v-else-if="r.st.flow === 'success'" name="check" :size="13" />
          <Icon v-else name="info" :size="13" />
          {{ r.st.message }}
        </p>
        <p v-else-if="!store.readyFor(r.check.platformId) && r.check.message" class="msg faint">{{ r.check.message }}</p>
        <div v-if="!store.readyFor(r.check.platformId)" class="acts">
          <template v-if="inFlow(r.check.platformId)">
            <button v-ripple class="btn sm primary" :disabled="r.st?.flow === 'verifying'" @click="confirmRow(r.check.platformId)"><Icon name="check" />我已登录</button>
            <button class="btn sm" @click="act(() => store.loginCancel(r.check.platformId))">取消</button>
          </template>
          <template v-else>
            <button v-ripple class="btn sm primary" @click="act(() => store.loginStart(r.check.platformId))">
              <Icon name="login" />{{ r.st?.status === "expired" ? "重新登录" : "登录" }}
            </button>
            <button class="btn sm ghost" title="已经在浏览器里登录过？打开账号页确认一次" @click="confirmRow(r.check.platformId)"><Icon name="shield" />我已登录</button>
          </template>
          <label v-if="r.check.need === 'recommended'" class="skip tiny">
            <input v-model="skipLogin[r.check.platformId]" type="checkbox" />不登录也搜（结果可能较少）
          </label>
        </div>
      </div>
    </div>
    <p class="faint tiny note">
      <Icon name="shield" :size="12" />每个平台独立登录、互不影响；会话只保存在本机（含加密备份），浏览器意外丢失登录时会自动恢复。
    </p>
    <template #foot>
      <button class="btn ghost" @click="store.settleGate(null)">取消搜索</button>
      <span class="spacer"></span>
      <button class="btn" :disabled="allReady" @click="removeBlocked">移除未登录平台</button>
      <button v-ripple class="btn primary" :disabled="!allReady" @click="start">
        <Icon name="search" />{{ allReady ? "全部就绪，开始搜索" : `还有 ${pending.length} 个平台未登录` }}
      </button>
    </template>
  </GSheet>
</template>

<style scoped>
.rows {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.row-item {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  border-radius: 16px;
  background: var(--glass-1);
  border: 1px solid var(--line);
  transition: border-color 300ms, box-shadow 300ms;
}
.row-item.flow {
  border-color: color-mix(in srgb, var(--brand-1) 50%, transparent);
  box-shadow: 0 0 0 4px rgba(61, 123, 255, 0.1);
}
.row-item.ok {
  border-color: rgba(47, 210, 122, 0.45);
}
.top {
  display: flex;
  align-items: center;
  gap: 10px;
}
.glyph {
  width: 32px;
  height: 32px;
  border-radius: 10px;
  display: grid;
  place-items: center;
  color: #fff;
  font-style: normal;
  font-weight: 700;
  flex: none;
}
.name {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  line-height: 1.3;
}
.name small {
  font-size: 11.5px;
  color: var(--text-3);
}
.msg {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  padding: 7px 10px;
  border-radius: 10px;
  background: var(--info-bg);
  color: var(--brand-ink);
}
.msg.faint {
  background: transparent;
  padding: 0;
  color: var(--text-3);
}
.msg.success {
  background: var(--ok-bg);
  color: var(--ok);
}
.msg.cancelled,
.msg.timeout,
.msg.failed {
  background: var(--warn-bg);
  color: var(--warn);
}
.acts {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}
.skip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  margin-left: auto;
  color: var(--text-2);
  cursor: pointer;
}
.note {
  display: flex;
  align-items: center;
  gap: 5px;
  margin-top: 14px;
}
.spacer {
  flex: 1;
}
</style>
