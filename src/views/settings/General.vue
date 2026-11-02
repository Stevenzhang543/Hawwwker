<script setup lang="ts">
import { computed } from "vue";
import type { AppSettingsPatch, MotionMode, Role, SearchPrefs, ThemeMode, WindowEffect } from "@/api/types";
import { ROLE_LABEL, useAppStore } from "@/stores/app";
import { useUiStore } from "@/stores/ui";
import { CURRENCIES } from "@/utils/format";
import { isTauri, toAppError } from "@/api";
import GSegmented from "@/components/ui/GSegmented.vue";
import GSwitch from "@/components/ui/GSwitch.vue";
import Icon from "@/components/ui/Icon.vue";

const app = useAppStore();
const ui = useUiStore();
const s = computed(() => app.settings!);

async function patch(p: AppSettingsPatch, msg = "已保存") {
  try {
    await app.update(p);
    ui.toast(msg, "success", 1200);
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
function sp<K extends keyof SearchPrefs>(k: K, v: SearchPrefs[K]) {
  void patch({ search: { [k]: v } as Partial<SearchPrefs> });
}
const roleOpts = (Object.keys(ROLE_LABEL) as Role[]).map((r) => ({ value: r, label: ROLE_LABEL[r] }));
const themeOpts: { value: ThemeMode; label: string }[] = [
  { value: "system", label: "跟随系统" },
  { value: "light", label: "浅色" },
  { value: "dark", label: "深色" },
];
const motionOpts: { value: MotionMode; label: string }[] = [
  { value: "full", label: "完整动效" },
  { value: "reduced", label: "减少动效" },
];
const effectOpts: { value: WindowEffect; label: string }[] = [
  { value: "mica", label: "Mica 云母" },
  { value: "acrylic", label: "Acrylic 亚克力" },
  { value: "none", label: "纯色" },
];
const regionOpts = [
  { value: "all" as const, label: "全部" },
  { value: "cn" as const, label: "国内版" },
  { value: "intl" as const, label: "国际版" },
];
</script>

<template>
  <div v-if="app.settings" class="page">
    <div class="page-head">
      <div>
        <h1>通用</h1>
        <p>角色、显示货币、外观与搜索行为。修改即时生效。</p>
      </div>
    </div>

    <section class="glass card">
      <div class="card-head"><h3>工作台</h3></div>
      <div class="set">
        <div class="set-l"><b>角色</b><small>决定默认首页、示例指令与 Copilot 技能排序，不限制任何功能</small></div>
        <GSegmented :model-value="s.role" :options="roleOpts" @update:model-value="(v) => patch({ role: v }, `已切换为${ROLE_LABEL[v]}`)" />
      </div>
      <div class="set">
        <div class="set-l"><b>显示货币</b><small>所有价格会额外显示换算到该货币的金额（ECB 汇率）</small></div>
        <select class="select" style="width: 200px" :value="s.displayCurrency" @change="patch({ displayCurrency: ($event.target as HTMLSelectElement).value })">
          <option v-for="c in CURRENCIES" :key="c.code" :value="c.code">{{ c.code }} · {{ c.name }}</option>
        </select>
      </div>
    </section>

    <section class="glass card">
      <div class="card-head"><h3>外观</h3></div>
      <div class="set">
        <div class="set-l"><b>主题</b></div>
        <GSegmented :model-value="s.theme" :options="themeOpts" @update:model-value="(v) => patch({ theme: v })" />
      </div>
      <div class="set">
        <div class="set-l"><b>动效</b><small>减少动效会关闭模糊入场、景深与弹簧动画</small></div>
        <GSegmented :model-value="s.motion" :options="motionOpts" @update:model-value="(v) => patch({ motion: v })" />
      </div>
      <div class="set">
        <div class="set-l"><b>窗口材质</b><small>Windows 11 的系统级动态模糊；Windows 10 建议 Acrylic {{ isTauri ? "" : "（浏览器预览中不生效）" }}</small></div>
        <GSegmented :model-value="s.windowEffect" :options="effectOpts" @update:model-value="(v) => patch({ windowEffect: v })" />
      </div>
    </section>

    <section class="glass card">
      <div class="card-head"><h3>搜索行为</h3><span class="faint tiny">在你自己的浏览器会话中执行，节奏接近真人</span></div>
      <div class="set">
        <div class="set-l"><b>默认地区</b><small>智能搜索页打开时的地区筛选</small></div>
        <GSegmented :model-value="s.search.defaultRegion" :options="regionOpts" small @update:model-value="(v) => sp('defaultRegion', v)" />
      </div>
      <div class="set">
        <div class="set-l"><b>默认结果条数</b><small>最终展示的结果上限</small></div>
        <input class="input sm" type="number" min="5" max="200" style="width: 100px" :value="s.search.resultLimit" @change="sp('resultLimit', Number(($event.target as HTMLInputElement).value))" />
      </div>
      <div class="set">
        <div class="set-l"><b>每个平台抓取条数</b></div>
        <input class="input sm" type="number" min="5" max="100" style="width: 100px" :value="s.search.perPlatform" @change="sp('perPlatform', Number(($event.target as HTMLInputElement).value))" />
      </div>
      <div class="set">
        <div class="set-l"><b>每个平台搜索词数量</b><small>多个搜索词可提高召回，但耗时更长</small></div>
        <input class="input sm" type="number" min="1" max="4" style="width: 100px" :value="s.search.maxQueries" @change="sp('maxQueries', Number(($event.target as HTMLInputElement).value))" />
      </div>
      <div class="set">
        <div class="set-l"><b>并行平台数</b><small>同时打开的标签页（1–4）</small></div>
        <input class="input sm" type="number" min="1" max="4" style="width: 100px" :value="s.search.concurrency" @change="sp('concurrency', Number(($event.target as HTMLInputElement).value))" />
      </div>
      <div class="set">
        <div class="set-l"><b>单平台超时（秒）</b><small>超时的平台会被标记失败，不影响其他平台</small></div>
        <input class="input sm" type="number" min="30" max="600" style="width: 100px" :value="s.search.platformTimeoutSecs" @change="sp('platformTimeoutSecs', Number(($event.target as HTMLInputElement).value))" />
      </div>
      <div class="set">
        <div class="set-l"><b>后台运行浏览器</b><small>窗口放在屏幕外；需要你操作时自动移回</small></div>
        <GSwitch :model-value="s.search.background" @update:model-value="(v) => sp('background', v)" />
      </div>
      <div class="set">
        <div class="set-l"><b>会话过期时弹窗等待（秒）</b><small>0 = 不弹窗，直接标记"需要登录"</small></div>
        <input class="input sm" type="number" min="0" max="600" style="width: 100px" :value="s.search.loginWaitSecs" @change="sp('loginWaitSecs', Number(($event.target as HTMLInputElement).value))" />
      </div>
      <div class="set">
        <div class="set-l"><b>未登录平台也弹窗</b><small>关闭时未登录的平台会被跳过，不打断你</small></div>
        <GSwitch :model-value="s.search.popupOnLogin" @update:model-value="(v) => sp('popupOnLogin', v)" />
      </div>
      <div class="set">
        <div class="set-l"><b>登录保活</b><small>每 6 小时低频访问已登录平台首页，保持会话有效</small></div>
        <GSwitch :model-value="s.search.keepalive" @update:model-value="(v) => sp('keepalive', v)" />
      </div>
    </section>

    <section class="glass card">
      <div class="card-head"><h3>快捷键</h3></div>
      <div class="keys">
        <span><kbd>Ctrl K</kbd> 命令面板</span><span><kbd>Ctrl J</kbd> Copilot</span><span><kbd>Enter</kbd> 运行指令</span>
        <span><kbd>Alt 1–5</kbd> 切换一级入口</span><span><kbd>Esc</kbd> 关闭弹层</span>
      </div>
      <p class="faint tiny" style="margin-top: 8px"><Icon name="info" :size="12" /> 在指令框中 Shift+Enter 换行。</p>
    </section>
  </div>
</template>

<style scoped>
.set {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 12px 0;
  border-top: 1px solid var(--line);
}
.card-head + .set {
  border-top: none;
}
.set-l {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.set-l b {
  font-weight: 600;
}
.set-l small {
  font-size: 12px;
  color: var(--text-3);
}
.keys {
  display: flex;
  flex-wrap: wrap;
  gap: 10px 20px;
  font-size: 13px;
  color: var(--text-2);
}
.keys kbd {
  margin-right: 4px;
}
</style>
