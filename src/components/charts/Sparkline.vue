<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(defineProps<{ data: number[]; width?: number; height?: number; color?: string; fill?: boolean }>(), {
  width: 120,
  height: 34,
  fill: true,
});
const uid = "sp" + Math.random().toString(36).slice(2, 7);
const trend = computed(() => {
  const d = props.data;
  if (d.length < 2) return 0;
  return d[d.length - 1]! - d[0]!;
});
const stroke = computed(() => props.color ?? (trend.value < 0 ? "#2fd27a" : trend.value > 0 ? "#ff5a5f" : "#8a95a8"));
const pts = computed(() => {
  const d = props.data;
  if (!d.length) return [] as [number, number][];
  const lo = Math.min(...d), hi = Math.max(...d);
  const span = hi - lo || 1;
  return d.map((v, i) => [(i / Math.max(1, d.length - 1)) * (props.width - 4) + 2, props.height - 3 - ((v - lo) / span) * (props.height - 6)] as [number, number]);
});
const line = computed(() => pts.value.map((p, i) => `${i ? "L" : "M"}${p[0].toFixed(1)},${p[1].toFixed(1)}`).join(""));
const area = computed(() => (pts.value.length ? `${line.value}L${pts.value[pts.value.length - 1]![0]},${props.height}L${pts.value[0]![0]},${props.height}Z` : ""));
</script>

<template>
  <svg :width="width" :height="height" class="spark" :viewBox="`0 0 ${width} ${height}`">
    <defs>
      <linearGradient :id="uid" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" :stop-color="stroke" stop-opacity=".3" />
        <stop offset="1" :stop-color="stroke" stop-opacity="0" />
      </linearGradient>
    </defs>
    <path v-if="fill && area" :d="area" :fill="`url(#${uid})`" />
    <path :d="line" fill="none" :stroke="stroke" stroke-width="1.8" stroke-linejoin="round" stroke-linecap="round" />
    <circle v-if="pts.length" :cx="pts[pts.length - 1]![0]" :cy="pts[pts.length - 1]![1]" r="2.6" :fill="stroke" />
  </svg>
</template>

<style scoped>
.spark {
  display: block;
  overflow: visible;
}
</style>
