<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { api } from "@/api";
import type { AiUsage } from "@/api/types";
import { useAppStore } from "@/stores/app";
import { useUiStore } from "@/stores/ui";
import { ago, compactNum } from "@/utils/format";
import AiConfigForm from "@/components/settings/AiConfigForm.vue";
import Icon from "@/components/ui/Icon.vue";
import Bars from "@/components/charts/Bars.vue";

const app = useAppStore();
const ui = useUiStore();
const route = useRoute();
const router = useRouter();
const usage = ref<AiUsage | null>(null);

onMounted(async () => {
  try {
    usage.value = await api.aiUsage();
  } catch {
    /* ignore */
  }
});
function onVerified() {
  ui.toast("AI 已连接，所有功能已解锁", "success", 3000);
  if (route.query.gate) void router.push(app.homeRoute);
}
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1>AI 模型</h1>
        <p>Hawwwker 必须接入 AI 才能使用：规划搜索词、抽取商品信息、校验匹配度、生成总结与 Copilot 都依赖它。</p>
      </div>
    </div>

    <div v-if="!app.ai.verified" class="banner">
      <Icon name="warn" />
      <span>{{ app.ai.lastError ? `AI 连接已失效：${app.ai.lastError}` : "AI 尚未连接" }}。完成下方测试后即可继续使用搜索、追踪与洞察。</span>
    </div>

    <div class="layout">
      <section class="glass card" style="flex: 1; min-width: 0">
        <div class="card-head">
          <h3>连接</h3>
          <span class="chip" :class="app.ai.verified ? 'green' : 'red'">{{ app.ai.verified ? `已连接 · ${app.ai.model}` : "未连接" }}</span>
          <span v-if="app.ai.verifiedAt" class="faint tiny">{{ ago(app.ai.verifiedAt) }}验证</span>
        </div>
        <AiConfigForm :auto-detect="!app.ai.verified" @verified="onVerified" />
      </section>

      <aside class="side">
        <section class="glass card">
          <div class="card-head"><h3>用量</h3><span class="faint tiny">本机统计</span></div>
          <div v-if="usage" class="stack" style="gap: 12px">
            <div class="grid g2">
              <div class="u"><span class="faint tiny">今日调用</span><b class="num">{{ usage.today.calls }}</b></div>
              <div class="u"><span class="faint tiny">今日 tokens</span><b class="num">{{ compactNum(usage.today.promptTokens + usage.today.completionTokens) }}</b></div>
              <div class="u"><span class="faint tiny">累计调用</span><b class="num">{{ compactNum(usage.total.calls) }}</b></div>
              <div class="u"><span class="faint tiny">累计 tokens</span><b class="num">{{ compactNum(usage.total.promptTokens + usage.total.completionTokens) }}</b></div>
            </div>
            <Bars :data="usage.days.slice(-10).map((d) => ({ label: String(Number(d.day.slice(8))), value: d.calls }))" vertical :height="120" />
          </div>
          <div v-else class="skel" style="height: 180px"></div>
        </section>
        <section class="glass card tips">
          <div class="card-head"><h3>AI 在哪些环节工作</h3></div>
          <ol>
            <li><b>规划</b>：为每个平台生成本地语言搜索词、预算与排除项</li>
            <li><b>抽取</b>：规则识别不出的卡片交给快速模型补全</li>
            <li><b>校验</b>：逐条判断精确 / 接近 / 相关 / 不符</li>
            <li><b>总结</b>：推荐款、价格洞察与注意事项</li>
            <li><b>Copilot</b>：礼物构思、比价、行业判断</li>
          </ol>
          <p class="faint tiny">只发送商品公开信息与你的指令，不发送 Cookie、账号或个人信息。API Key 只保存在本机，界面不会回显。</p>
        </section>
      </aside>
    </div>
  </div>
</template>

<style scoped>
.layout {
  display: flex;
  gap: 16px;
  align-items: flex-start;
}
.side {
  width: 320px;
  flex: none;
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.u {
  display: flex;
  flex-direction: column;
  padding: 10px 12px;
  border-radius: 12px;
  background: var(--hover);
}
.u b {
  font-size: 19px;
}
.tips ol {
  margin: 0 0 10px;
  padding-left: 18px;
  display: grid;
  gap: 5px;
  font-size: 12.5px;
  color: var(--text-2);
}
@media (max-width: 1180px) {
  .layout {
    flex-direction: column;
  }
  .side {
    width: 100%;
  }
}
</style>
