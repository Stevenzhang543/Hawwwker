<script setup lang="ts">
/* 灵动岛：顶部居中胶囊。有通知时形变展开；检索进行中常驻显示迷你进度。 */
import { computed } from "vue";
import { useRouter } from "vue-router";
import { useUiStore } from "@/stores/ui";
import { useSearchStore } from "@/stores/search";
import Icon from "@/components/ui/Icon.vue";

const ui = useUiStore();
const search = useSearchStore();
const router = useRouter();

const run = computed(() => search.active[0] ?? null);
const runText = computed(() => {
  const r = run.value;
  if (!r) return "";
  const ps = Object.values(r.platforms);
  const done = ps.filter((p) => ["done", "error", "skipped", "login_required"].includes(p.state)).length;
  return r.stage === "search" ? `正在搜索 ${done}/${ps.length} 个平台` : r.stageLabel;
});
const mode = computed(() => (ui.island ? "msg" : run.value ? "run" : "idle"));
const ICON: Record<"info" | "success" | "warn" | "error" | "ai", string> = { info: "info", success: "check", warn: "warn", error: "warn", ai: "spark" };

function click() {
  if (mode.value === "run" && run.value) {
    search.currentId = run.value.runId;
    void router.push("/discover/search");
  } else ui.hideIsland();
}
</script>

<template>
  <div class="island-wrap">
    <button class="island" :class="[mode, ui.island?.kind]" @click="click">
      <Transition name="isl" mode="out-in">
        <span v-if="mode === 'msg' && ui.island" :key="'m' + ui.island.id" class="isl-row">
          <span class="isl-ico"><Icon :name="ICON[ui.island.kind]" :size="15" :stroke="2.2" /></span>
          <span class="isl-text">
            <b>{{ ui.island.text }}</b>
            <small v-if="ui.island.sub">{{ ui.island.sub }}</small>
          </span>
        </span>
        <span v-else-if="mode === 'run' && run" key="run" class="isl-row">
          <span class="isl-orb"></span>
          <span class="isl-text"><b>{{ runText }}</b></span>
          <span class="isl-bar"><i :style="{ width: run.progress * 100 + '%' }"></i></span>
        </span>
        <span v-else key="idle"></span>
      </Transition>
    </button>
  </div>
</template>

<style scoped>
.island-wrap {
  position: fixed;
  z-index: 900;
  top: 5px;
  left: 50%;
  translate: -50% 0;
  pointer-events: none;
}
.island {
  pointer-events: auto;
  display: flex;
  align-items: center;
  justify-content: center;
  min-width: 0;
  max-width: 560px;
  height: 28px;
  padding: 0;
  border: none;
  border-radius: 999px;
  background: #0d0f16;
  color: #fff;
  cursor: pointer;
  overflow: hidden;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.25);
  transition: width 520ms var(--spring), height 520ms var(--spring), padding 420ms var(--spring), opacity 260ms, transform 420ms var(--spring);
}
.island.idle {
  width: 0;
  height: 0;
  opacity: 0;
  pointer-events: none;
}
.island.msg {
  height: auto;
  min-height: 44px;
  padding: 8px 18px 8px 9px;
}
.island.run {
  height: 30px;
  padding: 0 14px 0 9px;
}
.isl-row {
  display: flex;
  align-items: center;
  gap: 10px;
  white-space: nowrap;
}
.isl-ico {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  display: grid;
  place-items: center;
  background: #3d7bff;
  flex: none;
}
.island.success .isl-ico {
  background: #2fd27a;
  color: #04260f;
}
.island.warn .isl-ico {
  background: #ff9f0a;
  color: #2b1600;
}
.island.error .isl-ico {
  background: #ff4d4f;
}
.island.ai .isl-ico {
  background: linear-gradient(135deg, #3d7bff, #8a5cff);
}
.isl-text {
  display: flex;
  flex-direction: column;
  text-align: left;
  line-height: 1.3;
  min-width: 0;
}
.isl-text b {
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 440px;
}
.isl-text small {
  font-size: 11.5px;
  opacity: 0.72;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 440px;
}
.island.run .isl-text b {
  font-size: 12px;
}
.isl-orb {
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: conic-gradient(from 0deg, #ffb020, #3d7bff, #8a5cff, #ffb020);
  animation: spin 1.2s linear infinite;
}
.isl-bar {
  width: 70px;
  height: 4px;
  border-radius: 2px;
  background: rgba(255, 255, 255, 0.18);
  overflow: hidden;
}
.isl-bar i {
  display: block;
  height: 100%;
  background: linear-gradient(90deg, #ffb020, #ffd27a);
  border-radius: 2px;
  transition: width 500ms var(--ease-out);
}
.isl-enter-active {
  transition: opacity 260ms 120ms, filter 320ms 120ms, transform 420ms 80ms var(--spring);
}
.isl-leave-active {
  transition: opacity 120ms, filter 120ms;
}
.isl-enter-from {
  opacity: 0;
  filter: blur(6px);
  transform: scale(0.9);
}
.isl-leave-to {
  opacity: 0;
  filter: blur(6px);
}
</style>
