<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { NOTICE_META, useNoticeStore } from "@/stores/notices";
import { ago } from "@/utils/format";
import Icon from "@/components/ui/Icon.vue";
import type { Notice } from "@/api/types";

const emit = defineEmits<{ close: [] }>();
const notices = useNoticeStore();
const router = useRouter();
const root = ref<HTMLElement | null>(null);
const recent = computed(() => notices.list.slice(0, 7));

function open(n: Notice) {
  void notices.read(n.id);
  if (n.link) void router.push(n.link);
  emit("close");
}
function onDoc(e: MouseEvent) {
  if (root.value && !root.value.parentElement?.contains(e.target as Node)) emit("close");
}
onMounted(() => {
  if (!notices.loaded) void notices.load();
  setTimeout(() => document.addEventListener("pointerdown", onDoc), 0);
});
onBeforeUnmount(() => document.removeEventListener("pointerdown", onDoc));
</script>

<template>
  <div ref="root" class="pop glass-3">
    <div class="pop-head">
      <b>提醒</b>
      <span class="chip blue" v-if="notices.unread">{{ notices.unread }} 条未读</span>
      <span class="spacer"></span>
      <button class="btn ghost sm" @click="notices.readAll()">全部已读</button>
    </div>
    <div v-if="!notices.loaded" class="col" style="padding: 10px">
      <div v-for="i in 3" :key="i" class="skel" style="height: 52px"></div>
    </div>
    <div v-else-if="!recent.length" class="empty" style="padding: 26px">暂无提醒</div>
    <div v-else class="pop-list">
      <button v-for="n in recent" :key="n.id" class="n" :class="{ unread: !n.read }" @click="open(n)">
        <span class="n-ico chip" :class="NOTICE_META[n.kind].tone"><Icon :name="NOTICE_META[n.kind].icon" :size="15" /></span>
        <span class="n-body">
          <b class="ellipsis">{{ n.title }}</b>
          <small class="clamp2">{{ n.body }}</small>
        </span>
        <small class="n-time">{{ ago(n.createdAt) }}</small>
      </button>
    </div>
    <router-link to="/track/alerts" class="pop-foot" @click="emit('close')">查看全部提醒 <Icon name="arrow" :size="14" /></router-link>
  </div>
</template>

<style scoped>
.pop {
  position: absolute;
  right: 0;
  top: 42px;
  width: 380px;
  border-radius: 18px;
  padding: 6px;
  z-index: 80;
  animation: rise 360ms var(--spring-soft);
}
.pop-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
}
.pop-list {
  max-height: 420px;
  overflow-y: auto;
}
.n {
  width: 100%;
  display: flex;
  gap: 10px;
  align-items: flex-start;
  padding: 10px;
  border: none;
  background: transparent;
  text-align: left;
  border-radius: 12px;
  cursor: pointer;
  position: relative;
}
.n:hover {
  background: var(--hover);
}
.n.unread::before {
  content: "";
  position: absolute;
  left: 2px;
  top: 20px;
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--brand-1);
}
.n-ico {
  width: 30px;
  height: 30px;
  padding: 0;
  justify-content: center;
  border-radius: 10px;
  flex: none;
}
.n-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.n-body b {
  font-size: 13px;
  font-weight: 600;
}
.n-body small {
  font-size: 12px;
  color: var(--text-2);
}
.n-time {
  font-size: 11px;
  color: var(--text-3);
  white-space: nowrap;
}
.pop-foot {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 10px;
  font-size: 12.5px;
  border-top: 1px solid var(--line);
  margin-top: 4px;
}
</style>
