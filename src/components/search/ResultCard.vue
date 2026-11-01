<script setup lang="ts">
import type { ResultItem } from "@/api/types";
import { VERDICT, FLAG } from "@/utils/labels";
import { compactNum } from "@/utils/format";
import Icon from "@/components/ui/Icon.vue";
import PriceTag from "@/components/ui/PriceTag.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";
import Sparkline from "@/components/charts/Sparkline.vue";

defineProps<{ item: ResultItem; best?: boolean }>();
const emit = defineEmits<{ open: []; watch: []; copilot: []; external: []; copy: [] }>();
</script>

<template>
  <article v-tilt="2.5" class="rc glass" :class="[item.match.verdict, { best }]" @click="emit('open')">
    <div class="rc-media">
      <img v-if="item.image" :src="item.image" alt="" loading="lazy" />
      <div v-else class="noimg"><Icon name="box" :size="30" /></div>
      <span class="rc-plat"><PlatformBadge :id="item.platformId" /></span>
      <span class="rc-match chip" :class="VERDICT[item.match.verdict].tone" :title="VERDICT[item.match.verdict].desc">
        {{ VERDICT[item.match.verdict].label }} <b class="num">{{ item.match.score }}</b>
      </span>
      <span v-if="best" class="best-ribbon">AI 推荐</span>
    </div>
    <div class="rc-body">
      <h4 class="clamp2" :title="item.title">{{ item.title }}</h4>
      <div class="rc-price">
        <PriceTag :price="item.price" :currency="item.currency" :display="item.displayPrice" :original="item.originalPrice" :amber="best" />
        <Sparkline v-if="item.spark.length > 2" :data="item.spark" :width="64" :height="24" :fill="false" />
      </div>
      <div class="rc-meta">
        <span v-if="item.shop" class="ellipsis"><Icon name="shop" :size="13" />{{ item.shop }}</span>
        <span v-if="item.rating"><Icon name="star" :size="12" />{{ item.rating }}</span>
        <span v-if="item.sales">{{ compactNum(item.sales) }} 已售</span>
        <span v-if="item.condition === 'used'" class="chip orange">二手</span>
      </div>
      <div class="rc-reasons">
        <span v-for="f in item.match.flags" :key="f" class="chip red">{{ FLAG[f] ?? f }}</span>
        <span v-for="r in item.match.reasons.slice(0, 2)" :key="r" class="reason">{{ r }}</span>
        <span v-if="!item.match.aiChecked" class="chip outline">AI 未校验</span>
      </div>
      <div v-if="item.alsoOn.length" class="also">
        <span class="faint tiny">也在</span>
        <PlatformBadge v-for="a in item.alsoOn.slice(0, 4)" :key="a.listingId" :id="a.platformId" :show-name="false" />
        <span class="faint tiny">{{ item.alsoOn.length }} 个平台</span>
      </div>
    </div>
    <div class="rc-actions" @click.stop>
      <button v-ripple class="btn sm" title="在平台中打开" @click="emit('external')"><Icon name="external" />打开</button>
      <button v-ripple class="btn sm" :class="{ watched: item.watched }" :disabled="item.watched" @click="emit('watch')">
        <Icon :name="item.watched ? 'check' : 'tag'" />{{ item.watched ? "追踪中" : "追踪" }}
      </button>
      <span class="spacer"></span>
      <button class="icon-btn" title="复制链接" @click="emit('copy')"><Icon name="copy" :size="16" /></button>
      <button class="icon-btn" title="问 Copilot" @click="emit('copilot')"><Icon name="bot" :size="16" /></button>
    </div>
  </article>
</template>

<style scoped>
.rc {
  display: flex;
  flex-direction: column;
  border-radius: 20px;
  overflow: hidden;
  cursor: pointer;
}
.rc:hover {
  box-shadow: var(--shadow-2);
}
.rc.best {
  border-color: rgba(255, 176, 32, 0.55);
  box-shadow: var(--shadow-1), 0 0 0 1px rgba(255, 176, 32, 0.35);
}
.rc-media {
  position: relative;
  aspect-ratio: 16 / 10;
  overflow: hidden;
  background: var(--hover);
}
.rc-media img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  transition: transform 700ms var(--ease-out);
}
.rc:hover .rc-media img {
  transform: scale(1.05);
}
.noimg {
  width: 100%;
  height: 100%;
  display: grid;
  place-items: center;
  color: var(--text-3);
}
.rc-plat {
  position: absolute;
  left: 10px;
  top: 10px;
  padding: 4px 8px 4px 4px;
  border-radius: 9px;
  background: var(--glass-3);
  -webkit-backdrop-filter: blur(10px);
  backdrop-filter: blur(10px);
  max-width: 62%;
}
.rc-match {
  position: absolute;
  right: 10px;
  top: 10px;
  -webkit-backdrop-filter: blur(10px);
  backdrop-filter: blur(10px);
  background: var(--glass-3);
}
.rc-match.green {
  color: var(--ok);
}
.rc-match.blue {
  color: var(--brand-ink);
}
.rc-match.orange {
  color: var(--warn);
}
.rc-match.red {
  color: var(--err);
}
.best-ribbon {
  position: absolute;
  left: 10px;
  bottom: 10px;
  padding: 2px 9px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 700;
  background: linear-gradient(135deg, #ffbe3d, #ff9a1f);
  color: #2b1800;
}
.rc-body {
  padding: 12px 14px 4px;
  display: flex;
  flex-direction: column;
  gap: 7px;
  flex: 1;
}
.rc-body h4 {
  font-size: 13.5px;
  font-weight: 600;
  line-height: 1.45;
  min-height: 39px;
}
.rc-price {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 8px;
}
.rc-meta {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
  color: var(--text-3);
  min-width: 0;
}
.rc-meta > span {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  flex: none;
}
.rc-meta > span.ellipsis {
  flex: 0 1 auto;
  min-width: 0;
  display: inline-block;
}
.rc-reasons {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 6px;
  align-items: center;
}
.reason {
  font-size: 11.5px;
  color: var(--text-2);
}
.reason + .reason::before {
  content: "· ";
  color: var(--text-3);
}
.also {
  display: flex;
  align-items: center;
  gap: 4px;
}
.rc-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 10px 10px 14px;
}
.btn.watched {
  color: var(--ok);
  opacity: 1;
}
.rc.mismatch {
  opacity: 0.78;
}
</style>
