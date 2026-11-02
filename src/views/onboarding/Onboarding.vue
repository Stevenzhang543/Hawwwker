<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { api, toAppError } from "@/api";
import type { LoginNeed, Recommendation, Region, Role } from "@/api/types";
import { ROLE_HOME, ROLE_LABEL, useAppStore } from "@/stores/app";
import { usePlatformStore } from "@/stores/platforms";
import { useUiStore } from "@/stores/ui";
import { CURRENCIES } from "@/utils/format";
import AiConfigForm from "@/components/settings/AiConfigForm.vue";
import BrandMark from "@/components/shell/BrandMark.vue";
import GSegmented from "@/components/ui/GSegmented.vue";
import Icon from "@/components/ui/Icon.vue";

const app = useAppStore();
const platforms = usePlatformStore();
const ui = useUiStore();
const router = useRouter();
const step = ref(0);
const dir = ref<"fwd" | "back">("fwd");
const role = ref<Role>(app.settings?.role ?? "consumer");
const currency = ref(app.settings?.displayCurrency ?? "CNY");
const region = ref<"all" | Region>("all");
const finishing = ref(false);

const STEPS = ["欢迎", "角色", "连接 AI", "平台", "完成"];
const canNext = computed(() => (step.value === 2 ? app.ai.verified : true));

function go(n: number) {
  dir.value = n > step.value ? "fwd" : "back";
  step.value = n;
}
function next() {
  if (!canNext.value) {
    ui.toast("请先完成 AI 连接测试", "warn", 2400, "Hawwwker 的搜索、校验与总结都依赖 AI");
    return;
  }
  go(Math.min(STEPS.length - 1, step.value + 1));
}

const ROLES: { r: Role; icon: string; title: string; desc: string; flow: string }[] = [
  { r: "consumer", icon: "cart", title: "我是消费者", desc: "搜索好物、跨平台比价、盯价等降价，最后跳转到平台下单", flow: "配置平台 → 搜索 / 盯价 → 跳转平台或联系商家 → 购买" },
  { r: "merchant", icon: "shop", title: "我是商户", desc: "盯竞品价格、看价格带、对比货源进价与零售价", flow: "配置平台 → 监视价格走势 → 调价 / 选品决策" },
  { r: "analyst", icon: "chart", title: "我做平台 / 市场分析", desc: "定时监控品类，分析平台价差、价格分布与趋势", flow: "配置平台 → 监视价格走势 → 分析报告" },
];

