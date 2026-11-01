<script setup lang="ts">
/* 横向条形图 / 纵向柱状图 */
import { computed } from "vue";

const props = withDefaults(
  defineProps<{ data: { label: string; value: number; color?: string; sub?: string }[]; vertical?: boolean; format?: (v: number) => string; height?: number }>(),
  { vertical: false, height: 170 },
);
const max = computed(() => Math.max(1, ...props.data.map((d) => d.value)));
const fmt = (v: number) => (props.format ? props.format(v) : new Intl.NumberFormat("zh-CN").format(v));
</script>

<template>
  <div v-if="!vertical" class="hbars">
    <div v-for="(d, i) in data" :key="d.label" class="hb" :style="{ '--i': i }">
      <span class="hb-label ellipsis" :title="d.label"><slot name="label" :item="d">{{ d.label }}</slot></span>
      <span class="hb-track"><i :style="{ width: (d.value / max) * 100 + '%', background: d.color ?? 'var(--brand-grad)' }"></i></span>
      <span class="hb-val num">{{ fmt(d.value) }}<small v-if="d.sub">{{ d.sub }}</small></span>
    </div>
  </div>
  <div v-else class="vbars" :style="{ height: height + 'px' }">
    <div v-for="(d, i) in data" :key="d.label" class="vb">
      <span class="vb-val num">{{ fmt(d.value) }}</span>
      <span class="vb-col"><i :style="{ height: (d.value / max) * 100 + '%', background: d.color ?? 'var(--brand-grad)', animationDelay: i * 60 + 'ms' }"></i></span>
      <span class="vb-label">{{ d.label }}</span>
    </div>
  </div>
</template>

<style scoped>
.hbars {
  display: flex;
  flex-direction: column;
  gap: 9px;
}
.hb {
  display: grid;
  grid-template-columns: 150px 1fr 110px;
  align-items: center;
  gap: 10px;
  font-size: 12.5px;
}
.hb-label {
  color: var(--text-2);
  display: flex;
  align-items: center;
  min-width: 0;
}
.hb-track {
  height: 10px;
  border-radius: 6px;
  background: var(--hover);
  overflow: hidden;
}
.hb-track i {
  display: block;
  height: 100%;
  border-radius: 6px;
  transform-origin: left;
  animation: growx 900ms var(--spring-soft) both;
  animation-delay: calc(var(--i) * 50ms);
}
@keyframes growx {
  from {
    transform: scaleX(0);
  }
}
.hb-val {
  text-align: right;
  font-weight: 600;
}
.hb-val small {
  margin-left: 6px;
  font-weight: 500;
  color: var(--text-3);
}
.vbars {
  display: flex;
  align-items: stretch;
  gap: 10px;
}
.vb {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.vb-col {
  flex: 1;
  width: 100%;
  display: flex;
  align-items: flex-end;
  border-radius: 8px;
  background: var(--hover);
  overflow: hidden;
}
.vb-col i {
  display: block;
  width: 100%;
  border-radius: 8px;
  transform-origin: bottom;
  animation: growy 900ms var(--spring-soft) both;
}
@keyframes growy {
  from {
    transform: scaleY(0);
  }
}
.vb-val {
  font-size: 12px;
  font-weight: 600;
}
.vb-label {
  font-size: 11.5px;
  color: var(--text-3);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}
</style>
