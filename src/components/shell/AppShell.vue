<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import Sidebar from "./Sidebar.vue";
import TopBar from "./TopBar.vue";
import { useScrollBlur } from "@/composables/motion";
import { useSearchStore } from "@/stores/search";
import { useUiStore } from "@/stores/ui";
import { usePlatformStore } from "@/stores/platforms";
import { money } from "@/utils/format";
import { useAppStore } from "@/stores/app";

const scroller = ref<HTMLElement | null>(null);
const topbarEl = ref<HTMLElement | null>(null);
const route = useRoute();
const router = useRouter();
const search = useSearchStore();
const ui = useUiStore();
const platforms = usePlatformStore();
const app = useAppStore();

useScrollBlur(scroller, topbarEl);

watch(
  () => route.fullPath,
  () => {
    scroller.value?.scrollTo({ top: 0, behavior: "instant" });
    topbarEl.value?.style.setProperty("--sb", "0");
  },
);

// 不在搜索页时完成检索：灵动岛提示
let off: (() => void) | null = null;
onMounted(() => {
  off = search.onFinish((r) => {
    if (route.name === "search" && search.currentId === r.runId) return;
    if (r.status === "done") {
      const best = r.results.filter((x) => x.match.verdict === "exact" || x.match.verdict === "close").sort((a, b) => (a.displayPrice ?? 1e12) - (b.displayPrice ?? 1e12))[0];
      ui.toast(
        `检索完成：${r.results.filter((x) => x.match.verdict !== "mismatch").length} 条结果`,
        "success",
        5000,
        best ? `最低 ${money(best.displayPrice ?? best.price, best.displayPrice != null ? app.displayCurrency : best.currency)} · ${platforms.label(best.platformId)}` : undefined,
      );
    } else if (r.status === "error") ui.toast("检索失败：" + (r.error ?? ""), "error", 5000);
  });
});
onBeforeUnmount(() => off?.());

function goRun() {
  if (search.active[0]) {
    search.currentId = search.active[0].runId;
    void router.push("/discover/search");
  }
}
</script>

<template>
  <div class="shell">
    <Sidebar />
    <main class="main">
      <TopBar :el-ref="(el: HTMLElement | null) => (topbarEl = el)" @go-run="goRun" />
      <div ref="scroller" class="main-scroll">
        <router-view v-slot="{ Component, route: r }">
          <Transition name="page" mode="out-in">
            <component :is="Component" :key="r.path" />
          </Transition>
        </router-view>
      </div>
    </main>
  </div>
</template>

<style scoped>
.shell {
  height: 100%;
  display: flex;
  gap: 0;
  padding: 0 10px 10px 10px;
}
.main {
  position: relative;
  flex: 1;
  min-width: 0;
  margin-left: 10px;
  border-radius: var(--r-xl);
  overflow: hidden;
}
.main-scroll {
  position: absolute;
  inset: 0;
  overflow-y: auto;
  overflow-x: hidden;
  padding-top: 62px;
  scroll-behavior: smooth;
}
</style>
