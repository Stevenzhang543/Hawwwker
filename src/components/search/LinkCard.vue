<script setup lang="ts">
/* 粘贴商品链接后的识别卡：全网找同款比价 / 追踪这个价格 */
import type { LinkInfo } from "@/api/types";
import Icon from "@/components/ui/Icon.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";
import PriceTag from "@/components/ui/PriceTag.vue";

defineProps<{ info: LinkInfo | null; loading?: boolean; error?: string | null; url: string; watching?: boolean }>();
const emit = defineEmits<{ compare: []; watch: []; view: [number]; close: []; retry: [] }>();
</script>

<template>
  <section class="lc glass-2">
    <div v-if="loading" class="lc-row">
      <div class="skel lc-img"></div>
      <div class="lc-main">
        <span class="faint tiny"><Icon name="refresh" :size="12" class="spin" /> 正在读取商品页（在你的登录会话中打开，读完即关）…</span>
        <div class="skel" style="height: 16px; width: 70%"></div>
        <div class="skel" style="height: 14px; width: 30%"></div>
      </div>
    </div>

    <div v-else-if="error" class="lc-row">
      <span class="lc-ico err"><Icon name="warn" :size="18" /></span>
      <div class="lc-main">
        <b>没能读取这个链接</b>
        <span class="muted small">{{ error }}</span>
      </div>
      <button class="btn sm" @click="emit('retry')"><Icon name="refresh" />重试</button>
      <button class="icon-btn" title="关闭" @click="emit('close')"><Icon name="x" :size="16" /></button>
    </div>

    <div v-else-if="info" class="lc-row">
      <img v-if="info.image" :src="info.image" alt="" class="lc-img" />
      <span v-else class="lc-ico"><Icon name="link" :size="18" /></span>
      <div class="lc-main">
        <div class="row" style="gap: 8px">
          <PlatformBadge v-if="info.platformId" :id="info.platformId" />
          <span v-else class="chip outline">未识别平台</span>
          <span class="faint tiny ellipsis" :title="url">{{ url }}</span>
        </div>
        <b class="clamp2 lc-title" :title="info.title ?? ''">{{ info.title ?? "（没有读到标题）" }}</b>
        <div class="row" style="gap: 8px">
          <PriceTag v-if="info.price != null && info.currency" :price="info.price" :currency="info.currency" size="sm" />
          <span v-if="info.message" class="chip orange"><Icon name="info" />{{ info.message }}</span>
        </div>
      </div>
      <div class="lc-acts">
        <button v-ripple class="btn primary sm" @click="emit('compare')"><Icon name="scale" />全网找同款比价</button>
        <button v-if="info.watchId" class="btn sm" @click="emit('view', info.watchId)"><Icon name="check" />已在追踪 · 查看</button>
        <button v-else v-ripple class="btn sm amber" :disabled="watching" @click="emit('watch')">
          <Icon :name="watching ? 'refresh' : 'tag'" :class="{ spin: watching }" />追踪这个价格
        </button>
      </div>
      <button class="icon-btn" title="关闭" @click="emit('close')"><Icon name="x" :size="16" /></button>
    </div>
  </section>
</template>

<style scoped>
.lc {
  padding: 10px 12px;
  border-radius: 18px;
  animation: rise 420ms var(--spring-soft) both;
}
.lc-row {
  display: flex;
  align-items: center;
  gap: 12px;
}
.lc-img {
  width: 64px;
  height: 64px;
  border-radius: 12px;
  object-fit: cover;
  flex: none;
}
.lc-ico {
  width: 40px;
  height: 40px;
  border-radius: 12px;
  display: grid;
  place-items: center;
  background: var(--info-bg);
  color: var(--brand-ink);
  flex: none;
}
.lc-ico.err {
  background: var(--err-bg);
  color: var(--err);
}
.lc-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.lc-title {
  font-size: 13.5px;
  line-height: 1.4;
}
.lc-acts {
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex: none;
}
</style>
