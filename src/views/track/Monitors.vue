<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { api, toAppError } from "@/api";
import type { Monitor } from "@/api/types";
import { useWatchStore } from "@/stores/watch";
import { useSearchStore } from "@/stores/search";
import { useUiStore } from "@/stores/ui";
import { ago, interval, money } from "@/utils/format";
import GSwitch from "@/components/ui/GSwitch.vue";
import GSheet from "@/components/ui/GSheet.vue";
import Icon from "@/components/ui/Icon.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";

const store = useWatchStore();
const search = useSearchStore();
const ui = useUiStore();
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
    <TransitionGroup v-else name="list" tag="div" class="stack" style="position: relative">
      <article v-for="m in store.monitors" :key="m.id" class="mc glass" :class="{ off: !m.enabled }">
        <div class="mc-left">
          <span class="mc-ico" :class="STATUS[m.lastStatus][1]"><Icon name="clock" :size="20" :class="{ spin: m.lastStatus === 'running' }" /></span>
        </div>
        <div class="mc-main">
          <div class="row" style="gap: 8px">
            <h3>{{ m.name }}</h3>
            <span class="chip" :class="STATUS[m.lastStatus][1]">{{ STATUS[m.lastStatus][0] }}</span>
            <span class="chip">每 {{ interval(m.intervalMinutes) }}</span>
            <span v-if="m.targetPrice != null" class="chip amber"><Icon name="target" />目标 {{ money(m.targetPrice, m.currency) }}</span>
          </div>
          <p class="muted small ellipsis">{{ m.instruction }}</p>
          <div class="row" style="gap: 6px; flex-wrap: wrap">
            <PlatformBadge v-for="p in m.plan.platforms.slice(0, 8)" :key="p" :id="p" />
          </div>
        </div>
        <div class="mc-stats">
          <div><b class="num">{{ m.lastNewCount }}</b><small>上次新增</small></div>
          <div><b class="num price amber">{{ m.bestPrice != null ? money(m.bestPrice, m.currency) : "—" }}</b><small>最低价</small></div>
          <div><b class="num">{{ m.runCount }}</b><small>累计运行</small></div>
        </div>
        <div class="mc-side">
          <GSwitch :model-value="m.enabled" @update:model-value="(v: boolean) => toggle(m, v)" />
          <span class="faint tiny">{{ m.lastRunAt ? "上次 " + ago(m.lastRunAt) : "尚未运行" }}</span>
          <span class="faint tiny">{{ m.enabled && m.nextRunAt ? "下次 " + ago(m.nextRunAt) : "" }}</span>
          <div class="row" style="gap: 4px">
            <button class="btn sm" :disabled="m.lastStatus === 'running'" @click="runNow(m)"><Icon name="play" />立即运行</button>
            <button v-if="m.lastRunId" class="icon-btn" title="查看上次结果" @click="openLast(m)"><Icon name="eye" :size="16" /></button>
            <button class="icon-btn" title="编辑" @click="edit(m)"><Icon name="edit" :size="16" /></button>
            <button class="icon-btn" title="删除" @click="del(m)"><Icon name="trash" :size="16" /></button>
          </div>
        </div>
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
  align-items: center;
  gap: 16px;
  padding: 16px 18px;
}
.mc.off {
  opacity: 0.62;
}
.mc-ico {
  width: 44px;
  height: 44px;
  border-radius: 14px;
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
.mc-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.mc-main h3 {
  font-size: 15px;
}
.mc-stats {
  display: flex;
  gap: 18px;
}
.mc-stats > div {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  min-width: 70px;
}
.mc-stats b {
  font-size: 17px;
}
.mc-stats small {
  font-size: 11px;
  color: var(--text-3);
}
.mc-side {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 4px;
  min-width: 220px;
}
@media (max-width: 1180px) {
  .mc-stats {
    display: none;
  }
}
</style>
