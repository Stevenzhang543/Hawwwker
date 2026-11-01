<script setup lang="ts">
import { computed, ref } from "vue";

const props = withDefaults(defineProps<{ data: { label: string; value: number; color: string }[]; size?: number; thickness?: number; center?: string; sub?: string }>(), {
  size: 170,
  thickness: 22,
});
const hover = ref<number | null>(null);
const total = computed(() => props.data.reduce((s, d) => s + d.value, 0) || 1);
const R = computed(() => props.size / 2 - props.thickness / 2 - 4);
const C = computed(() => 2 * Math.PI * R.value);
const segs = computed(() => {
  let acc = 0;
  return props.data.map((d) => {
    const len = (d.value / total.value) * C.value;
    const s = { ...d, len, off: -acc, pct: (d.value / total.value) * 100 };
    acc += len;
    return s;
  });
});
</script>

<template>
  <div class="donut">
    <svg :width="size" :height="size" :viewBox="`0 0 ${size} ${size}`">
      <g :transform="`rotate(-90 ${size / 2} ${size / 2})`">
        <circle :cx="size / 2" :cy="size / 2" :r="R" fill="none" stroke="var(--hover)" :stroke-width="thickness" />
        <circle
          v-for="(s, i) in segs"
          :key="s.label"
          :cx="size / 2"
          :cy="size / 2"
          :r="R"
          fill="none"
          :stroke="s.color"
          :stroke-width="hover === i ? thickness + 6 : thickness"
          :stroke-dasharray="`${Math.max(0, s.len - 2)} ${C}`"
          :stroke-dashoffset="s.off"
          class="seg"
          :style="{ animationDelay: i * 70 + 'ms' }"
          @pointerenter="hover = i"
          @pointerleave="hover = null"
        />
      </g>
      <text :x="size / 2" :y="size / 2 - 2" text-anchor="middle" class="c1">{{ hover != null ? segs[hover]!.pct.toFixed(1) + "%" : center }}</text>
      <text :x="size / 2" :y="size / 2 + 17" text-anchor="middle" class="c2">{{ hover != null ? segs[hover]!.label : sub }}</text>
    </svg>
    <ul class="legend">
      <li v-for="(s, i) in segs" :key="s.label" :class="{ on: hover === i }" @pointerenter="hover = i" @pointerleave="hover = null">
        <i :style="{ background: s.color }"></i><span>{{ s.label }}</span><b class="num">{{ s.pct.toFixed(0) }}%</b>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.donut {
  display: flex;
  align-items: center;
  gap: 18px;
}
.seg {
  transition: stroke-width 260ms var(--spring);
  animation: segin 900ms var(--ease-out) both;
  cursor: pointer;
}
@keyframes segin {
  from {
    stroke-dasharray: 0 2000;
  }
}
.c1 {
  font-size: 20px;
  font-weight: 700;
  fill: var(--text);
  font-variant-numeric: tabular-nums;
}
.c2 {
  font-size: 11.5px;
  fill: var(--text-3);
}
.legend {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 5px;
  font-size: 12.5px;
  flex: 1;
  min-width: 0;
}
.legend li {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 2px 6px;
  border-radius: 7px;
  cursor: default;
}
.legend li.on {
  background: var(--hover);
}
.legend i {
  width: 9px;
  height: 9px;
  border-radius: 3px;
  flex: none;
}
.legend span {
  flex: 1;
  color: var(--text-2);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
