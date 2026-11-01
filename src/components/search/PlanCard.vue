<script setup lang="ts">
/* AI 计划卡：芯片展示，可展开编辑后按此计划重新搜索 */
import { computed, ref, watch } from "vue";
import type { Condition, SearchPlan, SortKey } from "@/api/types";
import { usePlatformStore } from "@/stores/platforms";
import { CONDITION, GOAL, SORT } from "@/utils/labels";
import { CURRENCIES, money } from "@/utils/format";
import Icon from "@/components/ui/Icon.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";
import { clone } from "@/utils/clone";

const props = defineProps<{ plan: SearchPlan; running?: boolean; collapsed?: boolean }>();
const expanded = ref(!props.collapsed);
watch(
  () => props.collapsed,
  (c) => (expanded.value = !c),
);
const emit = defineEmits<{ rerun: [SearchPlan]; monitor: [SearchPlan] }>();
const platforms = usePlatformStore();
const editing = ref(false);
const draft = ref<SearchPlan>(clone(props.plan));
const newExclude = ref("");
const newKeyword = ref("");

watch(
  () => props.plan,
  (p) => {
    if (!editing.value) draft.value = clone(p);
  },
  { deep: true },
);

const budget = computed(() => {
  const p = props.plan;
  if (p.priceMin != null && p.priceMax != null) return `${money(p.priceMin, p.currency)} – ${money(p.priceMax, p.currency)}`;
  if (p.priceMax != null) return `≤ ${money(p.priceMax, p.currency)}`;
  if (p.priceMin != null) return `≥ ${money(p.priceMin, p.currency)}`;
  return "";
});
const spec = computed(() => {
  const { brand, model } = props.plan.product;
  if (brand && model && model.toLowerCase().includes(brand.toLowerCase())) return model;
  return [brand, model].filter(Boolean).join(" ");
});

function startEdit() {
  draft.value = clone(props.plan);
  editing.value = true;
}
function addTo(list: "exclude" | "keywords") {
  const v = (list === "exclude" ? newExclude : newKeyword).value.trim();
  if (!v) return;
  if (!draft.value[list].includes(v)) draft.value[list].push(v);
  if (list === "exclude") newExclude.value = "";
  else newKeyword.value = "";
}
function removeFrom(list: "exclude" | "keywords", i: number) {
  draft.value[list].splice(i, 1);
}
function removePlatform(id: string) {
  draft.value.platforms = draft.value.platforms.filter((x) => x !== id);
  draft.value.queries = draft.value.queries.filter((q) => q.platformId !== id);
}
function submit() {
  editing.value = false;
  emit("rerun", clone(draft.value));
}
const sorts = Object.entries(SORT) as [SortKey, string][];
const conds: Condition[] = ["any", "new", "used"];
const numOrNull = (v: string) => (v.trim() === "" ? null : Number(v));
</script>

