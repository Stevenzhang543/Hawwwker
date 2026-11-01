<script setup lang="ts">
import { computed } from "vue";
import { useAppStore } from "@/stores/app";
import { money } from "@/utils/format";

const props = withDefaults(
  defineProps<{ price?: number | null; currency: string; display?: number | null; original?: number | null; size?: "sm" | "md" | "lg"; amber?: boolean }>(),
  { size: "md", amber: false },
);
const app = useAppStore();
const showConv = computed(() => props.currency !== app.displayCurrency && props.price != null);
const conv = computed(() => props.display ?? app.toDisplay(props.price, props.currency));
</script>

<template>
  <span class="pt" :class="size">
    <span class="price" :class="{ amber }">{{ money(price, currency) }}</span>
    <span v-if="original && price != null && original > price" class="strike orig">{{ money(original, currency) }}</span>
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
