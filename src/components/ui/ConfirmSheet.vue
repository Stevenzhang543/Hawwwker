<script setup lang="ts">
import { useUiStore } from "@/stores/ui";
import Icon from "./Icon.vue";
const ui = useUiStore();
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="ui.confirmReq" class="cf-back dof-backdrop" @pointerdown.self="ui.settleConfirm(false)"></div>
    </Transition>
    <Transition name="pop">
      <div v-if="ui.confirmReq" class="cf glass-3" role="alertdialog">
        <div class="cf-icon" :class="{ danger: ui.confirmReq.danger }"><Icon :name="ui.confirmReq.danger ? 'warn' : 'info'" :size="22" /></div>
        <h3>{{ ui.confirmReq.title }}</h3>
        <p class="muted">{{ ui.confirmReq.body }}</p>
        <div class="row cf-actions">
          <button class="btn" @click="ui.settleConfirm(false)">取消</button>
          <button v-ripple class="btn" :class="ui.confirmReq.danger ? 'danger-solid' : 'primary'" @click="ui.settleConfirm(true)">{{ ui.confirmReq.okText }}</button>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.cf-back {
  position: fixed;
  inset: 0;
  z-index: 500;
  background: rgba(10, 14, 28, 0.28);
}
.cf {
  position: fixed;
  z-index: 501;
  left: 50%;
  top: 50%;
  translate: -50% -50%;
  width: 420px;
  padding: 24px;
  border-radius: 22px;
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
}
.cf-icon {
  width: 46px;
  height: 46px;
  border-radius: 15px;
  display: grid;
  place-items: center;
  background: var(--info-bg);
  color: var(--brand-ink);
}
.cf-icon.danger {
  background: var(--err-bg);
  color: var(--err);
}
.cf h3 {
  font-size: 17px;
}
.cf-actions {
  margin-top: 8px;
  width: 100%;
}
.cf-actions .btn {
  flex: 1;
}
.danger-solid {
  background: linear-gradient(135deg, #ff5a5f, #e5383d);
  color: #fff;
  border-color: transparent;
}
</style>
