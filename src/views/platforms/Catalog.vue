<script setup lang="ts">
import { computed, ref } from "vue";
import { api, openExternal, toAppError } from "@/api";
import type { CustomPlatformInput, PlatformInfo, SelfTestResult } from "@/api/types";
import { KIND_LABEL, usePlatformStore } from "@/stores/platforms";
import { useUiStore } from "@/stores/ui";
import { CURRENCIES, money } from "@/utils/format";
import GSegmented from "@/components/ui/GSegmented.vue";
import GSwitch from "@/components/ui/GSwitch.vue";
import GSheet from "@/components/ui/GSheet.vue";
import Icon from "@/components/ui/Icon.vue";

const store = usePlatformStore();
const ui = useUiStore();
const region = ref<"all" | "cn" | "intl">("all");
const q = ref("");
const editing = ref<string | null>(null);
const tpl = ref("");
const adding = ref(false);
const form = ref<CustomPlatformInput>({ name: "", region: "intl", currency: "EUR", locale: "de-DE", searchUrl: "", linkPattern: "", home: "", kind: "retail" });

const rows = computed(() =>
  store.list.filter(
    (p) => (region.value === "all" || p.region === region.value) && (!q.value || `${p.name}${p.edition}${p.familyName}${p.id}`.toLowerCase().includes(q.value.toLowerCase())),
  ),
);
const enabledCount = computed(() => store.list.filter((p) => p.enabled).length);

/* ---------- 平台检测：在真实网络上做一次小搜索，看能否识别结果与价格 ---------- */
const results = ref<Record<string, SelfTestResult>>({});
const testing = ref<Set<string>>(new Set());
const batch = ref<{ done: number; total: number } | null>(null);
async function selftest(ids: string[]) {
  const todo = ids.filter((id) => !testing.value.has(id));
  if (!todo.length) return;
  testing.value = new Set([...testing.value, ...todo]);
  try {
    for (let i = 0; i < todo.length; i += 12) {
      const chunk = todo.slice(i, i + 12);
      const out = await api.platformSelftest(chunk);
      results.value = { ...results.value, ...Object.fromEntries(out.map((r) => [r.platformId, r])) };
      const left = new Set(testing.value);
      chunk.forEach((id) => left.delete(id));
      testing.value = left;
      if (batch.value) batch.value = { ...batch.value, done: Math.min(batch.value.total, batch.value.done + chunk.length) };
    }
  } catch (e) {
    ui.toast(toAppError(e).message, "error", 4000);
    const left = new Set(testing.value);
    todo.forEach((id) => left.delete(id));
    testing.value = left;
  }
}
async function testEnabled() {
  const ids = store.list.filter((p) => p.enabled).map((p) => p.id);
  if (!ids.length) return ui.toast("还没有启用任何平台", "warn");
  batch.value = { done: 0, total: ids.length };
  await selftest(ids);
  const res = ids.map((id) => results.value[id]).filter(Boolean) as SelfTestResult[];
  const ok = res.filter((r) => r.ok).length;
  const login = res.filter((r) => r.loginRequired).length;
  ui.toast(`检测完成：${ok} / ${ids.length} 通过`, ok === ids.length ? "success" : "warn", 4200, login ? `${login} 个平台需要先登录` : undefined);
  batch.value = null;
}
const secs = (ms: number) => (ms / 1000).toFixed(1) + "s";

