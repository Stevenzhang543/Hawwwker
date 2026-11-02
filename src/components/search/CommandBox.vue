<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from "vue";
import Icon from "@/components/ui/Icon.vue";

/** link = 输入的是商品链接：按钮变成"识别链接" */
const props = defineProps<{ modelValue: string; busy?: boolean; compact?: boolean; placeholder?: string; link?: boolean }>();
const emit = defineEmits<{ "update:modelValue": [string]; run: []; escape: []; blur: [] }>();
const ta = ref<HTMLTextAreaElement | null>(null);
const focused = ref(false);

function grow() {
  const el = ta.value;
  if (!el) return;
  el.style.height = "auto";
  el.style.height = Math.min(160, el.scrollHeight) + "px";
}
watch(() => props.modelValue, () => nextTick(grow));
onMounted(grow);

function onKey(e: KeyboardEvent) {
  if (e.key === "Enter" && (e.ctrlKey || e.metaKey || !e.shiftKey) && !e.isComposing) {
    e.preventDefault();
    emit("run");
  } else if (e.key === "Escape") {
    emit("escape");
  }
}
defineExpose({ focus: () => ta.value?.focus() });
</script>

<template>
  <div v-spec class="cbox glass-2" :class="{ focused, compact, busy }">
    <span class="ai-badge" :class="{ link }" :title="link ? '识别到商品链接' : 'AI 规划 + 校验'"><Icon :name="link ? 'link' : 'spark'" :size="18" /></span>
    <textarea
      ref="ta"
      class="cbox-input"
      rows="1"
      :value="modelValue"
      :placeholder="placeholder ?? '一句话说出想找的东西（品牌型号、预算、平台、成色），或粘贴商品链接找同款比价'"
      @input="emit('update:modelValue', ($event.target as HTMLTextAreaElement).value)"
      @keydown="onKey"
      @focus="focused = true"
      @blur="focused = false; emit('blur')"
    ></textarea>
    <div class="cbox-right">
      <kbd v-if="!compact" class="hint">Enter</kbd>
      <button v-ripple v-magnet="0.2" class="run-btn" :disabled="busy || !modelValue.trim()" @click="emit('run')">
        <Icon v-if="busy" name="refresh" :size="17" class="spin" />
        <Icon v-else :name="link ? 'scale' : 'arrow'" :size="18" :stroke="2.2" />
        <span>{{ busy ? "处理中" : link ? "找同款" : "搜索" }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.cbox {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 12px 12px 12px 14px;
  border-radius: 22px;
  transition: box-shadow var(--t-mid), transform var(--t-mid) var(--spring), border-color var(--t-fast);
}
.cbox.focused {
  box-shadow: var(--shadow-2), 0 0 0 4px rgba(61, 123, 255, 0.14), inset 0 1px 0 rgba(255, 255, 255, 0.4);
  border-color: color-mix(in srgb, var(--brand-1) 45%, var(--glass-edge));
}
.cbox.busy::before {
  opacity: 1;
  background: conic-gradient(from var(--ang, 0deg), transparent 0 70%, rgba(255, 176, 32, 0.9), rgba(138, 92, 255, 0.9), transparent);
  animation: angle 1.6s linear infinite;
}
@property --ang {
  syntax: "<angle>";
  initial-value: 0deg;
  inherits: false;
}
@keyframes angle {
  to {
    --ang: 360deg;
  }
}
.ai-badge {
  width: 38px;
  height: 38px;
  flex: none;
  border-radius: 13px;
  display: grid;
  place-items: center;
  background: var(--brand-grad);
  color: #fff;
  box-shadow: 0 6px 16px rgba(80, 100, 255, 0.35);
  margin-top: 2px;
}
.ai-badge.link {
  background: linear-gradient(135deg, #2fd27a, #1aa85c);
  box-shadow: 0 6px 16px rgba(30, 170, 92, 0.35);
}
.cbox-input {
  flex: 1;
  border: none;
  outline: none;
  resize: none;
  background: transparent;
  font-size: 17px;
  line-height: 1.5;
  padding: 8px 0;
  min-height: 42px;
  max-height: 160px;
}
.compact .cbox-input {
  font-size: 15px;
}
.cbox-input::placeholder {
  color: var(--text-3);
}
.cbox-right {
  display: flex;
  align-items: center;
  gap: 10px;
  align-self: flex-end;
}
.run-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 42px;
  padding: 0 18px;
  border: none;
  border-radius: 14px;
  background: var(--brand-grad);
  color: #fff;
  font-weight: 650;
  font-size: 14.5px;
  cursor: pointer;
  box-shadow: 0 8px 22px rgba(61, 110, 255, 0.38);
  transition: transform var(--t-mid) var(--spring), box-shadow var(--t-mid), opacity var(--t-fast), filter var(--t-fast);
}
.run-btn:hover:not(:disabled) {
  filter: brightness(1.07);
  box-shadow: 0 12px 28px rgba(61, 110, 255, 0.46);
}
.run-btn:active:not(:disabled) {
  transform: scale(0.95);
}
.run-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  box-shadow: none;
}
</style>
