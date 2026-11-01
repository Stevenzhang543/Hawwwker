<script setup lang="ts">
import { ref, watch } from "vue";
import type { NotifyPrefs } from "@/api/types";
import { useAppStore } from "@/stores/app";
import { useUiStore } from "@/stores/ui";
import { isTauri, toAppError } from "@/api";
import GSwitch from "@/components/ui/GSwitch.vue";
import Icon from "@/components/ui/Icon.vue";

const app = useAppStore();
const ui = useUiStore();
const url = ref(app.settings?.notify.webhookUrl ?? "");
const kind = ref<NotifyPrefs["webhookKind"]>(app.settings?.notify.webhookKind ?? "none");
const drop = ref(app.settings?.notify.dropPercent ?? 5);
watch(
  () => app.settings?.notify,
  (n) => {
    if (!n) return;
    url.value = n.webhookUrl;
    kind.value = n.webhookKind;
    drop.value = n.dropPercent;
  },
);

async function save(p: Partial<NotifyPrefs>, msg = "已保存") {
  try {
    await app.update({ notify: p });
    ui.toast(msg, "success", 1300);
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
async function testSystem() {
  if (!isTauri) {
    ui.toast("这是一条测试提醒", "success", 3000, "桌面版会同时弹出系统通知");
    return;
  }
  try {
    const n = await import("@tauri-apps/plugin-notification");
    let ok = await n.isPermissionGranted();
    if (!ok) ok = (await n.requestPermission()) === "granted";
    if (ok) n.sendNotification({ title: "Hawwwker 测试提醒", body: "降价与目标价提醒会以这种方式通知你。" });
    else ui.toast("系统通知权限被拒绝", "warn");
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
const KINDS: { value: NotifyPrefs["webhookKind"]; label: string; ph: string }[] = [
  { value: "none", label: "不推送", ph: "" },
  { value: "wecom", label: "企业微信", ph: "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=…" },
  { value: "feishu", label: "飞书", ph: "https://open.feishu.cn/open-apis/bot/v2/hook/…" },
  { value: "dingtalk", label: "钉钉", ph: "https://oapi.dingtalk.com/robot/send?access_token=…" },
  { value: "slack", label: "Slack", ph: "https://hooks.slack.com/services/…" },
  { value: "discord", label: "Discord", ph: "https://discord.com/api/webhooks/…" },
  { value: "generic", label: "通用 JSON", ph: "https://your-server.example.com/hook" },
];
</script>

<template>
  <div v-if="app.settings" class="page">
    <div class="page-head">
      <div>
        <h1>通知</h1>
        <p>降价、达到目标价、监控新品、登录过期与错误会出现在提醒中心，并按下面的设置推送。</p>
      </div>
    </div>

    <section class="glass card">
      <div class="card-head"><h3>系统通知</h3></div>
      <div class="set">
        <div class="set-l"><b>弹出系统通知</b><small>Windows 通知中心，应用最小化时也能收到</small></div>
        <button class="btn sm" @click="testSystem"><Icon name="bell" />发送测试</button>
        <GSwitch :model-value="app.settings.notify.system" @update:model-value="(v) => save({ system: v })" />
      </div>
      <div class="set">
        <div class="set-l"><b>降价提醒阈值</b><small>相对上一次价格下降超过该比例才提醒（达到目标价总会提醒）</small></div>
        <input v-model.number="drop" type="range" min="1" max="30" step="1" style="width: 200px" @change="save({ dropPercent: drop })" />
        <b class="num" style="width: 44px; text-align: right">{{ drop }}%</b>
      </div>
    </section>

    <section class="glass card">
      <div class="card-head"><h3>群机器人</h3><span class="faint tiny">把提醒推送到团队群</span></div>
      <div class="kinds">
        <button v-for="k in KINDS" :key="k.value" class="kind" :class="{ on: kind === k.value }" @click="kind = k.value">{{ k.label }}</button>
      </div>
      <label v-if="kind !== 'none'" class="field" style="margin-top: 12px">
        <span>Webhook 地址</span>
        <input v-model="url" class="input mono" :placeholder="KINDS.find((k) => k.value === kind)?.ph" />
        <span class="hint">飞书 / 钉钉如开启安全校验，请使用"自定义关键词"并填写 Hawwwker。</span>
      </label>
      <div class="row" style="margin-top: 12px; justify-content: flex-end">
        <button v-ripple class="btn primary" @click="save({ webhookKind: kind, webhookUrl: kind === 'none' ? '' : url.trim() })"><Icon name="check" />保存</button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.set {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 12px 0;
  border-top: 1px solid var(--line);
}
.card-head + .set {
  border-top: none;
}
.set-l {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.set-l small {
  font-size: 12px;
  color: var(--text-3);
}
.kinds {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.kind {
  height: 32px;
  padding: 0 14px;
  border-radius: 10px;
  border: 1px solid var(--line-2);
  background: var(--glass-1);
  cursor: pointer;
  font-size: 13px;
  transition: all 260ms var(--spring);
}
.kind.on {
  background: var(--brand-grad);
  color: #fff;
  border-color: transparent;
}
</style>
