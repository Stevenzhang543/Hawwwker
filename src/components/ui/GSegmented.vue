<script setup lang="ts" generic="T extends string | number">
/* 分段控件：玻璃滑块用弹簧曲线跟随选中项 */
import { nextTick, onMounted, ref, watch } from "vue";

const props = defineProps<{ modelValue: T; options: { value: T; label: string; icon?: string }[]; small?: boolean }>();
const emit = defineEmits<{ "update:modelValue": [T] }>();
const root = ref<HTMLElement | null>(null);
const glider = ref({ left: 0, width: 0, ready: false });

function measure() {
  const el = root.value?.querySelector<HTMLElement>("button.on");
  if (!el || !root.value) return;
  glider.value = { left: el.offsetLeft, width: el.offsetWidth, ready: true };
}
watch(
  () => [props.modelValue, props.options.length],
  () => nextTick(measure),
);
onMounted(() => {
  nextTick(measure);
  setTimeout(measure, 120);
});
</script>

<template>
  <div ref="root" class="seg" :class="{ small }">
    <span class="glider" :class="{ ready: glider.ready }" :style="{ transform: `translateX(${glider.left - 3}px)`, width: glider.width + 'px' }"></span>
    <button
      v-for="o in options"
      :key="String(o.value)"
      type="button"
      :class="{ on: o.value === modelValue }"
      @click="emit('update:modelValue', o.value)"
    >
      <slot name="option" :option="o">{{ o.label }}</slot>
    </button>
  </div>
</template>

<style scoped>
.seg {
  position: relative;
  display: inline-flex;
  padding: 3px;
  border-radius: 12px;
  background: var(--hover);
  border: 1px solid var(--line);
  isolation: isolate;
}
.glider {
  position: absolute;
  top: 3px;
  bottom: 3px;
  left: 3px;
  border-radius: 9px;
  background: var(--glass-3);
  box-shadow: var(--shadow-1), inset 0 0 0 1px var(--line);
  z-index: -1;
  opacity: 0;
}
.glider.ready {
  opacity: 1;
  transition: transform 520ms var(--spring), width 420ms var(--spring);
}
button {
  height: 30px;
  padding: 0 13px;
  border: none;
  background: transparent;
  border-radius: 9px;
  font-size: 13px;
  font-weight: 560;
  color: var(--text-2);
  cursor: pointer;
  white-space: nowrap;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  transition: color var(--t-fast), transform var(--t-mid) var(--spring);
}
button:active {
  transform: scale(0.95);
}
button.on {
  color: var(--text);
  font-weight: 640;
}
.small button {
  height: 26px;
  padding: 0 10px;
  font-size: 12.5px;
}
</style>
