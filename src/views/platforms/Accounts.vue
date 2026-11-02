<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api, toAppError } from "@/api";
import type { BrowserInfo, LoginState, PlatformInfo } from "@/api/types";
import { LOGIN_LABEL, usePlatformStore } from "@/stores/platforms";
import { useUiStore } from "@/stores/ui";
import { ago } from "@/utils/format";
import GSegmented from "@/components/ui/GSegmented.vue";
import Icon from "@/components/ui/Icon.vue";

const store = usePlatformStore();
const ui = useUiStore();
const region = ref<"all" | "cn" | "intl">("all");
/** 默认只看已启用的平台；"待处理" = 需要/建议登录但未登录或已过期 */
const scope = ref<"enabled" | "todo" | "all">("enabled");
const q = ref("");
const browser = ref<BrowserInfo | null>(null);
const refreshing = ref(false);

onMounted(async () => {
  try {
    browser.value = await api.browserInfo();
  } catch {
    /* ignore */
  }
});

function needsAction(p: PlatformInfo): boolean {
  const s = store.logins[p.id]?.status;
  return s === "expired" || (s === "logged_out" && p.login !== "optional") || ["opening", "waiting", "verifying"].includes(store.logins[p.id]?.flow ?? "");
}
function matches(p: PlatformInfo): boolean {
  if (region.value !== "all" && p.region !== region.value) return false;
  if (scope.value === "enabled" && !p.enabled) return false;
  if (scope.value === "todo" && !needsAction(p)) return false;
  const k = q.value.trim().toLowerCase();
  return !k || `${p.name}${p.edition}${p.familyName}${p.id}`.toLowerCase().includes(k);
}
const families = computed(() =>
  store.families.map((f) => ({ ...f, editions: f.editions.filter(matches) })).filter((f) => f.editions.length),
);
const todoCount = computed(() => store.list.filter(needsAction).length);
const scopeOpts = computed(() => [
  { value: "enabled" as const, label: `已启用 ${store.list.filter((p) => p.enabled).length}` },
  { value: "todo" as const, label: `待处理 ${todoCount.value}` },
  { value: "all" as const, label: `全部 ${store.list.length}` },
]);
const stats = computed(() => {
  const ls = Object.values(store.logins);
  return {
    ok: ls.filter((l) => l.status === "logged_in").length,
    expired: ls.filter((l) => l.status === "expired").length,
    out: ls.filter((l) => l.status === "logged_out" && store.byId[l.platformId]?.login !== "optional").length,
  };
});

function st(p: PlatformInfo): LoginState {
  return store.logins[p.id] ?? { platformId: p.id, status: "unknown", flow: "idle" };
}
function tone(s: LoginState["status"]) {
  return s === "logged_in" ? "green" : s === "expired" ? "orange" : s === "logged_out" ? "red" : "";
}
const inFlow = (s: LoginState) => ["opening", "waiting", "verifying"].includes(s.flow);
const FLOW: Record<string, string> = {
  opening: "正在打开官网登录页…",
  waiting: "请在弹出的浏览器窗口中登录，完成后会自动识别（会打开账号页确认）",
  verifying: "检测到登录信息，正在确认…（请勿关闭窗口）",
  success: "登录成功，会话已保存",
  cancelled: "已取消（窗口已关闭），可以重新登录",
  timeout: "10 分钟内没有完成登录，可以重新尝试",
  failed: "登录检测失败",
};
/** 主动验证 / 刷新以来的时间说明 */
function checkedText(s: LoginState): string {
  const t = s.checkedAt ?? s.verifiedAt;
  if (!t) return "已检测到会话";
  return `${s.checkedAt ? "已验证" : "最近使用"} · ${ago(t)}`;
}

