<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { api, toAppError } from "@/api";
import type { Monitor } from "@/api/types";
import { useWatchStore } from "@/stores/watch";
import { useSearchStore } from "@/stores/search";
import { useUiStore } from "@/stores/ui";
import { usePlatformStore } from "@/stores/platforms";
import { ago, interval, money } from "@/utils/format";
import GSwitch from "@/components/ui/GSwitch.vue";
import GSheet from "@/components/ui/GSheet.vue";
import Icon from "@/components/ui/Icon.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";

const store = useWatchStore();
const search = useSearchStore();
const ui = useUiStore();
const platforms = usePlatformStore();
const expanded = ref<Record<number, boolean>>({});
const menuFor = ref<number | null>(null);
const router = useRouter();
const loading = ref(true);
const creating = ref(false);
const form = ref({ instruction: "", name: "", intervalMinutes: 360, target: "" });
const editing = ref<Monitor | null>(null);
const editForm = ref({ name: "", intervalMinutes: 360, target: "" });
const intervals = [30, 60, 180, 360, 720, 1440, 4320];

onMounted(async () => {
  try {
    await store.loadMonitors();
  } finally {
    loading.value = false;
  }
});

async function create() {
  if (!form.value.instruction.trim()) return ui.toast("请填写监控指令", "warn");
  try {
    await store.createMonitor({
      instruction: form.value.instruction.trim(),
      name: form.value.name.trim() || undefined,
      intervalMinutes: form.value.intervalMinutes,
      targetPrice: form.value.target ? Number(form.value.target) : null,
    });
    creating.value = false;
    form.value = { instruction: "", name: "", intervalMinutes: 360, target: "" };
    ui.toast("监控已创建，AI 已生成搜索计划", "success");
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
async function toggle(m: Monitor, v: boolean) {
  await store.updateMonitor(m.id, { enabled: v });
}
async function runNow(m: Monitor) {
  try {
    const { runId } = await api.monitorRun(m.id);
    m.lastStatus = "running";
    await search.loadRun(runId);
    void router.push("/discover/search");
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
function edit(m: Monitor) {
  editing.value = m;
  editForm.value = { name: m.name, intervalMinutes: m.intervalMinutes, target: m.targetPrice != null ? String(m.targetPrice) : "" };
}
async function saveEdit() {
  if (!editing.value) return;
  await store.updateMonitor(editing.value.id, { name: editForm.value.name, intervalMinutes: editForm.value.intervalMinutes, targetPrice: editForm.value.target ? Number(editForm.value.target) : null });
  editing.value = null;
  ui.toast("已保存", "success", 1500);
}
async function del(m: Monitor) {
  if (!(await ui.confirm(`删除监控「${m.name}」？`, "历史运行记录会保留在搜索记录中。", "删除", true))) return;
  await store.deleteMonitor(m.id);
}
async function openLast(m: Monitor) {
  if (!m.lastRunId) return;
  await search.loadRun(m.lastRunId);
  void router.push("/discover/search");
}
const STATUS: Record<Monitor["lastStatus"], [string, string]> = { idle: ["等待首次运行", ""], running: ["运行中", "blue"], ok: ["正常", "green"], error: ["上次失败", "red"] };
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1>定时监控</h1>
        <p>把一次搜索保存下来，按间隔自动重跑：发现新品、记录最低价、低于目标价时提醒。</p>
      </div>
      <span class="spacer"></span>
      <button v-ripple class="btn primary" @click="creating = true"><Icon name="plus" />新建监控</button>
    </div>

    <div v-if="loading" class="col"><div v-for="i in 3" :key="i" class="skel" style="height: 120px"></div></div>
    <div v-else-if="!store.monitors.length" class="empty glass">
      <div class="empty-icon"><Icon name="clock" /></div>
      <h3>还没有定时监控</h3>
      <p>在搜索计划卡上点「保存为监控」，或在这里直接新建。</p>
    </div>
    <TransitionGroup v-else name="list" tag="div" class="stack" style="position: relative; gap: 10px">
      <article v-for="m in store.monitors" :key="m.id" class="mc glass" :class="{ off: !m.enabled }">
        <!-- 第一行：名称 · 状态 · 关键数字 · 操作 -->
        <div class="mc-l1">
          <span class="mc-ico" :class="STATUS[m.lastStatus][1]" :title="STATUS[m.lastStatus][0]"><Icon name="clock" :size="16" :class="{ spin: m.lastStatus === 'running' }" /></span>
          <h3 class="ellipsis">{{ m.name }}</h3>
          <span class="chip" :class="STATUS[m.lastStatus][1]">{{ STATUS[m.lastStatus][0] }}</span>
          <span v-if="m.targetPrice != null" class="chip amber"><Icon name="target" />目标 {{ money(m.targetPrice, m.currency) }}</span>
          <span class="spacer"></span>
          <span class="mc-best" title="最近一次运行的最低价"><small>最低</small><b class="price amber">{{ m.bestPrice != null ? money(m.bestPrice, m.currency) : "—" }}</b></span>
          <GSwitch :model-value="m.enabled" small @update:model-value="(v: boolean) => toggle(m, v)" />
          <button class="btn sm" :disabled="m.lastStatus === 'running'" @click="runNow(m)"><Icon name="play" />运行</button>
          <div class="more" @mouseleave="menuFor = null">
            <button class="icon-btn" title="更多" :aria-expanded="menuFor === m.id" @click="menuFor = menuFor === m.id ? null : m.id"><Icon name="menu" :size="16" /></button>
            <Transition name="pop">
              <div v-if="menuFor === m.id" class="more-menu glass-3">
                <button :disabled="!m.lastRunId" @click="menuFor = null; openLast(m)"><Icon name="eye" :size="15" />查看上次结果</button>
                <button @click="menuFor = null; edit(m)"><Icon name="edit" :size="15" />编辑</button>
                <button class="danger" @click="menuFor = null; del(m)"><Icon name="trash" :size="15" />删除</button>
              </div>
            </Transition>
          </div>
        </div>
        <!-- 第二行：指令 · 平台（折叠） · 时间与次数 -->
        <div class="mc-l2">
          <p class="muted small ellipsis" :title="m.instruction">{{ m.instruction }}</p>
          <button class="chip click" :title="m.plan.platforms.map((p) => platforms.label(p)).join('、')" @click="expanded = { ...expanded, [m.id]: !expanded[m.id] }">
            <Icon name="layers" />{{ m.plan.platforms.length }} 个平台<Icon :name="expanded[m.id] ? 'up' : 'down'" />
          </button>
          <span class="faint tiny nowrap">
            每 {{ interval(m.intervalMinutes) }} · {{ m.lastRunAt ? "上次 " + ago(m.lastRunAt) : "尚未运行" }}<template v-if="m.enabled && m.nextRunAt"> · 下次 {{ ago(m.nextRunAt) }}</template> · 新增 {{ m.lastNewCount }} · 累计 {{ m.runCount }} 次
          </span>
        </div>
        <Transition name="fade">
          <div v-if="expanded[m.id]" class="mc-plats">
            <PlatformBadge v-for="p in m.plan.platforms" :key="p" :id="p" />
          </div>
        </Transition>
      </article>
    </TransitionGroup>

    <GSheet v-model:open="creating" title="新建定时监控" subtitle="AI 会把指令转成搜索计划并按间隔自动执行" :width="520" center>
      <div class="stack">
        <label class="field"><span>监控指令</span><textarea v-model="form.instruction" class="textarea" rows="3" placeholder="例如：Switch 2 马力欧套装 京东 拼多多 亚马逊日本 最低价"></textarea></label>
        <div class="form-grid">
          <label class="field"><span>名称（可选）</span><input v-model="form.name" class="input" placeholder="自动生成" /></label>
          <label class="field"><span>间隔</span><select v-model.number="form.intervalMinutes" class="select"><option v-for="v in intervals" :key="v" :value="v">每 {{ interval(v) }}</option></select></label>
          <label class="field full"><span>目标价（显示币种，可选）</span><input v-model="form.target" class="input" type="number" placeholder="低于此价格时提醒" /></label>
        </div>
      </div>
      <template #foot>
        <button class="btn" @click="creating = false">取消</button>
        <button v-ripple class="btn primary" @click="create"><Icon name="check" />创建</button>
      </template>
    </GSheet>

    <GSheet :open="!!editing" title="编辑监控" :width="460" center @update:open="(v: boolean) => !v && (editing = null)">
      <div class="stack">
        <label class="field"><span>名称</span><input v-model="editForm.name" class="input" /></label>
        <label class="field"><span>间隔</span><select v-model.number="editForm.intervalMinutes" class="select"><option v-for="v in intervals" :key="v" :value="v">每 {{ interval(v) }}</option></select></label>
        <label class="field"><span>目标价</span><input v-model="editForm.target" class="input" type="number" placeholder="不设置" /></label>
      </div>
      <template #foot>
        <button class="btn" @click="editing = null">取消</button>
        <button v-ripple class="btn primary" @click="saveEdit">保存</button>
      </template>
    </GSheet>
  </div>
</template>

<style scoped>
.mc {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px 14px 12px 16px;
}
.mc.off {
  opacity: 0.62;
}
.mc-l1,
.mc-l2 {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.mc-l2 {
  padding-left: 38px;
}
.mc-l2 p {
  flex: 1;
  min-width: 0;
}
.mc-l1 h3 {
  font-size: 14.5px;
  min-width: 0;
}
.nowrap {
  white-space: nowrap;
}
.mc-ico {
  width: 30px;
  height: 30px;
  flex: none;
  border-radius: 10px;
  display: grid;
  place-items: center;
  background: var(--hover);
  color: var(--text-2);
}
.mc-ico.green {
  background: var(--ok-bg);
  color: var(--ok);
}
.mc-ico.blue {
  background: var(--info-bg);
  color: var(--brand-ink);
}
.mc-ico.red {
  background: var(--err-bg);
  color: var(--err);
}
.mc-best {
  display: inline-flex;
  align-items: baseline;
  gap: 5px;
  margin-right: 6px;
}
.mc-best small {
  font-size: 11px;
  color: var(--text-3);
}
.mc-best b {
  font-size: 15px;
}
.mc-plats {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding-left: 38px;
}
.more {
  position: relative;
}
.more-menu {
  position: absolute;
  right: 0;
  top: 34px;
  min-width: 160px;
  padding: 6px;
  border-radius: 12px;
  z-index: 30;
  display: flex;
  flex-direction: column;
}
.more-menu button {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border: none;
  background: transparent;
  border-radius: 8px;
  cursor: pointer;
  font-size: 13px;
  text-align: left;
  color: var(--text);
}
.more-menu button:hover:not(:disabled) {
  background: var(--hover);
}
.more-menu button:disabled {
  opacity: 0.45;
  cursor: default;
}
.more-menu .danger {
  color: var(--err);
}
@media (max-width: 1180px) {
  .mc-l2 .tiny {
    display: none;
  }
}
</style>
