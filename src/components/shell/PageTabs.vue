<script setup lang="ts">
/**
 * 二级页面标签：显示当前一级入口下的全部页面（侧栏只保留一级入口）。
 * 玻璃滑块跟随当前标签；徽标（未读、到价、检索中）与侧栏一致。
 */
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { useRoute } from "vue-router";
import { findGroup } from "@/nav";
import Icon from "@/components/ui/Icon.vue";
import { useNavBadges } from "./navBadges";

const route = useRoute();
const group = computed(() => findGroup(route.path));
const root = ref<HTMLElement | null>(null);
const glider = ref({ left: 0, width: 0, ready: false });
const badge = useNavBadges();

function measure() {
  const el = root.value?.querySelector<HTMLElement>("a.on");
  if (!el) {
    glider.value = { ...glider.value, ready: false };
    return;
  }
  glider.value = { left: el.offsetLeft, width: el.offsetWidth, ready: true };
}
watch(() => route.path, () => nextTick(measure));
onMounted(() => {
  nextTick(measure);
  setTimeout(measure, 160);
});
</script>

<template>
  <nav v-if="group" ref="root" class="ptabs" :aria-label="group.label">
    <span class="g-label"><Icon :name="group.icon" :size="15" />{{ group.label }}</span>
    <span class="track">
      <span class="glider" :class="{ ready: glider.ready }" :style="{ transform: `translateX(${glider.left - 3}px)`, width: glider.width + 'px' }"></span>
      <router-link v-for="it in group.items" :key="it.path" :to="it.path" :class="{ on: route.path.startsWith(it.path) }" :title="it.desc">
        <Icon :name="it.icon" :size="14" />
        <span>{{ it.label }}</span>
        <em v-if="it.tag" class="tag">{{ it.tag }}</em>
        <b v-if="badge(it.path) === 'live'" class="live"><i></i></b>
        <b v-else-if="badge(it.path)" class="badge">{{ badge(it.path) }}</b>
      </router-link>
    </span>
  </nav>
</template>

<style scoped>
.ptabs {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}
.g-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-weight: 680;
  font-size: 14px;
  color: var(--text);
  white-space: nowrap;
}
.g-label :deep(.ico) {
  color: var(--brand);
}
.track {
  position: relative;
  display: inline-flex;
  padding: 3px;
  border-radius: 12px;
  background: var(--hover);
  border: 1px solid var(--line);
  isolation: isolate;
  min-width: 0;
  overflow-x: auto;
  scrollbar-width: none;
}
.track::-webkit-scrollbar {
  display: none;
}
.glider {
  position: absolute;
  top: 3px;
  bottom: 3px;
  left: 3px;
  border-radius: 9px;
  background: var(--glass-3);
  box-shadow: var(--shadow-1), inset 0 0 0 1px var(--line);
  z-index: -1;
  opacity: 0;
}
.glider.ready {
  opacity: 1;
  transition: transform 520ms var(--spring), width 420ms var(--spring);
}
a {
  height: 28px;
  padding: 0 11px;
  border-radius: 9px;
  font-size: 12.5px;
  font-weight: 560;
  color: var(--text-2);
  white-space: nowrap;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  transition: color var(--t-fast), transform var(--t-mid) var(--spring);
}
a:hover {
  color: var(--text);
}
a:active {
  transform: scale(0.95);
}
a.on {
  color: var(--text);
  font-weight: 650;
}
a.on :deep(.ico) {
  color: var(--brand);
}
.tag {
  font-style: normal;
  font-size: 10px;
  padding: 0 5px;
  height: 15px;
  line-height: 15px;
  border-radius: 5px;
  background: var(--purple-bg);
  color: var(--purple);
  font-weight: 600;
}
.badge {
  min-width: 17px;
  height: 16px;
  padding: 0 5px;
  border-radius: 8px;
  background: var(--brand-grad);
  color: #fff;
  font-size: 10.5px;
  display: grid;
  place-items: center;
}
.live {
  width: 14px;
  height: 14px;
  display: grid;
  place-items: center;
}
.live i {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--amber);
  animation: ptping 1.4s infinite;
}
@keyframes ptping {
  70% {
    box-shadow: 0 0 0 6px rgba(255, 176, 32, 0);
  }
  0% {
    box-shadow: 0 0 0 0 rgba(255, 176, 32, 0.6);
  }
}
@media (max-width: 1180px) {
  .g-label {
    display: none;
  }
}
</style>
