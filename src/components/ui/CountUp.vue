<script setup lang="ts">
import { computed, toRef } from "vue";
import { useCountUp } from "@/composables/motion";

const props = withDefaults(defineProps<{ value: number; digits?: number; prefix?: string; suffix?: string }>(), { digits: 0, prefix: "", suffix: "" });
const v = useCountUp(toRef(props, "value"));
const text = computed(() => props.prefix + new Intl.NumberFormat("zh-CN", { maximumFractionDigits: props.digits, minimumFractionDigits: props.digits }).format(v.value) + props.suffix);
</script>

<template>
  <span class="num">{{ text }}</span>
</template>
