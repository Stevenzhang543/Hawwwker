<script setup lang="ts">
/* 三级层：右侧抽屉 / 居中弹层。打开时舞台景深后退，Esc 关闭。 */
import { onBeforeUnmount, watch } from "vue";
import { useUiStore } from "@/stores/ui";
import Icon from "./Icon.vue";

const props = withDefaults(defineProps<{ open: boolean; title?: string; subtitle?: string; width?: number; center?: boolean }>(), {
  width: 560,
  center: false,
});
const emit = defineEmits<{ "update:open": [boolean]; close: [] }>();
const ui = useUiStore();
let release: (() => void) | null = null;

function close() {
  emit("update:open", false);
  emit("close");
}

watch(
  () => props.open,
  (v, old) => {
    if (v && !old) {
      ui.sheets += 1;
      release = ui.pushEsc(close);
    } else if (!v && old) {
      ui.sheets = Math.max(0, ui.sheets - 1);
      release?.();
      release = null;
    }
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  if (props.open) ui.sheets = Math.max(0, ui.sheets - 1);
  release?.();
});
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="open" class="sheet-backdrop dof-backdrop" @pointerdown.self="close"></div>
    </Transition>
    <Transition :name="center ? 'pop' : 'drawer'">
      <section v-if="open" class="sheet glass-3" :class="{ center }" :style="{ width: width + 'px' }" role="dialog" aria-modal="true">
        <header v-if="title || $slots.head" class="sheet-head">
          <div class="sheet-title">
            <slot name="head">
              <h3>{{ title }}</h3>
              <p v-if="subtitle" class="muted tiny">{{ subtitle }}</p>
            </slot>
          </div>
          <button class="icon-btn" title="关闭（Esc）" @click="close"><Icon name="x" /></button>
        </header>
        <div class="sheet-body">
          <slot />
        </div>
        <footer v-if="$slots.foot" class="sheet-foot">
          <slot name="foot" />
        </footer>
      </section>
    </Transition>
  </Teleport>
</template>

<style scoped>
.sheet-backdrop {
  position: fixed;
  inset: 0;
  z-index: 300;
  background: rgba(10, 14, 28, 0.22);
}
.sheet {
  position: fixed;
  z-index: 301;
  top: calc(var(--titlebar-h) + 8px);
  right: 10px;
  bottom: 10px;
  max-width: calc(100vw - 20px);
  border-radius: var(--r-xl);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.sheet.center {
  top: 50%;
  left: 50%;
  right: auto;
  bottom: auto;
  translate: -50% -50%;
  max-height: calc(100vh - 80px);
}
.sheet-head {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 18px 18px 12px 22px;
  border-bottom: 1px solid var(--line);
}
.sheet-title {
  flex: 1;
  min-width: 0;
}
.sheet-title h3 {
  font-size: 17px;
  font-weight: 680;
}
.sheet-body {
  flex: 1;
  overflow-y: auto;
  padding: 18px 22px 22px;
}
.sheet-foot {
  display: flex;
  align-items: center;
  gap: 10px;
  justify-content: flex-end;
  padding: 12px 18px;
  border-top: 1px solid var(--line);
}
</style>
