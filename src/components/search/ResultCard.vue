<script setup lang="ts">
import { computed } from "vue";
import type { ResultItem } from "@/api/types";
import { VERDICT, FLAG } from "@/utils/labels";
import { compactNum } from "@/utils/format";
import { hasLandedInfo, landedChips } from "@/utils/landed";
import { RISK_LEVEL, TONE_CHIP, cardTags, isShopTag, reviewChip, riskTitle, scoreTone } from "./insight";
import Icon from "@/components/ui/Icon.vue";
import PriceTag from "@/components/ui/PriceTag.vue";
import DealBadge from "@/components/ui/DealBadge.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";
import Sparkline from "@/components/charts/Sparkline.vue";

/** pending：这条结果正在补充的信息（例如"评论读取中…"），由页面按当前阶段传入 */
const props = defineProps<{ item: ResultItem; best?: boolean; pending?: string | null; hiddenReason?: string | null }>();
const emit = defineEmits<{ open: []; watch: []; copilot: []; external: []; copy: []; irrelevant: []; restore: [] }>();

const tags = computed(() => cardTags(props.item));
const shopTag = computed(() => tags.value.find(isShopTag) ?? null);
const others = computed(() => tags.value.filter((t) => !isShopTag(t)));
const shownTags = computed(() => others.value.slice(0, 2));
const moreTags = computed(() => others.value.slice(2));
const risk = computed(() => props.item.risk ?? null);
const review = computed(() => reviewChip(props.item.reviewInsight));
const shopName = computed(() => props.item.shop ?? props.item.shopInfo?.name ?? null);
const reviewsCount = computed(() => props.item.reviews ?? props.item.reviewInsight?.count ?? null);
</script>

