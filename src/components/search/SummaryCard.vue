<script setup lang="ts">
import { computed, toRef } from "vue";
import type { AiSummary, CopilotAction, ResultItem } from "@/api/types";
import { useTypewriter } from "@/composables/motion";
import { usePlatformStore } from "@/stores/platforms";
import Icon from "@/components/ui/Icon.vue";
import PriceTag from "@/components/ui/PriceTag.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";

const props = defineProps<{ summary: AiSummary; results: ResultItem[] }>();
const emit = defineEmits<{ action: [CopilotAction]; open: [ResultItem] }>();
const platforms = usePlatformStore();
const headline = useTypewriter(toRef(() => props.summary.headline), 70);
const best = computed(() => (props.summary.bestPick ? props.results.find((r) => r.id === props.summary.bestPick!.id) ?? null : null));
const actIcon: Record<CopilotAction["type"], string> = { search: "search", watch: "tag", open: "external", navigate: "arrow" };
</script>

<template>
  <section class="sum glass">
    <div class="sum-glow"></div>
    <div class="sum-head">
      <span class="s-ico"><Icon name="spark" :size="16" /></span>
      <span class="eyebrow">AI 总结</span>
    </div>
    <h2 class="headline">{{ headline }}<span v-if="headline.length < summary.headline.length" class="caret"></span></h2>

    <div class="sum-body">
      <div class="left">
        <ul class="bullets">
          <li v-for="(b, i) in summary.bullets" :key="i" :style="{ '--i': i }">{{ b }}</li>
        </ul>
        <p v-if="summary.priceInsight" class="insight"><Icon name="trend" :size="15" />{{ summary.priceInsight }}</p>
        <div v-if="summary.caveats.length" class="caveats">
          <span v-for="(c, i) in summary.caveats" :key="i" class="chip orange"><Icon name="info" />{{ c }}</span>
        </div>
      </div>

      <button v-if="best" v-tilt="3" class="best glass-2" @click="emit('open', best)">
        <span class="best-tag">推荐</span>
        <img v-if="best.image" :src="best.image" alt="" />
        <div class="best-info">
          <PlatformBadge :id="best.platformId" />
          <p class="clamp2 best-title">{{ best.title }}</p>
          <PriceTag :price="best.price" :currency="best.currency" :display="best.displayPrice" amber />
          <p class="why">{{ summary.bestPick?.why }}</p>
        </div>
      </button>
    </div>

    <div v-if="summary.nextActions.length" class="acts">
      <span class="faint tiny">下一步</span>
      <button v-for="(a, i) in summary.nextActions" :key="i" v-ripple class="btn sm" @click="emit('action', a)">
        <Icon :name="actIcon[a.type]" />{{ a.label }}
      </button>
      <span class="spacer"></span>
      <span class="faint tiny">{{ platforms.list.length ? "结论基于本次抓取的公开信息，下单前请在平台确认" : "" }}</span>
    </div>
  </section>
</template>

<style scoped>
.sum {
  padding: 18px 20px;
  overflow: hidden;
}
.sum-glow {
  position: absolute;
  inset: -40% -10% auto auto;
  width: 360px;
  height: 360px;
  background: radial-gradient(circle, rgba(138, 92, 255, 0.22), transparent 65%);
  pointer-events: none;
  z-index: -1;
}
.sum-head {
  display: flex;
  align-items: center;
  gap: 8px;
}
.s-ico {
  width: 26px;
  height: 26px;
  border-radius: 8px;
  display: grid;
  place-items: center;
  background: var(--brand-grad);
  color: #fff;
}
.headline {
  margin: 10px 0 12px;
  font-size: 20px;
  font-weight: 700;
  line-height: 1.4;
  letter-spacing: -0.01em;
  min-height: 28px;
}
.caret {
  display: inline-block;
  width: 3px;
  height: 20px;
  margin-left: 2px;
  vertical-align: -3px;
  background: var(--brand);
  animation: pulse 0.8s infinite;
}
.sum-body {
  display: flex;
  gap: 18px;
  align-items: flex-start;
}
.left {
  flex: 1;
  min-width: 0;
}
.bullets {
  margin: 0;
  padding-left: 18px;
  display: grid;
  gap: 4px;
  color: var(--text-2);
}
.bullets li {
  animation: rise 500ms var(--spring-soft) both;
  animation-delay: calc(400ms + var(--i) * 90ms);
}
.insight {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  margin-top: 10px;
  padding: 10px 12px;
  border-radius: 12px;
  background: var(--info-bg);
  color: var(--brand-ink);
  font-size: 13px;
}
.insight :deep(.ico) {
  margin-top: 2px;
}
.caveats {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 10px;
}
.best {
  position: relative;
  width: 300px;
  flex: none;
  display: flex;
  gap: 12px;
  padding: 12px;
  border-radius: 18px;
  text-align: left;
  cursor: pointer;
  border: 1px solid rgba(255, 176, 32, 0.45);
}
.best-tag {
  position: absolute;
  top: -9px;
  left: 14px;
  padding: 1px 9px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 700;
  background: linear-gradient(135deg, #ffbe3d, #ff9a1f);
  color: #2b1800;
  z-index: 3;
}
.best img {
  width: 84px;
  height: 84px;
  border-radius: 12px;
  object-fit: cover;
  flex: none;
}
.best-info {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.best-title {
  font-size: 12.5px;
  font-weight: 560;
}
.why {
  font-size: 11.5px;
  color: var(--text-3);
}
.acts {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px solid var(--line);
}
@media (max-width: 1180px) {
  .sum-body {
    flex-direction: column;
  }
  .best {
    width: 100%;
  }
}
</style>
