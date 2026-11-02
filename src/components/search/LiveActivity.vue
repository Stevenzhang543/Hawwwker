<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { STAGES, type RunState } from "@/stores/search";
import { usePlatformStore } from "@/stores/platforms";
import { PSTATE, DROP_LABEL, RUN_STATUS } from "@/utils/labels";
import { duration } from "@/utils/format";
import Icon from "@/components/ui/Icon.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";

/** slim = 已有结果时收成一行状态栏（可展开为完整面板） */
const props = defineProps<{ run: RunState; slim?: boolean }>();
const emit = defineEmits<{ cancel: []; showBrowser: [] }>();
const platforms = usePlatformStore();
const router = useRouter();
const now = ref(Date.now());
let timer = 0;
onMounted(() => (timer = window.setInterval(() => (now.value = Date.now()), 100)));
onBeforeUnmount(() => clearInterval(timer));

const running = computed(() => props.run.status === "running");
const elapsed = computed(() => (props.run.finishedAt ?? now.value) - props.run.startedAt);
const stageIdx = computed(() => (props.run.stage === "done" ? STAGES.length : STAGES.findIndex((s) => s.key === props.run.stage)));
const plist = computed(() => Object.entries(props.run.platforms));
const needLogin = computed(() => plist.value.filter(([, p]) => p.state === "login_required"));
const waiting = computed(() => plist.value.some(([, p]) => p.state === "waiting_user"));
const title = computed(() => {
  const r = props.run;
  if (r.status === "done") return "搜索完成";
  if (r.status === "error") return "搜索失败";
  if (r.status === "cancelled") return "已取消";
  if (r.status === "interrupted") return "已中断";
  return r.stageLabel || "处理中…";
});
const expanded = ref(false);
const compact = computed(() => !!props.slim && !expanded.value);
const doneCount = computed(() => plist.value.filter(([, p]) => !["queued", "searching", "waiting_user"].includes(p.state)).length);
const partialCount = computed(() => Object.values(props.run.partial).reduce((n, l) => n + l.length, 0));
const line = computed(() => {
  const r = props.run;
  const n = plist.value.length;
  if (r.status === "running") {
    const parts = [`${doneCount.value}/${n} 个平台完成`];
    if (partialCount.value && !r.results.length) parts.push(`已出 ${partialCount.value} 条初步结果`);
    if (r.verify) parts.push(`AI 确认 ${r.verify.matched} 条`);
    return parts.join(" · ");
  }
  const parts = [`${n} 个平台`];
  if (r.filter) parts.push(`抓取 ${r.filter.fetched}`);
  if (r.verify) parts.push(`AI 确认匹配 ${r.verify.matched}`);
  if (r.verify?.hidden) parts.push(`折叠 ${r.verify.hidden}`);
  if (r.stats?.aiCalls) parts.push(`AI 调用 ${r.stats.aiCalls} 次`);
  return parts.join(" · ");
});
const dropText = computed(() => {
  const d = props.run.filter?.dropped ?? {};
  return Object.entries(d)
    .filter(([, v]) => v)
    .map(([k, v]) => `${DROP_LABEL[k] ?? k} ${v}`)
    .join(" · ");
});
</script>

