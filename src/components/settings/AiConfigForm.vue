<script setup lang="ts">
/* AI 连接表单：预设 → 协议 / 地址 / Key（只写）/ 模型 → 保存并测试。引导与设置页共用。 */
import { computed, onMounted, ref } from "vue";
import { api, openExternal, toAppError } from "@/api";
import type { AiConfig, AiPreset, AiProtocol, AiStatus } from "@/api/types";
import { useAppStore } from "@/stores/app";
import GSegmented from "@/components/ui/GSegmented.vue";
import Icon from "@/components/ui/Icon.vue";

const props = withDefaults(defineProps<{ compact?: boolean }>(), { compact: false });
const emit = defineEmits<{ verified: [AiStatus] }>();
const app = useAppStore();
const presets = ref<AiPreset[]>([]);
const cfg = ref<AiConfig | null>(null);
const provider = ref("deepseek");
const protocol = ref<AiProtocol>("openai");
const baseUrl = ref("");
const model = ref("");
const fastModel = ref("");
const apiKey = ref("");
const temperature = ref(0.2);
const timeoutSecs = ref(60);
const testing = ref(false);
const status = ref<AiStatus | null>(null);
const showAdv = ref(false);

onMounted(async () => {
  try {
    const [p, c] = await Promise.all([api.aiPresets(), api.aiGetConfig()]);
    presets.value = p;
    cfg.value = c;
    provider.value = c.provider || "deepseek";
    protocol.value = c.protocol;
    baseUrl.value = c.baseUrl;
    model.value = c.model;
    fastModel.value = c.fastModel;
    temperature.value = c.temperature;
    timeoutSecs.value = c.timeoutSecs;
    if (!c.baseUrl) pick(p.find((x) => x.id === "deepseek") ?? p[0]);
    if (app.ai.verified) status.value = app.ai;
  } catch (e) {
    status.value = { configured: false, verified: false, model: "", provider: "", lastError: toAppError(e).message };
  }
});

const preset = computed(() => presets.value.find((p) => p.id === provider.value) ?? null);
function pick(p?: AiPreset) {
  if (!p) return;
  provider.value = p.id;
  protocol.value = p.protocol;
  if (p.id !== "custom") {
    baseUrl.value = p.baseUrl;
    model.value = p.models[0] ?? "";
    fastModel.value = p.fastModel && p.fastModel !== p.models[0] ? p.fastModel : "";
  }
  status.value = null;
}

async function saveAndTest() {
  testing.value = true;
  status.value = null;
  try {
    cfg.value = await api.aiSaveConfig({
      protocol: protocol.value,
      provider: provider.value,
      baseUrl: baseUrl.value.trim(),
      model: model.value.trim(),
      fastModel: fastModel.value.trim(),
      temperature: temperature.value,
      timeoutSecs: timeoutSecs.value,
      apiKey: apiKey.value ? apiKey.value.trim() : undefined,
    });
    apiKey.value = "";
    const s = await api.aiTest();
    status.value = s;
    app.setAi(s);
    if (s.verified) emit("verified", s);
  } catch (e) {
    status.value = { configured: false, verified: false, model: model.value, provider: provider.value, lastError: toAppError(e).message };
  } finally {
    testing.value = false;
  }
}
const protoOpts = [
  { value: "openai" as const, label: "OpenAI 兼容" },
  { value: "anthropic" as const, label: "Anthropic" },
];
const REGION: Record<string, string> = { cn: "国内", intl: "国际", local: "本地" };
</script>

