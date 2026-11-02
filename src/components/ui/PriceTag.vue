<script setup lang="ts">
/**
 * 价格显示。传入 landed（到手价，原币种）时以到手价为主，显示"到手"前缀；
 * 不传 landed 时与旧版行为一致（其他页面沿用）。
 */
import { computed } from "vue";
import { useAppStore } from "@/stores/app";
import { money } from "@/utils/format";

const props = withDefaults(
  defineProps<{
    price?: number | null;
    currency: string;
    display?: number | null;
    original?: number | null;
    size?: "sm" | "md" | "lg";
    amber?: boolean;
    /** 到手价（原币种） */
    landed?: number | null;
    /** 到手价换算到显示币种 */
    displayLanded?: number | null;
    /** 是否显示"到手"前缀（有运费或券信息时才有意义） */
    landedLabel?: boolean;
  }>(),
  { size: "md", amber: false, landedLabel: false },
);
const app = useAppStore();
const main = computed(() => props.landed ?? props.price);
const showConv = computed(() => props.currency !== app.displayCurrency && main.value != null);
const conv = computed(() => {
  if (props.landed != null) return props.displayLanded ?? app.toDisplay(props.landed, props.currency);
  return props.display ?? app.toDisplay(props.price, props.currency);
});
const strike = computed(() => (props.original && main.value != null && props.original > main.value ? props.original : null));
</script>

<template>
  <span class="pt" :class="size">
    <span v-if="landedLabel && landed != null" class="lbl">到手</span>
    <span class="price" :class="{ amber }">{{ money(main, currency) }}</span>
    <span v-if="strike" class="strike orig">{{ money(strike, currency) }}</span>
    <span v-if="showConv && conv != null" class="conv">≈ {{ money(conv, app.displayCurrency) }}</span>
  </span>
</template>

<style scoped>
.pt {
  display: inline-flex;
  align-items: baseline;
  flex-wrap: wrap;
  gap: 2px 8px;
}
.lbl {
  font-size: 11px;
  font-weight: 650;
  color: var(--text-3);
  margin-right: -4px;
}
.lg .lbl {
  font-size: 13px;
}
.price {
  font-size: 18px;
}
.sm .price {
  font-size: 14px;
}
.lg .price {
  font-size: 26px;
}
.orig {
  font-size: 12px;
}
.conv {
  font-size: 12px;
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}
</style>
