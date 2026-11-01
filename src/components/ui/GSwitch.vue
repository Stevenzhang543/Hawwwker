<script setup lang="ts">
const props = defineProps<{ modelValue: boolean; disabled?: boolean; small?: boolean }>();
const emit = defineEmits<{ "update:modelValue": [boolean] }>();
function toggle() {
  if (!props.disabled) emit("update:modelValue", !props.modelValue);
}
</script>

<template>
  <button
    type="button"
    role="switch"
    class="sw"
    :class="{ on: modelValue, small, disabled }"
    :aria-checked="modelValue"
    :disabled="disabled"
    @click.stop="toggle"
  >
    <i></i>
  </button>
</template>

<style scoped>
.sw {
  --w: 44px;
  --h: 26px;
  position: relative;
  width: var(--w);
  height: var(--h);
  flex: none;
  border-radius: 999px;
  border: none;
  padding: 0;
  cursor: pointer;
  background: var(--line-2);
  transition: background 260ms var(--ease-out), box-shadow 260ms;
}
.sw.small {
  --w: 36px;
  --h: 21px;
}
.sw i {
  position: absolute;
  top: 2px;
  left: 2px;
  width: calc(var(--h) - 4px);
  height: calc(var(--h) - 4px);
  border-radius: 999px;
  background: #fff;
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.22);
  transition: transform 460ms var(--spring), width 200ms var(--ease-out);
}
.sw:active i {
  width: calc(var(--h) + 2px);
}
.sw.on {
  background: var(--brand-grad);
  box-shadow: 0 4px 12px rgba(61, 110, 255, 0.3);
}
.sw.on i {
  transform: translateX(calc(var(--w) - var(--h)));
}
.sw.on:active i {
  transform: translateX(calc(var(--w) - var(--h) - 6px));
}
.sw.disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
</style>