async function toggle(p: PlatformInfo, v: boolean) {
  try {
    await store.update(p.id, { enabled: v });
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
function startEdit(p: PlatformInfo) {
  editing.value = p.id;
  tpl.value = p.searchUrl;
}
async function saveTpl(p: PlatformInfo) {
  if (!tpl.value.includes("{q}")) return ui.toast("搜索地址必须包含 {q} 作为关键词占位符", "warn");
  try {
    await store.update(p.id, { searchUrl: tpl.value.trim() });
    editing.value = null;
    ui.toast("搜索地址已更新", "success", 1500);
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
async function resetTpl(p: PlatformInfo) {
  await store.update(p.id, { searchUrl: "" });
  editing.value = null;
  ui.toast("已恢复默认模板", "success", 1500);
}
async function addCustom() {
  try {
    const p = await store.addCustom({ ...form.value, linkPattern: form.value.linkPattern || undefined, home: form.value.home || undefined });
    adding.value = false;
    ui.toast(`已添加自定义平台「${p.name}」`, "success");
  } catch (e) {
    ui.toast(toAppError(e).message, "error", 4000);
  }
}
async function remove(p: PlatformInfo) {
  if (await ui.confirm(`删除自定义平台「${p.name}」？`, "历史结果会保留。", "删除", true)) await store.removeCustom(p.id);
}
const regionOpts = [
  { value: "all" as const, label: "全部" },
  { value: "cn" as const, label: "国内版" },
  { value: "intl" as const, label: "国际版" },
];
const LOGIN_NEED = { required: "必须登录", recommended: "建议登录", optional: "无需登录" };
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1>平台目录</h1>
        <p>选择参与搜索的平台与站点版本；「检测」会在真实网络上做一次小搜索，确认能识别结果与价格。网站改版时可修改搜索地址模板。</p>
      </div>
      <span class="spacer"></span>
      <button v-ripple class="btn" :disabled="!!batch" title="对每个已启用平台做一次真实的小搜索，检查能否识别结果与价格" @click="testEnabled">
        <Icon :name="batch ? 'refresh' : 'shield'" :class="{ spin: !!batch }" />{{ batch ? `检测中 ${batch.done}/${batch.total}` : "检测已启用平台" }}
      </button>
      <button v-ripple class="btn primary" @click="adding = true"><Icon name="plus" />添加自定义平台</button>
    </div>
    <div class="row">
      <GSegmented v-model="region" :options="regionOpts" small />
      <input v-model="q" class="input sm" style="width: 220px" placeholder="搜索平台…" />
      <span class="spacer"></span>
      <span class="faint small">已启用 {{ enabledCount }} / {{ store.list.length }}</span>
    </div>

    <section class="glass table-wrap">
      <table class="table">
        <thead>
          <tr><th style="width: 60px">启用</th><th>平台</th><th>检测</th><th class="r" style="width: 210px">操作</th></tr>
        </thead>
        <tbody>
          <template v-for="p in rows" :key="p.id">
            <tr :class="{ off: !p.enabled }">
              <td><GSwitch :model-value="p.enabled" small @update:model-value="(v: boolean) => toggle(p, v)" /></td>
              <td>
                <div class="row" style="gap: 10px">
                  <i class="glyph" :style="{ background: p.color }">{{ p.glyph }}</i>
                  <div class="col" style="gap: 1px; min-width: 0">
                    <b>{{ p.name }}<span v-if="p.beta" class="chip orange beta">Beta</span><span v-if="p.custom" class="chip purple beta">自定义</span><span v-if="p.searchUrl !== p.defaultSearchUrl" class="chip amber beta">模板已修改</span></b>
                    <small class="faint">
                      <span :class="p.region === 'cn' ? 'cn' : 'intl'">{{ p.region === "cn" ? "国内版" : "国际版" }}</span> {{ p.country }}<template v-if="p.edition !== (p.region === 'cn' ? '国内版' : '国际版')"> · {{ p.edition }}</template> · {{ KIND_LABEL[p.kind] }} · {{ p.currency }} · {{ LOGIN_NEED[p.login] }}
                    </small>
                  </div>
                </div>
              </td>
              <td class="st-cell">
                <span v-if="testing.has(p.id)" class="faint small"><Icon name="refresh" :size="13" class="spin" /> 正在打开搜索页…</span>
                <template v-else-if="results[p.id]">
                  <div class="st">
                    <span v-if="results[p.id]!.ok" class="chip green"><Icon name="check" />通过 · {{ results[p.id]!.withPrice }} 条含价格</span>
                    <span v-else-if="results[p.id]!.loginRequired" class="chip orange"><Icon name="lock" />需要登录</span>
                    <span v-else class="chip red"><Icon name="warn" />未通过</span>
                    <span class="faint tiny">{{ secs(results[p.id]!.ms) }}</span>
                  </div>
                  <button v-if="results[p.id]!.samples[0]" class="sample" :title="results[p.id]!.samples[0]!.url" @click="openExternal(results[p.id]!.samples[0]!.url)">
                    <span class="ellipsis">{{ results[p.id]!.samples[0]!.title }}</span>
                    <b class="price">{{ money(results[p.id]!.samples[0]!.landed ?? results[p.id]!.samples[0]!.price, results[p.id]!.samples[0]!.currency) }}</b>
                  </button>
                  <small v-else-if="results[p.id]!.message" class="faint ellipsis msg" :title="results[p.id]!.message ?? ''">{{ results[p.id]!.message }}</small>
                </template>
                <span v-else class="faint tiny">未检测</span>
              </td>
              <td class="r">
                <div class="row ops">
                  <button class="btn sm" :disabled="testing.has(p.id)" @click="selftest([p.id])"><Icon name="shield" />检测</button>
                  <button class="btn sm ghost" :class="{ on: editing === p.id }" @click="editing === p.id ? (editing = null) : startEdit(p)"><Icon name="edit" />编辑模板</button>
                  <button v-if="p.custom" class="icon-btn" title="删除" @click="remove(p)"><Icon name="trash" :size="16" /></button>
                </div>
              </td>
            </tr>
            <tr v-if="editing === p.id" class="tpl-row">
              <td></td>
              <td colspan="3">
                <div class="tpl-edit">
                  <span class="faint tiny">搜索地址模板（用 {q} 代表关键词；网站改版时可在这里修正）</span>
                  <div class="row" style="gap: 6px">
                    <input v-model="tpl" class="input sm mono" @keydown.enter="saveTpl(p)" @keydown.esc="editing = null" />
                    <button class="btn sm primary" @click="saveTpl(p)">保存</button>
                    <button v-if="p.searchUrl !== p.defaultSearchUrl" class="btn sm" @click="resetTpl(p)">恢复默认</button>
                    <button class="btn sm ghost" @click="editing = null">取消</button>
                  </div>
                  <code class="faint tiny mono">默认：{{ p.defaultSearchUrl }}</code>
                </div>
              </td>
            </tr>
          </template>
        </tbody>
      </table>
    </section>

    <GSheet v-model:open="adding" title="添加自定义平台" subtitle="任何有网页搜索页的购物网站都可以接入" :width="560" center>
      <div class="form-grid">
        <label class="field"><span>名称</span><input v-model="form.name" class="input" placeholder="例如 MediaMarkt" /></label>
        <label class="field">
          <span>地区</span>
          <select v-model="form.region" class="select"><option value="cn">国内版</option><option value="intl">国际版</option></select>
        </label>
        <label class="field full">
          <span>搜索地址模板（用 {q} 代表关键词）</span>
          <input v-model="form.searchUrl" class="input mono" placeholder="https://www.mediamarkt.de/de/search.html?query={q}" />
        </label>
        <label class="field full">
          <span>商品链接正则（可选，第 1 组为商品 ID）</span>
          <input v-model="form.linkPattern" class="input mono" placeholder="/product/[^/]+-(\d+)\.html" />
          <span class="hint">留空时按"商品图 + 价格"自动识别结果卡片</span>
        </label>
        <label class="field">
          <span>币种</span>
          <select v-model="form.currency" class="select"><option v-for="c in CURRENCIES" :key="c.code" :value="c.code">{{ c.code }} · {{ c.name }}</option></select>
        </label>
        <label class="field"><span>语言区域</span><input v-model="form.locale" class="input" placeholder="de-DE" /></label>
      </div>
      <template #foot>
        <button class="btn" @click="adding = false">取消</button>
        <button v-ripple class="btn primary" :disabled="!form.name || !form.searchUrl.includes('{q}')" @click="addCustom"><Icon name="plus" />添加</button>
      </template>
    </GSheet>
  </div>
</template>

<style scoped>
.table-wrap {
  padding: 6px 8px;
  overflow-x: auto;
}
tr.off td {
  opacity: 0.6;
}
tr.off td:first-child {
  opacity: 1;
}
.glyph {
  width: 30px;
  height: 30px;
  border-radius: 10px;
  display: grid;
  place-items: center;
  color: #fff;
  font-style: normal;
  font-weight: 700;
  flex: none;
}
.beta {
  height: 18px;
  font-size: 10.5px;
  margin-left: 6px;
  vertical-align: 1px;
}
.cn {
  color: var(--err);
}
.intl {
  color: var(--brand-ink);
}
.st-cell {
  max-width: 360px;
}
.st {
  display: flex;
  align-items: center;
  gap: 6px;
}
.sample {
  display: flex;
  align-items: baseline;
  gap: 8px;
  max-width: 340px;
  margin-top: 3px;
  padding: 0;
  border: none;
  background: transparent;
  cursor: pointer;
  font-size: 12px;
  color: var(--text-2);
  text-align: left;
}
.sample:hover span {
  text-decoration: underline;
}
.sample .price {
  font-size: 12.5px;
  flex: none;
}
.msg {
  display: block;
  max-width: 340px;
  margin-top: 3px;
}
.ops {
  gap: 4px;
  justify-content: flex-end;
}
.ops .on {
  background: var(--info-bg);
  color: var(--brand-ink);
}
.tpl-row td {
  border-top: none;
  padding-top: 0;
}
.tpl-edit {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px 12px;
  border-radius: 12px;
  background: var(--hover);
}
.tpl-edit .input {
  flex: 1;
}
</style>