/* 第 4 步：按所在地区推荐平台（首次进入时自动选中推荐项） */
const rec = ref<Recommendation | null>(null);
const recApplied = ref(false);
const tz = (() => {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "";
  } catch {
    return "";
  }
})();
watch(step, async (n) => {
  if (n !== 3 || rec.value) return;
  try {
    rec.value = await api.platformsRecommend(tz, navigator.language || "");
    if (rec.value.region === "cn") region.value = "cn";
    await applyRec(false);
  } catch {
    /* 推荐失败不影响手动选择 */
  }
});
const recSet = computed(() => new Set(rec.value?.ids ?? []));
async function applyRec(only: boolean) {
  if (!rec.value) return;
  const jobs = platforms.list
    .filter((p) => (recSet.value.has(p.id) && !p.enabled) || (only && !recSet.value.has(p.id) && p.enabled && !p.custom))
    .map((p) => platforms.update(p.id, { enabled: recSet.value.has(p.id) }));
  try {
    await Promise.all(jobs);
    recApplied.value = true;
    if (only) ui.toast(`已只保留 ${recSet.value.size} 个推荐平台`, "success", 2200);
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
const LOGIN_BADGE: Record<LoginNeed, [string, string]> = { required: ["需登录", "orange"], recommended: ["建议登录", "blue"], optional: ["免登录", "green"] };

const pool = computed(() =>
  platforms.list
    .filter((p) => !p.custom && (region.value === "all" || p.region === region.value))
    .sort((a, b) => Number(recSet.value.has(b.id)) - Number(recSet.value.has(a.id))),
);
async function togglePlatform(id: string, v: boolean) {
  try {
    await platforms.update(id, { enabled: v });
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}

async function finish() {
  finishing.value = true;
  try {
    await app.update({ onboarded: true, role: role.value, displayCurrency: currency.value, search: { defaultRegion: region.value } });
    ui.toast(`欢迎使用 Hawwwker，${ROLE_LABEL[role.value]}工作台已就绪`, "success", 3200);
    void router.replace(ROLE_HOME[role.value]);
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  } finally {
    finishing.value = false;
  }
}
const regionOpts = [
  { value: "all" as const, label: "国内 + 国际" },
  { value: "cn" as const, label: "只用国内版" },
  { value: "intl" as const, label: "只用国际版" },
];
</script>

<template>
  <div class="ob">
    <div class="ob-card glass-2">
      <header class="ob-head">
        <BrandMark :size="30" />
        <div class="dots">
          <button v-for="(s, i) in STEPS" :key="s" class="dot-step" :class="{ on: i === step, done: i < step }" :disabled="i > step" @click="go(i)">
            <i>
              <Icon v-if="i < step" name="check" :size="11" :stroke="3" />
              <template v-else>{{ i + 1 }}</template>
            </i>
            <span>{{ s }}</span>
          </button>
        </div>
      </header>

      <div class="ob-body">
        <Transition :name="dir === 'fwd' ? 'slide-l' : 'slide-r'" mode="out-in">
          <section v-if="step === 0" key="0" class="pane pane-welcome">
            <div class="hero-mark breathe"><BrandMark :size="92" /></div>
            <h1>欢迎来到 <span class="grad">Hawwwker 货客</span></h1>
            <p class="muted lead">你说要什么，它就在你登录好的各个平台上替你搜、替你比、替你盯，最后把你带回平台下单或找商家。</p>
            <div class="feats">
              <div class="feat"><Icon name="spark" /><b>AI 规划与校验</b><span>本地化搜索词，逐条判断是否真的是你要的那款</span></div>
              <div class="feat"><Icon name="globe" /><b>国内版 + 国际版</b><span>小红书 ⇄ RedNote、拼多多 ⇄ Temu、Amazon 各国站…</span></div>
              <div class="feat"><Icon name="tag" /><b>盯价与提醒</b><span>目标价、降价、新品，系统通知第一时间告诉你</span></div>
              <div class="feat"><Icon name="shield" /><b>全程在你掌控</b><span>在你自己的浏览器会话中运行，可见、可暂停、可取消</span></div>
            </div>
          </section>

          <section v-else-if="step === 1" key="1" class="pane">
            <h2>你主要用 Hawwwker 做什么？</h2>
            <p class="muted">决定默认首页与 Copilot 技能排序，随时可以在侧栏切换，不限制任何功能。</p>
            <div class="roles">
              <button v-for="x in ROLES" :key="x.r" v-tilt="3" class="role glass" :class="{ on: role === x.r }" @click="role = x.r">
                <span class="r-ico"><Icon :name="x.icon" :size="22" /></span>
                <b>{{ x.title }}</b>
                <span class="muted small">{{ x.desc }}</span>
                <span class="flow tiny">{{ x.flow }}</span>
                <span class="r-ck"><Icon name="check" :size="13" :stroke="3" /></span>
              </button>
            </div>
            <label class="field" style="max-width: 280px; margin-top: 18px">
              <span>显示货币（其他币种价格会换算显示）</span>
              <select v-model="currency" class="select">
                <option v-for="c in CURRENCIES" :key="c.code" :value="c.code">{{ c.code }} · {{ c.name }}</option>
              </select>
            </label>
          </section>

          <section v-else-if="step === 2" key="2" class="pane">
            <h2>连接 AI 模型 <span class="chip red" style="vertical-align: 3px">必需</span></h2>
            <p class="muted">Hawwwker 的搜索规划、结果校验、总结与 Copilot 都由 AI 完成。选择服务商、填入 Key，测试通过后即可继续。</p>
            <AiConfigForm compact auto-detect style="margin-top: 14px" @verified="ui.toast('AI 连接成功', 'success')" />
          </section>

          <section v-else-if="step === 3" key="3" class="pane">
            <h2>选择要使用的平台</h2>
            <p class="muted">之后可以在「平台目录」随时调整。标「需登录」的平台在「账号与登录」里登录一次即可长期使用；「免登录」的平台装好就能搜。</p>
            <div v-if="rec" class="rec">
              <Icon name="compass" :size="18" />
              <span class="rec-text"><b>{{ rec.reason }}</b><small>{{ tz ? `按你的时区 ${tz} 推荐` : "按系统语言推荐" }} · 已为你选中 {{ rec.ids.length }} 个推荐平台</small></span>
              <button class="btn sm" @click="applyRec(true)">只保留推荐</button>
            </div>
            <div class="row" style="margin: 12px 0">
              <GSegmented v-model="region" :options="regionOpts" small />
              <span class="spacer"></span>
              <span class="faint tiny">已启用 {{ platforms.list.filter((p) => p.enabled).length }} 个</span>
            </div>
            <div class="plats">
              <button v-for="p in pool" :key="p.id" class="plat" :class="{ on: p.enabled }" @click="togglePlatform(p.id, !p.enabled)">
                <i class="glyph" :style="{ background: p.color }">{{ p.glyph }}</i>
                <span class="pl-name">
                  <b>{{ p.name }}<Icon v-if="recSet.has(p.id)" name="star" :size="11" class="rec-star" /></b>
                  <small><em class="need" :class="LOGIN_BADGE[p.login][1]">{{ LOGIN_BADGE[p.login][0] }}</em>{{ p.edition }}</small>
                </span>
                <span class="pl-ck"><Icon name="check" :size="12" :stroke="3" /></span>
              </button>
            </div>
          </section>

          <section v-else key="4" class="pane pane-done">
            <svg class="check-anim big-check" width="84" height="84" viewBox="0 0 34 34"><circle cx="17" cy="17" r="16" fill="#2fd27a" /><path d="M10 17.5l4.6 4.6L24.5 12" fill="none" stroke="#fff" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" /></svg>
            <h1>一切就绪</h1>
            <p class="muted lead">{{ ROLE_LABEL[role] }}工作台 · AI：{{ app.ai.model }} · 显示货币 {{ currency }} · {{ platforms.list.filter((p) => p.enabled).length }} 个平台</p>
            <div class="tips">
              <div><kbd>Ctrl K</kbd> 随时输入指令搜索</div>
              <div><kbd>Ctrl J</kbd> 打开 Copilot</div>
              <div><Icon name="key" :size="14" /> 需要登录的平台：账号与登录 → 登录一次</div>
            </div>
          </section>
        </Transition>
      </div>

      <footer class="ob-foot">
        <button v-if="step > 0" class="btn ghost" @click="go(step - 1)"><Icon name="back" />上一步</button>
        <span class="spacer"></span>
        <span v-if="step === 2 && !app.ai.verified" class="faint small">测试通过后才能继续</span>
        <button v-if="step < STEPS.length - 1" v-ripple v-magnet="0.15" class="btn primary lg" :disabled="!canNext" @click="next">
          {{ step === 0 ? "开始设置" : "下一步" }}<Icon name="arrow" />
        </button>
        <button v-else v-ripple class="btn primary lg" :disabled="finishing" @click="finish">进入 Hawwwker<Icon name="arrow" /></button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.ob {
  height: 100%;
  display: grid;
  place-items: center;
  padding: 20px;
  overflow-y: auto;
}
.ob-card {
  width: min(920px, 100%);
  min-height: min(680px, calc(100vh - 80px));
  border-radius: 30px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.ob-head {
  display: flex;
  align-items: center;
  gap: 20px;
  padding: 20px 26px;
  border-bottom: 1px solid var(--line);
}
.dots {
  display: flex;
  gap: 6px;
  flex: 1;
  justify-content: center;
}
.dot-step {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 5px 12px 5px 5px;
  border: none;
  background: transparent;
  border-radius: 999px;
  color: var(--text-3);
  font-size: 12.5px;
  cursor: pointer;
  transition: all 360ms var(--spring);
}
.dot-step:disabled {
  cursor: default;
}
.dot-step i {
  width: 22px;
  height: 22px;
  border-radius: 50%;
  display: grid;
  place-items: center;
  font-style: normal;
  font-size: 11px;
  font-weight: 700;
  border: 1.5px solid var(--line-2);
}
.dot-step.on {
  background: var(--info-bg);
  color: var(--brand-ink);
  font-weight: 650;
}
.dot-step.on i {
  background: var(--brand-grad);
  border-color: transparent;
  color: #fff;
}
.dot-step.done {
  color: var(--text-2);
}
.dot-step.done i {
  background: var(--ok);
  border-color: transparent;
  color: #fff;
}
.ob-body {
  flex: 1;
  padding: 26px 34px;
  overflow-y: auto;
  position: relative;
}
.pane h2 {
  font-size: 22px;
  margin-bottom: 6px;
}
.pane-welcome,
.pane-done {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: 12px;
  padding-top: 8px;
}
.hero-mark {
  filter: drop-shadow(0 18px 40px rgba(80, 100, 255, 0.4));
  margin-bottom: 4px;
}
.pane-welcome h1,
.pane-done h1 {
  font-size: 30px;
  letter-spacing: -0.02em;
}
.grad {
  background: linear-gradient(120deg, #3d7bff, #8a5cff 55%, #ff9f1a);
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}
.lead {
  max-width: 620px;
  font-size: 15px;
}
.feats {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
  margin-top: 14px;
  width: 100%;
  max-width: 720px;
  text-align: left;
}
.feat {
  display: grid;
  grid-template-columns: 26px 1fr;
  grid-template-rows: auto auto;
  column-gap: 10px;
  padding: 14px;
  border-radius: 16px;
  background: var(--glass-1);
  border: 1px solid var(--line);
  animation: rise 600ms var(--spring-soft) both;
}
.feat:nth-child(2) {
  animation-delay: 80ms;
}
.feat:nth-child(3) {
  animation-delay: 160ms;
}
.feat:nth-child(4) {
  animation-delay: 240ms;
}
.feat :deep(.ico) {
  grid-row: span 2;
  color: var(--brand);
  width: 22px;
  height: 22px;
}
.feat span {
  font-size: 12.5px;
  color: var(--text-2);
}
.roles {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
  margin-top: 18px;
}
.role {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 18px;
  text-align: left;
  cursor: pointer;
  border-radius: 20px;
}
.role.on {
  border-color: color-mix(in srgb, var(--brand-1) 60%, transparent);
  box-shadow: var(--shadow-2), 0 0 0 3px rgba(61, 123, 255, 0.18);
}
.r-ico {
  width: 44px;
  height: 44px;
  border-radius: 14px;
  display: grid;
  place-items: center;
  background: var(--info-bg);
  color: var(--brand-ink);
}
.role.on .r-ico {
  background: var(--brand-grad);
  color: #fff;
}
.role b {
  font-size: 15.5px;
}
.flow {
  margin-top: auto;
  padding-top: 8px;
  border-top: 1px dashed var(--line-2);
  color: var(--text-3);
}
.r-ck {
  position: absolute;
  right: 14px;
  top: 14px;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  display: grid;
  place-items: center;
  background: var(--brand-grad);
  color: #fff;
  transform: scale(0);
  transition: transform 380ms var(--spring);
  z-index: 3;
}
.role.on .r-ck {
  transform: scale(1);
}
.rec {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 12px;
  padding: 10px 12px;
  border-radius: 14px;
  background: var(--info-bg);
  color: var(--brand-ink);
}
.rec-text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  line-height: 1.35;
}
.rec-text b {
  font-size: 13px;
  color: var(--text);
}
.rec-text small {
  font-size: 11.5px;
  color: var(--text-2);
}
.rec-star {
  color: var(--amber);
  margin-left: 4px;
  vertical-align: -1px;
}
.need {
  font-style: normal;
  font-size: 10px;
  font-weight: 600;
  padding: 0 4px;
  margin-right: 4px;
  border-radius: 4px;
}
.need.orange {
  background: var(--warn-bg);
  color: var(--warn);
}
.need.blue {
  background: var(--info-bg);
  color: var(--brand-ink);
}
.need.green {
  background: var(--ok-bg);
  color: var(--ok);
}
.plats {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
}
.plat {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 9px 10px;
  border-radius: 13px;
  border: 1px solid var(--line);
  background: var(--glass-1);
  cursor: pointer;
  text-align: left;
  transition: all 280ms var(--spring);
}
.plat:active {
  transform: scale(0.97);
}
.plat.on {
  border-color: color-mix(in srgb, var(--brand-1) 50%, transparent);
  background: var(--info-bg);
}
.glyph {
  width: 28px;
  height: 28px;
  border-radius: 9px;
  display: grid;
  place-items: center;
  color: #fff;
  font-style: normal;
  font-weight: 700;
  font-size: 13px;
  flex: none;
}
.pl-name {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  line-height: 1.25;
}
.pl-name b {
  font-size: 13px;
}
.pl-name small {
  font-size: 11px;
  color: var(--text-3);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.pl-ck {
  width: 18px;
  height: 18px;
  border-radius: 6px;
  border: 1.5px solid var(--line-2);
  display: grid;
  place-items: center;
  color: transparent;
  flex: none;
}
.plat.on .pl-ck {
  background: var(--brand-grad);
  border-color: transparent;
  color: #fff;
}
.big-check {
  filter: drop-shadow(0 10px 26px rgba(47, 210, 122, 0.45));
}
.tips {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 10px;
  font-size: 13px;
  color: var(--text-2);
}
.tips div {
  display: flex;
  align-items: center;
  gap: 8px;
  justify-content: center;
}
.ob-foot {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 16px 26px;
  border-top: 1px solid var(--line);
}
.slide-l-enter-active,
.slide-r-enter-active {
  transition: opacity 360ms var(--ease-out), transform 520ms var(--spring-soft), filter 360ms var(--ease-out);
}
.slide-l-leave-active,
.slide-r-leave-active {
  transition: opacity 180ms ease, transform 200ms ease, filter 180ms ease;
}
.slide-l-enter-from {
  opacity: 0;
  transform: translateX(40px);
  filter: blur(10px);
}
.slide-l-leave-to {
  opacity: 0;
  transform: translateX(-30px);
  filter: blur(8px);
}
.slide-r-enter-from {
  opacity: 0;
  transform: translateX(-40px);
  filter: blur(10px);
}
.slide-r-leave-to {
  opacity: 0;
  transform: translateX(30px);
  filter: blur(8px);
}
</style>
