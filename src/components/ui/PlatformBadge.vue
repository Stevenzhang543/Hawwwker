<script setup lang="ts">
import { computed } from "vue";
import { usePlatformStore } from "@/stores/platforms";

const props = withDefaults(defineProps<{ id: string; size?: "sm" | "md"; showName?: boolean; edition?: boolean }>(), {
  size: "sm",
  showName: true,
  edition: true,
});
const store = usePlatformStore();
const p = computed(() => store.byId[props.id]);
const name = computed(() => (p.value ? p.value.name : props.id));
const ed = computed(() => {
  const x = p.value;
  if (!x || !props.edition) return "";
  if (x.region === "intl" && store.list.filter((o) => o.family === x.family).length > 1) return x.country !== "GL" ? x.country : "国际";
  return "";
});
</script>

<template>
  <span class="pb" :class="size" :title="p ? `${p.name} · ${p.edition}` : id">
    <i class="glyph" :style="{ background: p?.color ?? '#8a95a8' }">{{ p?.glyph ?? "?" }}</i>
    <span v-if="showName" class="nm">{{ name }}<em v-if="ed">{{ ed }}</em></span>
  </span>
</template>

<style scoped>
.pb {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-2);
  min-width: 0;
}
.glyph {
  width: 18px;
  height: 18px;
  border-radius: 6px;
  display: grid;
  place-items: center;
  color: #fff;
  font-style: normal;
  font-size: 10.5px;
  font-weight: 700;
  flex: none;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.25);
}
.md .glyph {
  width: 26px;
  height: 26px;
  border-radius: 8px;
  font-size: 13px;
}
.md {
  font-size: 13.5px;
  color: var(--text);
}
.nm {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.nm em {
  font-style: normal;
  margin-left: 4px;
  padding: 0 5px;
  border-radius: 5px;
  font-size: 10.5px;
  background: var(--hover);
  color: var(--text-3);
}
</style>