<template>
  <article v-tilt="2.5" class="rc glass" :class="[item.match.verdict, { best, provisional: item.provisional, marked: item.feedback === 'irrelevant' }]" @click="emit('open')">
    <div class="rc-media">
      <img v-if="item.image" :src="item.image" alt="" loading="lazy" />
      <div v-else class="noimg"><Icon name="box" :size="30" /></div>
      <span class="rc-plat"><PlatformBadge :id="item.platformId" /></span>
      <span v-if="item.provisional" class="rc-match chip checking" title="规则初判，AI 正在逐条核对，完成后原位更新">
        <Icon name="refresh" :size="11" class="spin" />AI 核对中
      </span>
      <span v-else-if="item.feedback === 'irrelevant'" class="rc-match chip red" title="你标记为不相关，同一需求以后会自动排除">已标记不相关</span>
      <span v-else class="rc-match chip" :class="VERDICT[item.match.verdict].tone" :title="VERDICT[item.match.verdict].desc">
        {{ VERDICT[item.match.verdict].label }} <b class="num">{{ item.match.score }}</b>
      </span>
      <span v-if="best" class="best-ribbon">AI 推荐</span>
      <span v-if="item.finalScore != null && !item.provisional" class="final" :class="scoreTone(item.finalScore)" title="综合分：匹配、风险、店铺、评论、价格加权">
        <b class="num">{{ item.finalScore }}</b><small>综合</small>
      </span>
    </div>
    <div class="rc-body">
      <h4 class="clamp2" :title="item.title">{{ item.title }}</h4>
      <div class="rc-price">
        <PriceTag
          :price="item.price"
          :currency="item.currency"
          :display="item.displayPrice"
          :original="item.originalPrice"
          :landed="item.landedPrice"
          :display-landed="item.displayLanded"
          :landed-label="hasLandedInfo(item)"
          :amber="best"
        />
        <Sparkline v-if="item.spark.length > 2" :data="item.spark" :width="64" :height="24" :fill="false" />
      </div>
      <div v-if="landedChips(item).length || item.deal" class="rc-landed">
        <DealBadge :deal="item.deal" compact />
        <span v-for="c in landedChips(item)" :key="c.text" class="chip mini" :class="c.tone" :title="c.title">{{ c.text }}</span>
      </div>
      <div v-if="tags.length || risk" class="rc-tags">
        <span v-if="shopTag" class="chip mini shop" :class="TONE_CHIP[shopTag.tone] ?? ''" :title="shopName ?? ''"><Icon name="shop" :size="11" />{{ shopTag.label }}</span>
        <span v-if="risk" class="chip mini risk" :class="RISK_LEVEL[risk.level].tone" :title="riskTitle(risk)">
          <Icon :name="RISK_LEVEL[risk.level].icon" :size="11" />{{ RISK_LEVEL[risk.level].label }} <b class="num">{{ risk.score }}</b>
        </span>
        <span v-for="t in shownTags" :key="t.kind + t.label" class="chip mini" :class="TONE_CHIP[t.tone] ?? ''">{{ t.label }}</span>
        <span v-if="moreTags.length" class="chip mini outline" :title="moreTags.map((t) => t.label).join('、')">+{{ moreTags.length }}</span>
      </div>
      <div class="rc-meta">
        <span v-if="shopName" class="ellipsis" :title="shopName"><Icon name="shop" :size="13" />{{ shopName }}</span>
        <span v-if="item.rating"><Icon name="star" :size="12" />{{ item.rating }}<template v-if="reviewsCount">（{{ compactNum(reviewsCount) }}）</template></span>
        <span v-if="item.sales">{{ compactNum(item.sales) }} 已售</span>
        <span v-if="item.condition === 'used'" class="chip orange">二手</span>
      </div>
      <div v-if="review || pending" class="rc-review">
        <span v-if="review" class="chip mini" :class="review.tone" :title="review.title"><Icon name="chat" :size="11" />{{ review.text }}</span>
        <span v-if="pending" class="pending"><Icon name="refresh" :size="11" class="spin" />{{ pending }}</span>
      </div>
      <p v-if="hiddenReason" class="hidden-reason"><Icon name="warn" :size="12" />{{ hiddenReason }}</p>
      <div class="rc-reasons">
        <span v-for="f in item.match.flags" :key="f" class="chip red">{{ FLAG[f] ?? f }}</span>
        <span v-for="r in item.match.reasons.slice(0, 2)" :key="r" class="reason">{{ r }}</span>
        <span v-if="item.feedback === 'irrelevant'" class="chip red">你标记为不相关</span>
        <span v-if="!item.match.aiChecked && !item.provisional" class="chip outline">AI 未校验</span>
      </div>
      <div v-if="item.alsoOn.length" class="also">
        <span class="faint tiny">也在</span>
        <PlatformBadge v-for="a in item.alsoOn.slice(0, 4)" :key="a.listingId" :id="a.platformId" :show-name="false" />
        <span class="faint tiny">{{ item.alsoOn.length }} 个平台</span>
      </div>
    </div>
    <div class="rc-actions" @click.stop>
      <button v-ripple class="btn sm" title="在平台中打开" @click="emit('external')"><Icon name="external" />打开</button>
      <button
        v-ripple
        class="btn sm"
        :class="{ watched: item.watched }"
        :disabled="item.watched || item.provisional"
        :title="item.provisional ? 'AI 核对完成后即可追踪' : ''"
        @click="emit('watch')"
      >
        <Icon :name="item.watched ? 'check' : 'tag'" />{{ item.watched ? "追踪中" : "追踪" }}
      </button>
      <span class="spacer"></span>
      <button class="icon-btn" title="复制链接" @click="emit('copy')"><Icon name="copy" :size="16" /></button>
      <button class="icon-btn" title="问 Copilot" @click="emit('copilot')"><Icon name="bot" :size="16" /></button>
      <button v-if="item.feedback === 'irrelevant'" class="icon-btn" title="撤销「不相关」" @click="emit('restore')"><Icon name="refresh" :size="16" /></button>
      <button v-else class="icon-btn fb" title="不相关：折叠这条，同一需求以后自动排除" @click="emit('irrelevant')"><Icon name="x" :size="16" /></button>
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
.rc-landed {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px;
  margin-top: -2px;
}
.chip.mini {
  height: 20px;
  padding: 0 7px;
  font-size: 11px;
}
.rc-match.checking {
  color: var(--purple);
  gap: 4px;
}
.rc.provisional {
  border-style: dashed;
}
.rc.provisional .rc-media::after {
  content: "";
  position: absolute;
  inset: 0;
  background: linear-gradient(110deg, transparent 30%, rgba(255, 255, 255, 0.16) 50%, transparent 70%);
  background-size: 250% 100%;
  animation: sheen 2.2s linear infinite;
  pointer-events: none;
}
@keyframes sheen {
  from {
    background-position: 120% 0;
  }
  to {
    background-position: -120% 0;
  }
}
.rc.marked {
  opacity: 0.6;
}
.icon-btn.fb:hover {
  color: var(--err);
}
.final {
  position: absolute;
  right: 10px;
  bottom: 10px;
  display: inline-flex;
  align-items: baseline;
  gap: 3px;
  padding: 2px 8px;
  border-radius: 10px;
  background: var(--glass-3);
  -webkit-backdrop-filter: blur(10px);
  backdrop-filter: blur(10px);
  color: var(--text-2);
}
.final b {
  font-size: 14px;
}
.final small {
  font-size: 10.5px;
  color: var(--text-3);
}
.final.green b {
  color: var(--ok);
}
.final.blue b {
  color: var(--brand-ink);
}
.final.orange b {
  color: var(--warn);
}
.final.red b {
  color: var(--err);
}
.rc-tags,
.rc-review {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px;
}
.chip.risk,
.chip.shop {
  gap: 3px;
}
.chip.risk b {
  font-weight: 700;
}
.pending {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  color: var(--purple);
}
.hidden-reason {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 11.5px;
  color: var(--err);
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
