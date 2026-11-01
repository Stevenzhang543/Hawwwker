<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from "vue";
import { useRoute, useRouter } from "vue-router";
import TitleBar from "@/components/shell/TitleBar.vue";
import Island from "@/components/shell/Island.vue";
import CommandPalette from "@/components/shell/CommandPalette.vue";
import CopilotDrawer from "@/components/shell/CopilotDrawer.vue";
import ConfirmSheet from "@/components/ui/ConfirmSheet.vue";
import { useUiStore } from "@/stores/ui";
import { useAppStore } from "@/stores/app";
import { NAV } from "@/nav";

const ui = useUiStore();
const app = useAppStore();
const route = useRoute();
const router = useRouter();
const bare = computed(() => !!route.meta.bare);

function onKey(e: KeyboardEvent) {
  const mod = e.ctrlKey || e.metaKey;
  if (mod && e.key.toLowerCase() === "k") {
    e.preventDefault();
    if (!bare.value) ui.paletteOpen = !ui.paletteOpen;
    return;
  }
  if (mod && e.key.toLowerCase() === "j") {
    e.preventDefault();
    if (!bare.value && app.ai.verified) ui.copilotOpen = !ui.copilotOpen;
    return;
  }
  if (e.altKey && /^[1-5]$/.test(e.key) && !bare.value) {
    e.preventDefault();
    const g = NAV[Number(e.key) - 1];
    if (g?.items[0]) void router.push(g.items[0].path);
    return;
  }
  if (e.key === "Escape") {
    if (ui.popEsc()) e.preventDefault();
  }
}

onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="app-root" :class="{ bare }">
    <TitleBar />
    <div class="stage" :class="{ dof: ui.depth > 0 }">
      <div class="aurora" aria-hidden="true"><i class="a1"></i><i class="a2"></i><i class="a3"></i><i class="a4"></i><div class="grain"></div></div>
      <div v-if="app.bootError" class="boot-error glass-2">
        <h3>无法连接到 Hawwwker 内核</h3>
        <p class="muted">{{ app.bootError }}</p>
      </div>
      <router-view v-else v-slot="{ Component }">
        <Transition name="page" mode="out-in">
          <component :is="Component" :key="bare ? 'bare' : 'shell'" />
        </Transition>
      </router-view>
    </div>
    <Island />
    <CommandPalette />
    <CopilotDrawer />
    <ConfirmSheet />
  </div>
</template>

<style scoped>
.app-root {
  position: fixed;
  inset: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.stage {
  position: relative;
  flex: 1;
  min-height: 0;
}
.stage > :deep(*:not(.aurora)) {
  position: relative;
  z-index: 1;
}
.boot-error {
  margin: 80px auto;
  max-width: 460px;
  padding: 28px;
  text-align: center;
}
</style>
