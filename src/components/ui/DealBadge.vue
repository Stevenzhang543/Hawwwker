<script setup lang="ts">
/** 价格判断徽章：历史最低 / 近期低位 / 价格正常 / 偏高 / 数据积累中。悬停显示依据。 */
import { computed } from "vue";
import type { DealInfo } from "@/api/types";
import Icon from "./Icon.vue";

const props = withDefaults(defineProps<{ deal?: DealInfo | null; compact?: boolean; showNew?: boolean }>(), { compact: false, showNew: false });

const tone = computed(() => {
  switch (props.deal?.level) {
    case "lowest":
      return { cls: "green", icon: "star" };
    case "low":
      return { cls: "green", icon: "drop" };
    case "normal":
      return { cls: "blue", icon: "check" };
    case "high":
      return { cls: "orange", icon: "rise" };
    default:
      return { cls: "outline", icon: "clock" };
  }
});
const visible = computed(() => !!props.deal && (props.deal.level !== "new" || props.showNew));
</script>

<template>
  <span v-if="visible && deal" class="chip deal" :class="[tone.cls, { compact }]" :title="deal.basis" :aria-label="`${deal.label}：${deal.basis}`">
    <Icon :name="tone.icon" :size="compact ? 11 : 12" />
    <span>{{ deal.label }}</span>
  </span>
</template>

<style scoped>
.deal {
  white-space: nowrap;
  cursor: help;
}
.deal.compact {
  font-size: 11px;
  padding: 1px 6px;
  gap: 3px;
}
</style>
