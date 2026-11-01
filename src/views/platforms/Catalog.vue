<script setup lang="ts">
import { computed, ref } from "vue";
import { toAppError } from "@/api";
import type { CustomPlatformInput, PlatformInfo } from "@/api/types";
import { KIND_LABEL, usePlatformStore } from "@/stores/platforms";
import { useUiStore } from "@/stores/ui";
import { CURRENCIES } from "@/utils/format";
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
        <p>选择要参与搜索的平台与站点版本。国际站搜索规则为 Beta，网站改版时可直接修改搜索地址模板。</p>
      </div>
      <span class="spacer"></span>
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
          <tr><th style="width: 60px">启用</th><th>平台</th><th>地区</th><th>类型</th><th>币种</th><th>登录</th><th>搜索地址模板</th><th></th></tr>
        </thead>
        <tbody>
          <tr v-for="p in rows" :key="p.id" :class="{ off: !p.enabled }">
            <td><GSwitch :model-value="p.enabled" small @update:model-value="(v: boolean) => toggle(p, v)" /></td>
            <td>
              <div class="row" style="gap: 10px">
                <i class="glyph" :style="{ background: p.color }">{{ p.glyph }}</i>
                <div class="col" style="gap: 0">
                  <b>{{ p.name }}<span v-if="p.beta" class="chip orange beta">Beta</span><span v-if="p.custom" class="chip purple beta">自定义</span></b>
                  <small class="faint">{{ p.edition }} · {{ p.familyName }}</small>
                </div>
              </div>
            </td>
            <td><span class="chip" :class="p.region === 'cn' ? 'red' : 'blue'">{{ p.region === "cn" ? "国内版" : "国际版" }}</span> <small class="faint">{{ p.country }}</small></td>
            <td class="faint">{{ KIND_LABEL[p.kind] }}</td>
            <td class="num">{{ p.currency }}</td>
            <td class="faint">{{ LOGIN_NEED[p.login] }}</td>
            <td style="max-width: 380px">
              <div v-if="editing === p.id" class="row" style="gap: 6px">
                <input v-model="tpl" class="input sm mono" @keydown.enter="saveTpl(p)" @keydown.esc="editing = null" />
                <button class="btn sm primary" @click="saveTpl(p)">保存</button>
                <button v-if="p.searchUrl !== p.defaultSearchUrl" class="btn sm" @click="resetTpl(p)">默认</button>
              </div>
              <button v-else class="tpl" :title="p.searchUrl" @click="startEdit(p)">
                <code class="ellipsis">{{ p.searchUrl }}</code>
                <span v-if="p.searchUrl !== p.defaultSearchUrl" class="chip amber">已修改</span>
                <Icon name="edit" :size="13" />
              </button>
            </td>
            <td class="r">
              <button v-if="p.custom" class="icon-btn" title="删除" @click="remove(p)"><Icon name="trash" :size="16" /></button>
            </td>
          </tr>
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
.tpl {
  display: flex;
  align-items: center;
  gap: 6px;
  max-width: 100%;
  padding: 4px 8px;
  border-radius: 8px;
  border: 1px dashed transparent;
  background: transparent;
  cursor: pointer;
  color: var(--text-3);
}
.tpl:hover {
  border-color: var(--line-2);
  background: var(--hover);
}
.tpl code {
  font-family: var(--mono);
  font-size: 11.5px;
  color: var(--text-2);
  min-width: 0;
}
</style>
