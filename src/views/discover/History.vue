<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { toAppError } from "@/api";
import { useSearchStore } from "@/stores/search";
import { useUiStore } from "@/stores/ui";
import { useAppStore } from "@/stores/app";
import { RUN_STATUS } from "@/utils/labels";
import { ago, dateTime, money } from "@/utils/format";
import type { RunStatus } from "@/api/types";
import GSegmented from "@/components/ui/GSegmented.vue";
import Icon from "@/components/ui/Icon.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";

const search = useSearchStore();
const ui = useUiStore();
const app = useAppStore();
const router = useRouter();
const loading = ref(true);
const filter = ref<"all" | RunStatus>("all");
const q = ref("");

onMounted(async () => {
  await search.loadHistory(200);
  loading.value = false;
});

const rows = computed(() =>
  search.history.filter((h) => (filter.value === "all" || h.status === filter.value) && (!q.value || h.instruction.includes(q.value))),
);
const groups = computed(() => {
  const out: { label: string; items: typeof rows.value }[] = [];
  const day = (iso: string) => {
    const d = new Date(iso);
    const t = new Date();
    const diff = Math.floor((new Date(t.toDateString()).getTime() - new Date(d.toDateString()).getTime()) / 86400000);
    return diff === 0 ? "今天" : diff === 1 ? "昨天" : diff < 7 ? `${diff} 天前` : d.toLocaleDateString("zh-CN");
  };
  for (const r of rows.value) {
    const l = day(r.startedAt);
    const g = out.find((x) => x.label === l);
    if (g) g.items.push(r);
    else out.push({ label: l, items: [r] });
  }
  return out;
});

async function open(id: string) {
  try {
    await search.loadRun(id);
    void router.push({ path: "/discover/search", query: { run: id } });
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
async function rerun(instruction: string) {
  try {
    await router.push("/discover/search");
    const id = await search.start(instruction);
    if (!id) ui.toast("未开始搜索", "info", 2600, "所选平台需要先登录；登录后再搜索即可");
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
async function del(id: string) {
  if (!(await ui.confirm("删除这条搜索记录？", "结果与 AI 总结会一起删除，已追踪的商品不受影响。", "删除", true))) return;
  await search.remove(id);
  ui.toast("已删除", "success", 1500);
}
const opts = [
  { value: "all" as const, label: "全部" },
  { value: "done" as const, label: "完成" },
  { value: "interrupted" as const, label: "已中断" },
  { value: "error" as const, label: "失败" },
  { value: "cancelled" as const, label: "已取消" },
];
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1>搜索记录</h1>
        <p>每次搜索的计划、结果与 AI 总结都会保存，点击即可完整回放。</p>
      </div>
      <span class="spacer"></span>
      <input v-model="q" class="input" style="width: 220px" placeholder="筛选指令…" />
      <GSegmented v-model="filter" :options="opts" small />
    </div>

    <div v-if="loading" class="col">
      <div v-for="i in 5" :key="i" class="skel" style="height: 72px"></div>
    </div>
    <div v-else-if="!rows.length" class="empty glass">
      <div class="empty-icon"><Icon name="history" /></div>
      <h3>还没有搜索记录</h3>
      <p>去「智能搜索」说一句你想买的东西吧。</p>
      <router-link to="/discover/search" class="btn primary">开始搜索</router-link>
    </div>
    <template v-else>
      <section v-for="g in groups" :key="g.label" class="stack" style="gap: 8px">
        <p class="eyebrow">{{ g.label }}</p>
        <div class="glass list stagger">
          <div v-for="(h, i) in g.items" :key="h.id" class="hr" :style="{ '--i': i }" @click="open(h.id)">
            <span class="hr-ico" :class="RUN_STATUS[h.status].tone"><Icon :name="h.trigger === 'monitor' ? 'clock' : 'search'" :size="17" /></span>
            <div class="hr-main">
              <p class="hr-title ellipsis">{{ h.instruction }}</p>
              <div class="row" style="gap: 8px; flex-wrap: wrap">
                <span class="chip" :class="RUN_STATUS[h.status].tone">{{ RUN_STATUS[h.status].label }}</span>
                <span v-if="h.trigger === 'monitor'" class="chip purple">定时监控</span>
                <span class="faint tiny">{{ dateTime(h.startedAt) }} · {{ ago(h.startedAt) }}</span>
                <span class="plats"><PlatformBadge v-for="p in h.platforms.slice(0, 6)" :key="p" :id="p" :show-name="false" /></span>
                <span v-if="h.error" class="tiny" style="color: var(--err)">{{ h.error }}</span>
              </div>
            </div>
            <div class="hr-num">
              <b class="num">{{ h.kept }}</b><small>条结果</small>
            </div>
            <div class="hr-num">
              <b class="num price amber">{{ h.bestPrice != null ? money(h.bestPrice, h.currency || app.displayCurrency) : "—" }}</b><small>最低价</small>
            </div>
            <div class="row" @click.stop>
              <button class="btn sm" title="用同一指令重新搜索" @click="rerun(h.instruction)"><Icon name="refresh" />重搜</button>
              <button class="icon-btn" title="删除" @click="del(h.id)"><Icon name="trash" :size="16" /></button>
            </div>
          </div>
        </div>
      </section>
    </template>
  </div>
</template>

<style scoped>
.list {
  padding: 4px;
}
.hr {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 12px 14px;
  border-radius: 14px;
  cursor: pointer;
  transition: background var(--t-fast);
}
.hr:hover {
  background: var(--hover);
}
.hr-ico {
  width: 38px;
  height: 38px;
  border-radius: 12px;
  display: grid;
  place-items: center;
  background: var(--hover);
  color: var(--text-2);
  flex: none;
}
.hr-ico.green {
  background: var(--ok-bg);
  color: var(--ok);
}
.hr-ico.orange {
  background: var(--warn-bg);
  color: var(--warn);
}
.hr-ico.red {
  background: var(--err-bg);
  color: var(--err);
}
.hr-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.hr-title {
  font-weight: 600;
}
.plats {
  display: inline-flex;
  gap: 2px;
}
.hr-num {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  min-width: 90px;
}
.hr-num b {
  font-size: 16px;
}
.hr-num small {
  font-size: 11px;
  color: var(--text-3);
}
</style>