<template>
  <section class="plan glass">
    <div class="plan-head">
      <span class="pl-ico"><Icon name="wand" :size="16" /></span>
      <h3>AI 搜索计划</h3>
      <span class="chip purple">{{ plan.parser === "rules" ? "规则解析" : plan.parser === "ai" ? "AI 规划" : "AI + 规则" }}</span>
      <span class="spacer"></span>
      <template v-if="!editing">
        <button class="btn ghost sm" :disabled="running" @click="emit('monitor', plan)"><Icon name="clock" />保存为监控</button>
        <button class="btn sm" :disabled="running" @click="startEdit"><Icon name="edit" />编辑计划</button>
        <button class="icon-btn" :title="expanded ? '收起' : '展开搜索词与说明'" @click="expanded = !expanded"><Icon :name="expanded ? 'up' : 'down'" :size="16" /></button>
      </template>
    </div>

    <div v-if="!editing" class="chips">
      <span class="chip blue"><Icon name="target" />{{ GOAL[plan.goal] }}</span>
      <span v-for="k in plan.keywords" :key="k" class="chip">{{ k }}</span>
      <span v-if="spec && !plan.keywords.includes(spec)" class="chip outline">{{ spec }}</span>
      <span v-if="budget" class="chip amber"><Icon name="tag" />{{ budget }}</span>
      <span v-if="plan.condition !== 'any'" class="chip green">{{ CONDITION[plan.condition] }}</span>
      <span v-for="x in plan.exclude" :key="x" class="chip red"><Icon name="x" />{{ x }}</span>
      <span v-if="plan.sort !== 'relevance'" class="chip">{{ SORT[plan.sort] }}</span>
      <span v-if="plan.monitor" class="chip green"><Icon name="clock" />建议每 {{ Math.round(plan.monitor.intervalMinutes / 60) }} 小时监控</span>
    </div>
    <div v-if="!editing && expanded && plan.queries.length" class="queries">
      <div v-for="q in plan.queries" :key="q.platformId + q.q" class="q">
        <PlatformBadge :id="q.platformId" />
        <code class="selectable">{{ q.q }}</code>
        <span class="lang">{{ q.lang }}</span>
      </div>
    </div>
    <ul v-if="!editing && expanded && plan.notes.length" class="notes">
      <li v-for="(n, i) in plan.notes" :key="i">{{ n }}</li>
    </ul>

    <div v-if="editing" class="edit">
      <div class="form-grid">
        <label class="field full">
          <span>关键词</span>
          <div class="chips">
            <span v-for="(k, i) in draft.keywords" :key="k" class="chip">{{ k }}<button class="x" @click="removeFrom('keywords', i)"><Icon name="x" :size="11" /></button></span>
            <input v-model="newKeyword" class="input sm mini" placeholder="添加关键词，回车" @keydown.enter.prevent="addTo('keywords')" />
          </div>
        </label>
        <label class="field">
          <span>预算下限</span>
          <input class="input sm" type="number" :value="draft.priceMin ?? ''" placeholder="不限" @input="draft.priceMin = numOrNull(($event.target as HTMLInputElement).value)" />
        </label>
        <label class="field">
          <span>预算上限</span>
          <div class="row">
            <input class="input sm" type="number" :value="draft.priceMax ?? ''" placeholder="不限" @input="draft.priceMax = numOrNull(($event.target as HTMLInputElement).value)" />
            <select v-model="draft.currency" class="select sm" style="width: 110px">
              <option v-for="c in CURRENCIES" :key="c.code" :value="c.code">{{ c.code }}</option>
            </select>
          </div>
        </label>
        <label class="field">
          <span>成色</span>
          <select v-model="draft.condition" class="select sm">
            <option v-for="c in conds" :key="c" :value="c">{{ CONDITION[c] }}</option>
          </select>
        </label>
        <label class="field">
          <span>排序</span>
          <select v-model="draft.sort" class="select sm">
            <option v-for="[k, l] in sorts" :key="k" :value="k">{{ l }}</option>
          </select>
        </label>
        <label class="field full">
          <span>排除词</span>
          <div class="chips">
            <span v-for="(x, i) in draft.exclude" :key="x" class="chip red">{{ x }}<button class="x" @click="removeFrom('exclude', i)"><Icon name="x" :size="11" /></button></span>
            <input v-model="newExclude" class="input sm mini" placeholder="添加排除词，回车" @keydown.enter.prevent="addTo('exclude')" />
          </div>
        </label>
        <div class="field full">
          <span>各平台搜索词（AI 已按平台语言本地化，可直接修改）</span>
          <div class="qedit">
            <div v-for="q in draft.queries" :key="q.platformId" class="qrow">
              <PlatformBadge :id="q.platformId" />
              <input v-model="q.q" class="input sm" />
              <button class="icon-btn" title="移除该平台" @click="removePlatform(q.platformId)"><Icon name="x" :size="15" /></button>
            </div>
          </div>
        </div>
      </div>
      <div class="row" style="justify-content: flex-end; margin-top: 12px">
        <span class="faint tiny">共 {{ draft.platforms.length }} 个平台 · {{ platforms.list.length }} 个可用</span>
        <span class="spacer"></span>
        <button class="btn sm" @click="editing = false">取消</button>
        <button v-ripple class="btn sm primary" :disabled="!draft.keywords.length || !draft.platforms.length" @click="submit"><Icon name="play" />按此计划重新搜索</button>
      </div>
    </div>
  </section>
</template>

<style scoped>
.plan {
  padding: 14px 18px 16px;
}
.plan-head {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 12px;
}
.plan-head h3 {
  font-size: 14.5px;
}
.pl-ico {
  width: 28px;
  height: 28px;
  border-radius: 9px;
  display: grid;
  place-items: center;
  background: var(--purple-bg);
  color: var(--purple);
}
.queries {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 10px;
}
.q {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 4px 8px 4px 6px;
  border-radius: 10px;
  background: var(--hover);
  font-size: 12px;
}
.q code {
  font-family: var(--mono);
  font-size: 12px;
  color: var(--text);
}
.lang {
  font-size: 10.5px;
  color: var(--text-3);
  text-transform: uppercase;
}
.notes {
  margin: 10px 0 0;
  padding-left: 18px;
  font-size: 12.5px;
  color: var(--text-2);
  display: grid;
  gap: 2px;
}
.chip .x {
  border: none;
  background: transparent;
  color: inherit;
  cursor: pointer;
  padding: 0;
  margin-left: 2px;
  display: grid;
  place-items: center;
  opacity: 0.7;
}
.mini {
  width: 160px;
  height: 26px;
}
.qedit {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px 12px;
}
.qrow {
  display: grid;
  grid-template-columns: 120px 1fr auto;
  align-items: center;
  gap: 8px;
}
</style>
