<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { api, toAppError } from "@/api";
import type { FxRates } from "@/api/types";
import { useAppStore } from "@/stores/app";
import { useUiStore } from "@/stores/ui";
import { CURRENCIES } from "@/utils/format";
import Icon from "@/components/ui/Icon.vue";

const app = useAppStore();
const ui = useUiStore();
const router = useRouter();
const fx = ref<FxRates | null>(app.fx);

onMounted(async () => {
  try {
    fx.value = await api.fxRates();
  } catch {
    /* ignore */
  }
});

async function openDir() {
  try {
    const p = await api.openDataDir();
    ui.toast("已打开数据目录", "success", 2000, p);
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
async function exp(kind: "watches" | "listings" | "runs") {
  try {
    const p = await api.dataExport(kind);
    ui.toast("已导出 CSV（Excel 可直接打开）", "success", 4000, p);
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
function rate(code: string): string {
  if (!fx.value) return "—";
  const base = app.displayCurrency;
  const r = (fx.value.rates[code] ?? (code === fx.value.base ? 1 : NaN)) / (fx.value.rates[base] ?? (base === fx.value.base ? 1 : NaN));
  return Number.isFinite(r) ? (1 / r).toFixed(code === "JPY" || code === "KRW" ? 4 : 3) : "—";
}
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1>数据</h1>
        <p>所有数据只保存在本机：SQLite 数据库、浏览器会话目录与日志。</p>
      </div>
    </div>

    <section class="glass card">
      <div class="card-head"><h3>数据目录</h3></div>
      <div class="row">
        <code class="path selectable">{{ app.dataDir }}</code>
        <span class="spacer"></span>
        <button class="btn" @click="openDir"><Icon name="folder" />打开</button>
      </div>
    </section>

    <section class="glass card">
      <div class="card-head"><h3>导出</h3><span class="faint tiny">CSV（UTF-8 BOM，Excel 不乱码）</span></div>
      <div class="grid g3">
        <button v-ripple class="exp" @click="exp('watches')"><Icon name="tag" :size="20" /><b>追踪商品与价格历史</b><small>watchlist.csv</small></button>
        <button v-ripple class="exp" @click="exp('listings')"><Icon name="box" :size="20" /><b>全部商品记录</b><small>listings.csv</small></button>
        <button v-ripple class="exp" @click="exp('runs')"><Icon name="history" :size="20" /><b>搜索记录</b><small>runs.csv</small></button>
      </div>
    </section>

    <section class="glass card">
      <div class="card-head">
        <h3>汇率</h3>
        <span class="chip" :class="fx?.source === 'ecb' ? 'green' : 'orange'">{{ fx?.source === "ecb" ? "ECB 参考汇率" : "内置参考表（离线）" }}</span>
        <span class="faint tiny">{{ fx?.date }}</span>
      </div>
      <div class="fx">
        <div v-for="c in CURRENCIES.filter((x) => x.code !== app.displayCurrency)" :key="c.code" class="fx-i">
          <span class="faint tiny">1 {{ c.code }}</span>
          <b class="num">{{ rate(c.code) }}</b>
          <span class="faint tiny">{{ app.displayCurrency }}</span>
        </div>
      </div>
    </section>

    <section class="glass card">
      <div class="card-head"><h3>登录会话</h3></div>
      <div class="row">
        <span class="muted small">登录会话保存在本机浏览器目录。需要清除时请到「账号与登录」逐个退出或一键清除。</span>
        <span class="spacer"></span>
        <button class="btn" @click="router.push('/platforms/accounts')"><Icon name="key" />账号与登录</button>
      </div>
    </section>
    <p class="faint tiny" style="text-align: center">Hawwwker 货客 v{{ app.version }} · Tauri + Vue · 在你的电脑上运行</p>
  </div>
</template>

<style scoped>
.path {
  font-family: var(--mono);
  font-size: 12.5px;
  padding: 8px 12px;
  border-radius: 10px;
  background: var(--hover);
}
.exp {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 4px;
  padding: 16px;
  border-radius: 16px;
  border: 1px solid var(--line);
  background: var(--glass-1);
  cursor: pointer;
  text-align: left;
  transition: transform var(--t-mid) var(--spring), box-shadow var(--t-mid);
}
.exp:hover {
  transform: translateY(-2px);
  box-shadow: var(--shadow-1);
}
.exp :deep(.ico) {
  color: var(--brand);
  margin-bottom: 4px;
}
.exp small {
  font-size: 11.5px;
  color: var(--text-3);
  font-family: var(--mono);
}
.fx {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: 8px;
}
.fx-i {
  display: flex;
  flex-direction: column;
  padding: 10px 12px;
  border-radius: 12px;
  background: var(--hover);
}
.fx-i b {
  font-size: 16px;
}
</style>