<template>
  <div class="aif" :class="{ compact: props.compact }">
    <div class="presets">
      <button v-for="p in presets" :key="p.id" v-ripple class="preset" :class="{ on: provider === p.id }" @click="pick(p)">
        <span class="p-name">{{ p.name }}</span>
        <span class="p-note">{{ p.note || (p.protocol === "anthropic" ? "Messages 协议" : "OpenAI 兼容") }}</span>
        <span class="p-reg">{{ REGION[p.region] }}</span>
        <span class="p-ck"><Icon name="check" :size="12" :stroke="3" /></span>
      </button>
    </div>

    <div class="form-grid">
      <label class="field full">
        <span>API Key <a v-if="preset?.keyUrl" class="tiny" href="#" @click.prevent="openExternal(preset.keyUrl)">去获取 <Icon name="external" :size="11" /></a></span>
        <input
          v-model="apiKey"
          class="input mono"
          type="password"
          autocomplete="off"
          :placeholder="cfg?.hasKey ? `已保存 ${cfg.keyHint}（留空则保持不变）` : provider === 'ollama' ? '本地模型可留空' : '粘贴 API Key，仅保存在本机'"
        />
      </label>
      <label class="field">
        <span>主模型（规划 / 总结 / Copilot）</span>
        <input v-model="model" class="input" list="ai-models" placeholder="模型名称" />
        <datalist id="ai-models"><option v-for="m in preset?.models ?? []" :key="m" :value="m" /></datalist>
      </label>
      <label class="field">
        <span>快速模型（抽取 / 校验，可选）</span>
        <input v-model="fastModel" class="input" list="ai-models" placeholder="留空则使用主模型" />
      </label>
      <template v-if="showAdv || provider === 'custom'">
        <label class="field full">
          <span>Base URL</span>
          <input v-model="baseUrl" class="input mono" placeholder="https://api.example.com/v1" />
        </label>
        <div class="field">
          <span>协议</span>
          <GSegmented v-model="protocol" :options="protoOpts" small />
        </div>
        <label class="field">
          <span>温度 {{ temperature.toFixed(1) }} · 超时 {{ timeoutSecs }}s</span>
          <div class="row">
            <input v-model.number="temperature" type="range" min="0" max="1" step="0.1" />
            <input v-model.number="timeoutSecs" class="input sm" type="number" min="10" max="300" style="width: 80px" />
          </div>
        </label>
      </template>
    </div>

    <div class="row" style="margin-top: 14px">
      <button v-if="provider !== 'custom'" class="btn ghost sm" @click="showAdv = !showAdv"><Icon :name="showAdv ? 'up' : 'sliders'" />{{ showAdv ? "收起高级选项" : "高级选项" }}</button>
      <span class="spacer"></span>
      <button v-ripple class="btn primary" :disabled="testing || !model || !baseUrl" @click="saveAndTest">
        <Icon :name="testing ? 'refresh' : 'zap'" :class="{ spin: testing }" />{{ testing ? "正在连接模型…" : "保存并测试连接" }}
      </button>
    </div>

    <Transition name="blur" mode="out-in">
      <div v-if="status?.verified" key="ok" class="result ok">
        <svg class="check-anim" width="34" height="34" viewBox="0 0 34 34"><circle cx="17" cy="17" r="16" fill="#2fd27a" /><path d="M10 17.5l4.6 4.6L24.5 12" fill="none" stroke="#fff" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" /></svg>
        <div>
          <b>连接成功 · {{ status.model }}</b>
          <p class="small">延迟 {{ status.latencyMs ?? "—" }} ms<template v-if="status.sample"> · 模型回答：「{{ status.sample }}」</template></p>
        </div>
      </div>
      <div v-else-if="status && !status.verified" key="bad" class="result bad">
        <Icon name="warn" :size="22" />
        <div>
          <b>连接失败</b>
          <p class="small selectable">{{ status.lastError ?? "未知错误" }}</p>
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.presets {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
  margin-bottom: 16px;
}
.compact .presets {
  grid-template-columns: repeat(3, minmax(0, 1fr));
}
.preset {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  padding: 10px 12px;
  border-radius: 14px;
  border: 1px solid var(--line-2);
  background: var(--glass-1);
  cursor: pointer;
  text-align: left;
  transition: border-color var(--t-fast), box-shadow var(--t-mid), transform var(--t-mid) var(--spring);
}
.preset:hover {
  transform: translateY(-1px);
}
.preset.on {
  border-color: color-mix(in srgb, var(--brand-1) 60%, transparent);
  box-shadow: 0 0 0 3px rgba(61, 123, 255, 0.14);
}
.p-name {
  font-weight: 650;
  font-size: 13.5px;
}
.p-note {
  font-size: 11.5px;
  color: var(--text-3);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}
.p-reg {
  position: absolute;
  right: 10px;
  top: 10px;
  font-size: 10.5px;
  color: var(--text-3);
}
.p-ck {
  position: absolute;
  right: 8px;
  bottom: 8px;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  display: grid;
  place-items: center;
  background: var(--brand-grad);
  color: #fff;
  transform: scale(0);
  transition: transform 360ms var(--spring);
}
.preset.on .p-ck {
  transform: scale(1);
}
.field > span a {
  margin-left: 6px;
  display: inline-flex;
  align-items: center;
  gap: 2px;
}
.result {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 14px;
  padding: 12px 14px;
  border-radius: 14px;
}
.result.ok {
  background: var(--ok-bg);
  color: var(--ok);
}
.result.ok p {
  color: var(--text-2);
}
.result.bad {
  background: var(--err-bg);
  color: var(--err);
}
</style>