async function act(fn: () => Promise<void>) {
  try {
    await fn();
  } catch (e) {
    ui.toast(toAppError(e).message, "error", 4000);
  }
}
async function logout(p: PlatformInfo) {
  const ok = await ui.confirm(
    `退出 ${store.label(p.id)}？`,
    "将删除本机保存的该平台登录会话与加密备份，不影响其他平台。",
    "退出",
    true,
  );
  if (ok)
    await act(async () => {
      const s = await store.logout(p.id);
      if (s.message) ui.toast(s.message, "success", 3200);
    });
}
const verifying = ref<Record<string, boolean>>({});
async function verifyOne(p: PlatformInfo) {
  verifying.value = { ...verifying.value, [p.id]: true };
  await act(async () => {
    const s = await store.loginConfirm(p.id);
    if (s.message && !inFlow(s)) ui.toast(s.message, s.status === "logged_in" ? "success" : "warn", 3600);
  });
  verifying.value = { ...verifying.value, [p.id]: false };
}
/** 登录体检：主动验证所有已启用、需要登录的平台 */
async function checkAll() {
  await act(async () => {
    const states = await store.checkAll();
    const need = states.filter((s) => store.byId[s.platformId]?.enabled && store.byId[s.platformId]?.login !== "optional");
    const ok = need.filter((s) => s.status === "logged_in").length;
    const bad = need.length - ok;
    ui.toast(bad ? `登录体检：${ok} 个有效，${bad} 个需要登录` : `登录体检：${ok} 个平台登录都有效`, bad ? "warn" : "success", 4200, bad ? "可在「待处理」里一键登录" : undefined);
    if (bad) scope.value = "todo";
  });
}
async function refresh() {
  refreshing.value = true;
  await act(() => store.refreshLogins(true));
  refreshing.value = false;
  ui.toast("登录状态已刷新", "success", 1500);
}
async function wipe() {
  const ok = await ui.confirm("清除全部登录？", "会关闭自动化浏览器并删除本机浏览器目录，所有平台都需要重新登录。", "全部清除", true);
  if (!ok) return;
  await act(async () => {
    await api.browserWipe();
    await store.refreshLogins();
  });
  ui.toast("已清除全部登录", "success");
}
const regionOpts = [
  { value: "all" as const, label: "全部" },
  { value: "cn" as const, label: "国内版" },
  { value: "intl" as const, label: "国际版" },
];
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1>账号与登录</h1>
        <p>在官网登录一次，之后的搜索与盯价都复用这个会话。每个平台独立登录、互不影响；会话只保存在本机（含加密备份，浏览器意外丢失登录时自动恢复），Hawwwker 不上传。</p>
      </div>
    </div>

    <div class="row wrap">
      <GSegmented v-model="scope" :options="scopeOpts" small />
      <GSegmented v-model="region" :options="regionOpts" small />
      <div class="searchbox">
        <Icon name="search" :size="14" class="faint" />
        <input v-model="q" placeholder="找平台…" aria-label="搜索平台" />
        <button v-if="q" class="icon-btn" title="清除" @click="q = ''"><Icon name="x" :size="13" /></button>
      </div>
      <span class="spacer"></span>
      <span class="faint tiny stat-line" :title="`已登录 ${stats.ok} · 已过期 ${stats.expired} · 未登录 ${stats.out}`">
        <span class="dot green"></span>{{ stats.ok }}<span class="dot orange"></span>{{ stats.expired }}<span class="dot red"></span>{{ stats.out }}
      </span>
      <button class="btn sm" :disabled="refreshing" @click="refresh"><Icon name="refresh" :class="{ spin: refreshing }" />刷新状态</button>
      <button v-ripple class="btn sm primary" :disabled="store.checkingAll" title="打开每个已登录平台的账号页，确认登录真的有效" @click="checkAll">
        <Icon name="shield" :class="{ spin: store.checkingAll }" />{{ store.checkingAll ? "体检中…" : "登录体检" }}
      </button>
    </div>

    <div v-if="!families.length" class="empty glass">
      <div class="empty-icon"><Icon name="key" /></div>
      <h3>{{ scope === "todo" ? "没有需要处理的登录" : "没有符合条件的平台" }}</h3>
      <p v-if="scope !== 'all'">切换到「全部」查看所有 {{ store.list.length }} 个平台，或到「平台目录」启用更多平台。</p>
    </div>

    <div class="fam-grid">
      <section v-for="f in families" :key="f.family" class="fam glass" :class="{ multi: f.editions.length > 1 }">
        <header class="fam-head">
          <b>{{ f.name }}</b>
          <span v-if="f.editions.some((e) => e.region === 'cn') && f.editions.some((e) => e.region === 'intl')" class="chip purple">国内版 ⇄ 国际版</span>
          <span v-else class="faint tiny">{{ f.editions.length }} 个版本</span>
        </header>
        <div class="eds">
          <div v-for="p in f.editions" :key="p.id" class="ed" :class="[st(p).status, { flow: inFlow(st(p)) }]">
            <div class="ed-top">
              <i class="glyph" :style="{ background: p.color }">{{ p.glyph }}</i>
              <div class="ed-name">
                <b>{{ p.name }}</b>
                <small>{{ p.edition === (p.region === "cn" ? "国内版" : "国际版") ? p.edition : `${p.edition} · ${p.region === "cn" ? "国内版" : "国际版"}` }}<template v-if="p.beta"> · Beta</template></small>
              </div>
              <span class="chip" :class="tone(st(p).status)">{{ LOGIN_LABEL[st(p).status] }}</span>
            </div>

            <p class="ed-meta faint tiny">
              <template v-if="st(p).status === 'logged_in'">{{ checkedText(st(p)) }}<template v-if="st(p).cookieExpires"> · 会话约 {{ ago(st(p).cookieExpires) }}到期</template><span v-if="st(p).vaulted" class="vault" title="本机有加密的会话备份：浏览器意外丢失登录时会自动恢复"><Icon name="shield" :size="11" />已备份</span></template>
              <template v-else-if="st(p).status === 'expired'">{{ st(p).message ?? "最近一次搜索或验证遇到登录页，需要重新登录" }}</template>
              <template v-else-if="st(p).status === 'not_needed'">该平台无需登录即可搜索{{ p.login === "optional" ? "，登录后可看到会员价" : "" }}</template>
              <template v-else-if="p.login === 'required'">需要登录才能搜索</template>
              <template v-else>建议登录以获得完整结果</template>
            </p>

            <Transition name="blur">
              <div v-if="inFlow(st(p)) || ['cancelled', 'timeout', 'failed', 'success'].includes(st(p).flow)" class="flowbox" :class="st(p).flow">
                <Icon v-if="inFlow(st(p))" name="refresh" :size="14" class="spin" />
                <Icon v-else-if="st(p).flow === 'success'" name="check" :size="14" />
                <Icon v-else name="info" :size="14" />
                <span>{{ st(p).message || FLOW[st(p).flow] }}</span>
              </div>
            </Transition>

            <div class="ed-actions">
              <template v-if="inFlow(st(p))">
                <button v-ripple class="btn sm primary" :disabled="st(p).flow === 'verifying'" @click="verifyOne(p)"><Icon name="check" />我已登录</button>
                <button class="btn sm" @click="act(() => store.loginCancel(p.id))">取消</button>
              </template>
              <template v-else-if="st(p).status === 'logged_in'">
                <button class="btn sm" :disabled="verifying[p.id]" title="打开账号页确认登录仍然有效" @click="verifyOne(p)"><Icon name="shield" :class="{ spin: verifying[p.id] }" />{{ verifying[p.id] ? "验证中…" : "验证" }}</button>
                <button class="btn sm ghost danger" @click="logout(p)"><Icon name="logout" />退出</button>
              </template>
              <template v-else>
                <button v-ripple class="btn sm" :class="{ primary: st(p).status !== 'not_needed' }" @click="act(() => store.loginStart(p.id))">
                  <Icon name="login" />{{ st(p).status === "expired" ? "重新登录" : st(p).status === "not_needed" ? "登录（可选）" : "登录" }}
                </button>
              </template>
            </div>
          </div>
        </div>
      </section>
    </div>

    <section class="glass card browser">
      <div class="card-head">
        <span class="b-ico"><Icon name="monitor" :size="18" /></span>
        <h3>自动化浏览器</h3>
        <span class="chip" :class="browser?.running ? 'green' : ''">{{ browser?.running ? "运行中" : "未运行" }}</span>
        <span class="spacer"></span>
        <button class="btn sm" @click="act(() => api.browserShow())"><Icon name="eye" />显示窗口</button>
        <button class="btn sm danger" @click="wipe"><Icon name="trash" />清除全部登录</button>
      </div>
      <div class="kv">
        <span>浏览器</span><code class="selectable">{{ browser?.executable ?? "未找到 Edge / Chrome，请先安装其中之一" }}</code>
        <span>会话目录</span><code class="selectable">{{ browser?.profileDir ?? "—" }}</code>
        <template v-if="browser?.transport">
        <span>通信方式</span>
        <span>
          <span class="chip" :class="browser?.transport === 'port' ? 'orange' : 'green'">
            <Icon :name="browser?.transport === 'port' ? 'warn' : 'shield'" />{{ browser?.transport === "port" ? "本地端口（回退模式）" : "管道通信 · 不开放端口" }}
          </span>
        </span>
        </template>
      </div>
      <p class="faint tiny" style="margin-top: 10px">
        登录由你本人在官网完成，Hawwwker 拿不到密码；只有离开登录 / 验证页面、并打开账号页确认后才算登录成功。遇到验证码或安全验证时会把窗口移到屏幕上等你处理，不做任何绕过。退出某个平台只删除该平台的会话，不影响其他平台。
      </p>
    </section>
  </div>