<template>
  <section v-if="compact" class="la slim glass" :class="[run.status]">
    <div class="slim-row">
      <div class="orb sm" :class="{ live: running }">
        <Icon v-if="run.status === 'done'" name="check" :size="14" :stroke="2.8" />
        <Icon v-else-if="run.status === 'error'" name="warn" :size="13" />
        <Icon v-else-if="!running" name="stop" :size="12" />
      </div>
      <b class="slim-title">{{ title }}</b>
      <span class="muted small ellipsis slim-line">{{ line }}</span>
      <span class="mini-pills">
        <span v-for="[id, p] in plist" :key="id" class="mp" :class="p.state" :title="`${platforms.label(id)}：${p.state === 'done' ? p.count + ' 条' : PSTATE[p.state].label}`">
          <PlatformBadge :id="id" :show-name="false" />
          <i></i>
        </span>
      </span>
      <button v-if="needLogin.length" class="chip orange click" @click="router.push('/platforms/accounts')"><Icon name="key" />{{ needLogin.length }} 个平台需登录</button>
      <span class="spacer"></span>
      <span class="time num">{{ duration(elapsed) }}</span>
      <button v-if="running" class="btn sm ghost" title="把自动化浏览器窗口移到屏幕上" @click="emit('showBrowser')"><Icon name="monitor" />浏览器</button>
      <button v-if="running" v-ripple class="btn sm danger" @click="emit('cancel')"><Icon name="stop" />取消</button>
      <button class="icon-btn" title="展开搜索过程" @click="expanded = true"><Icon name="down" :size="16" /></button>
    </div>
    <div v-if="running" class="slim-bar"><i :style="{ width: Math.max(3, run.progress * 100) + '%' }"></i></div>
    <div v-if="waiting" class="banner" style="margin-top: 8px">
      <Icon name="warn" />有平台需要你在浏览器窗口中完成登录或安全验证，完成后会自动继续。
      <span class="spacer"></span>
      <button class="btn sm" @click="emit('showBrowser')">显示浏览器</button>
    </div>
    <div v-if="run.error" class="banner" style="margin-top: 8px; background: var(--err-bg); color: var(--err)"><Icon name="warn" />{{ run.error }}</div>
  </section>

  <section v-else class="la glass" :class="[run.status]">
    <div class="la-head">
      <div class="orb" :class="{ live: running }">
        <Icon v-if="run.status === 'done'" name="check" :size="20" :stroke="2.6" />
        <Icon v-else-if="run.status === 'error'" name="warn" :size="18" />
        <Icon v-else-if="!running" name="stop" :size="16" />
      </div>
      <div class="la-title">
        <h3>{{ title }}<span v-if="!running" class="chip" :class="RUN_STATUS[run.status].tone" style="margin-left: 8px">{{ RUN_STATUS[run.status].label }}</span></h3>
        <p class="muted small ellipsis">{{ run.instruction || run.plan?.instruction }}</p>
      </div>
      <span class="time num">{{ duration(elapsed) }}</span>
      <button v-if="running" class="btn sm" title="把自动化浏览器窗口移到屏幕上" @click="emit('showBrowser')"><Icon name="monitor" />显示浏览器</button>
      <button v-if="running" v-ripple class="btn sm danger" @click="emit('cancel')"><Icon name="stop" />取消</button>
      <button v-if="slim" class="icon-btn" title="收起" @click="expanded = false"><Icon name="up" :size="16" /></button>
    </div>

    <template v-if="run.status !== 'done'">
    <div class="bar"><i :style="{ width: Math.max(3, run.progress * 100) + '%' }" :class="{ live: running }"></i></div>

    <div class="stages">
      <span v-for="(s, i) in STAGES" :key="s.key" class="st" :class="{ done: i < stageIdx, on: i === stageIdx && running }">
        <i><Icon v-if="i < stageIdx" name="check" :size="11" :stroke="3" /></i>{{ s.label }}
      </span>
    </div>
    </template>

    <div v-if="plist.length" class="pills">
      <div v-for="[id, p] in plist" :key="id" class="pill" :class="p.state" :title="p.message ?? ''">
        <PlatformBadge :id="id" />
        <span class="ps">
          <Icon v-if="p.state === 'searching'" name="refresh" :size="12" class="spin" />
          <template v-if="p.state === 'done'">{{ p.count }} 条</template>
          <template v-else>{{ PSTATE[p.state].label }}</template>
        </span>
        <span v-if="p.ms && p.state === 'done'" class="pms">{{ duration(p.ms) }}</span>
      </div>
    </div>

    <div v-if="waiting" class="banner" style="margin-top: 10px">
      <Icon name="warn" />有平台需要你在浏览器窗口中完成登录或安全验证，完成后会自动继续。
      <span class="spacer"></span>
      <button class="btn sm" @click="emit('showBrowser')">显示浏览器</button>
    </div>
    <div v-if="needLogin.length && !running" class="banner info" style="margin-top: 10px">
      <Icon name="key" />{{ needLogin.map(([id]) => platforms.label(id)).join("、") }} 未登录，本次已跳过（其他平台不受影响）。
      <span class="spacer"></span>
      <button class="btn sm" @click="router.push('/platforms/accounts')">去登录（只需一次）</button>
    </div>
    <div v-if="run.error" class="banner" style="margin-top: 10px; background: var(--err-bg); color: var(--err)"><Icon name="warn" />{{ run.error }}</div>

    <div v-if="run.filter || run.verify" class="facts">
      <span v-if="run.filter"><b class="num">{{ run.filter.fetched }}</b> 条抓取</span>
      <span v-if="run.filter"><b class="num">{{ run.filter.kept }}</b> 条通过过滤</span>
      <span v-if="run.verify"><b class="num">{{ run.verify.matched }}</b> 条 AI 确认匹配</span>
      <span v-if="run.verify && run.verify.hidden"><b class="num">{{ run.verify.hidden }}</b> 条不相关已折叠</span>
      <span v-if="run.filter?.duplicates"><b class="num">{{ run.filter.duplicates }}</b> 条重复合并</span>
      <span v-if="dropText" class="faint">过滤：{{ dropText }}</span>
    </div>
  </section>
</template>

