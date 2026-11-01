<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import type { Notice, NoticeKind } from "@/api/types";
import { NOTICE_META, useNoticeStore } from "@/stores/notices";
import { ago, dateTime } from "@/utils/format";
import GSegmented from "@/components/ui/GSegmented.vue";
import Icon from "@/components/ui/Icon.vue";

const notices = useNoticeStore();
const router = useRouter();
const kind = ref<"all" | "unread" | NoticeKind>("all");

onMounted(() => {
  if (!notices.loaded) void notices.load();
});

const rows = computed(() =>
  notices.list.filter((n) => kind.value === "all" || (kind.value === "unread" ? !n.read : n.kind === kind.value)),
);
function open(n: Notice) {
  void notices.read(n.id);
  if (n.link) void router.push(n.link);
}
const opts = [
  { value: "all" as const, label: "全部" },
  { value: "unread" as const, label: "未读" },
  { value: "price_drop" as const, label: "降价" },
  { value: "target_hit" as const, label: "目标价" },
  { value: "new_items" as const, label: "新品" },
  { value: "login" as const, label: "登录" },
  { value: "error" as const, label: "错误" },
];
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1>提醒中心</h1>
        <p>降价、达到目标价、监控新品、登录过期与错误都会汇总在这里，同时推送系统通知。</p>
      </div>
      <span class="spacer"></span>
      <button class="btn" :disabled="!notices.unread" @click="notices.readAll()"><Icon name="check" />全部已读</button>
    </div>
    <GSegmented v-model="kind" :options="opts" small />

    <div v-if="!notices.loaded" class="col"><div v-for="i in 5" :key="i" class="skel" style="height: 70px"></div></div>
    <div v-else-if="!rows.length" class="empty glass">
      <div class="empty-icon"><Icon name="bell" /></div>
      <h3>这里很安静</h3>
      <p>追踪商品或创建监控后，价格变化会出现在这里。</p>
    </div>
    <div v-else class="timeline">
      <TransitionGroup name="list">
        <button v-for="n in rows" :key="n.id" class="tl glass" :class="{ unread: !n.read }" @click="open(n)">
          <span class="tl-ico chip" :class="NOTICE_META[n.kind].tone"><Icon :name="NOTICE_META[n.kind].icon" :size="17" /></span>
          <span class="tl-body">
            <span class="row" style="gap: 8px">
              <b>{{ n.title }}</b>
              <span class="chip" :class="NOTICE_META[n.kind].tone">{{ NOTICE_META[n.kind].label }}</span>
              <span v-if="!n.read" class="dot blue"></span>
            </span>
            <span class="muted small">{{ n.body }}</span>
          </span>
          <span class="tl-time">
            <span>{{ ago(n.createdAt) }}</span>
            <small class="faint">{{ dateTime(n.createdAt) }}</small>
          </span>
          <Icon v-if="n.link" name="chevron" :size="16" class="faint" />
        </button>
      </TransitionGroup>
    </div>
  </div>
</template>

<style scoped>
.timeline {
  display: flex;
  flex-direction: column;
  gap: 8px;
  position: relative;
}
.tl {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 14px 16px;
  text-align: left;
  cursor: pointer;
  border-radius: 18px;
  transition: transform var(--t-mid) var(--spring), box-shadow var(--t-mid);
  width: 100%;
}
.tl:hover {
  transform: translateX(3px);
  box-shadow: var(--shadow-2);
}
.tl:not(.unread) {
  opacity: 0.82;
}
.tl-ico {
  width: 38px;
  height: 38px;
  border-radius: 12px;
  padding: 0;
  justify-content: center;
  flex: none;
}
.tl-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.tl-time {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  font-size: 12.5px;
  color: var(--text-2);
  white-space: nowrap;
}
.tl-time small {
  font-size: 11px;
}
</style>