</template>

<style scoped>
.searchbox {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 4px 0 10px;
  border-radius: 10px;
  border: 1px solid var(--field-border);
  background: var(--field);
  width: 180px;
}
.searchbox input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  font-size: 12.5px;
}
.searchbox .icon-btn {
  width: 24px;
  height: 24px;
}
.stat-line {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.stat-line .dot {
  margin-left: 6px;
}
.fam-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(360px, 1fr));
  grid-auto-flow: row dense;
  gap: 14px;
  align-items: start;
}
.fam.multi {
  grid-column: span 2;
}
@media (max-width: 1180px) {
  .fam.multi {
    grid-column: span 1;
  }
}
.fam {
  padding: 14px;
}
.fam-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 4px 10px;
}
.fam-head b {
  font-size: 15px;
}
.eds {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  gap: 10px;
}
.ed {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  border-radius: 16px;
  background: var(--glass-1);
  border: 1px solid var(--line);
  transition: border-color 300ms, box-shadow 300ms;
}
.ed.flow {
  border-color: color-mix(in srgb, var(--brand-1) 50%, transparent);
  box-shadow: 0 0 0 4px rgba(61, 123, 255, 0.1);
}
.ed.expired {
  border-color: rgba(255, 159, 10, 0.4);
}
.ed-top {
  display: flex;
  align-items: center;
  gap: 10px;
}
.glyph {
  width: 34px;
  height: 34px;
  border-radius: 11px;
  display: grid;
  place-items: center;
  color: #fff;
  font-style: normal;
  font-weight: 700;
  font-size: 15px;
  flex: none;
}
.ed-name {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  line-height: 1.3;
}
.ed-name small {
  font-size: 11.5px;
  color: var(--text-3);
}
.ed-meta {
  min-height: 18px;
}
.vault {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  margin-left: 8px;
  color: var(--ok);
}
.flowbox {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 10px;
  font-size: 12px;
  background: var(--info-bg);
  color: var(--brand-ink);
}
.flowbox.success {
  background: var(--ok-bg);
  color: var(--ok);
}
.flowbox.cancelled,
.flowbox.timeout,
.flowbox.failed {
  background: var(--warn-bg);
  color: var(--warn);
}
.ed-actions {
  display: flex;
  gap: 6px;
  margin-top: auto;
}
.b-ico {
  width: 32px;
  height: 32px;
  border-radius: 10px;
  display: grid;
  place-items: center;
  background: var(--info-bg);
  color: var(--brand-ink);
}
.kv {
  display: grid;
  grid-template-columns: 80px 1fr;
  gap: 6px 12px;
  font-size: 12.5px;
  color: var(--text-2);
}
.kv code {
  font-family: var(--mono);
  font-size: 12px;
  color: var(--text);
  word-break: break-all;
}
</style>