<style scoped>
.la {
  padding: 16px 18px 14px;
}
.la.slim {
  padding: 8px 12px;
  position: relative;
  overflow: hidden;
}
.slim-row {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}
.orb.sm {
  width: 26px;
  height: 26px;
}
.orb.sm.live::after {
  inset: 4px;
}
.slim-title {
  font-size: 13.5px;
  white-space: nowrap;
}
.slim-line {
  min-width: 0;
  flex: 0 1 auto;
}
.mini-pills {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  flex: none;
}
.mp {
  position: relative;
  display: inline-flex;
}
.mp i {
  position: absolute;
  right: -1px;
  bottom: -1px;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  border: 1.5px solid var(--bg-solid);
  background: var(--text-3);
}
.mp.done i {
  background: var(--ok);
}
.mp.searching i {
  background: var(--brand);
  animation: pulse 0.9s infinite;
}
.mp.error i {
  background: var(--err);
}
.mp.login_required i,
.mp.waiting_user i {
  background: var(--warn);
}
.slim-bar {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  height: 2px;
  background: var(--hover);
}
.slim-bar i {
  display: block;
  height: 100%;
  background: linear-gradient(90deg, #3d7bff, #8a5cff, #ffb020, #3d7bff);
  background-size: 300% 100%;
  animation: flow 2s linear infinite;
  transition: width 600ms var(--spring-soft);
}
.la-head {
  display: flex;
  align-items: center;
  gap: 12px;
}
.orb {
  position: relative;
  width: 40px;
  height: 40px;
  flex: none;
  border-radius: 50%;
  display: grid;
  place-items: center;
  color: #fff;
  background: var(--text-3);
}
.done .orb {
  background: linear-gradient(135deg, #2fd27a, #1aa85c);
  animation: popin 460ms var(--spring);
}
.error .orb {
  background: linear-gradient(135deg, #ff6b6b, #e5383d);
}
.orb.live {
  background: conic-gradient(from 0deg, #3d7bff, #8a5cff, #ffb020, #3d7bff);
  animation: spin 2.4s linear infinite;
}
.orb.live::after {
  content: "";
  position: absolute;
  inset: 5px;
  border-radius: 50%;
  background: var(--glass-3);
}
.la-title {
  flex: 1;
  min-width: 0;
}
.la-title h3 {
  font-size: 15.5px;
  display: flex;
  align-items: center;
}
.time {
  font-size: 13px;
  color: var(--text-3);
  min-width: 52px;
  text-align: right;
}
.bar {
  height: 5px;
  border-radius: 3px;
  background: var(--hover);
  margin: 14px 0 10px;
  overflow: hidden;
}
.bar i {
  display: block;
  height: 100%;
  border-radius: 3px;
  background: var(--brand-grad);
  transition: width 600ms var(--spring-soft);
}
.bar i.live {
  background: linear-gradient(90deg, #3d7bff, #8a5cff, #ffb020, #3d7bff);
  background-size: 300% 100%;
  animation: flow 2s linear infinite;
}
@keyframes flow {
  to {
    background-position: 300% 0;
  }
}
.stages {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}
.st {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px 4px 5px;
  border-radius: 999px;
  font-size: 12px;
  color: var(--text-3);
  transition: all 320ms var(--spring);
}
.st i {
  width: 17px;
  height: 17px;
  border-radius: 50%;
  border: 1.5px solid var(--line-2);
  display: grid;
  place-items: center;
}
.st.done {
  color: var(--text-2);
}
.st.done i {
  background: var(--ok);
  border-color: transparent;
  color: #fff;
}
.st.on {
  color: var(--brand-ink);
  background: var(--info-bg);
  font-weight: 620;
}
.st.on i {
  border-color: var(--brand-1);
  border-top-color: transparent;
  animation: spin 0.9s linear infinite;
}
.pills {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 12px;
}
.done .pills {
  margin-top: 14px;
}
.pill {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  height: 30px;
  padding: 0 10px 0 6px;
  border-radius: 10px;
  background: var(--hover);
  border: 1px solid transparent;
  font-size: 12px;
  animation: rise 420ms var(--spring-soft) both;
  transition: background 300ms, border-color 300ms;
}
.ps {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-weight: 600;
  color: var(--text-3);
}
.pms {
  color: var(--text-3);
  font-size: 11px;
}
.pill.searching {
  border-color: color-mix(in srgb, var(--brand-1) 35%, transparent);
  background: var(--info-bg);
}
.pill.searching .ps {
  color: var(--brand-ink);
}
.pill.done .ps {
  color: var(--ok);
}
.pill.error .ps {
  color: var(--err);
}
.pill.login_required,
.pill.waiting_user {
  background: var(--warn-bg);
}
.pill.login_required .ps,
.pill.waiting_user .ps {
  color: var(--warn);
}
.facts {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 16px;
  margin-top: 12px;
  font-size: 12.5px;
  color: var(--text-2);
}
.facts b {
  color: var(--text);
  font-size: 13.5px;
}
</style>
