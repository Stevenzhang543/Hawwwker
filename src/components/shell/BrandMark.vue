<script setup lang="ts">
/* Hawwwker 品牌标志：价签外形 = 鹰头，穿孔 = 琥珀色鹰眼 */
withDefaults(defineProps<{ size?: number; plain?: boolean }>(), { size: 28, plain: false });
const gid = "hk" + Math.random().toString(36).slice(2, 7);
</script>

<template>
  <svg :width="size" :height="size" viewBox="0 0 64 64" class="brand-mark" aria-hidden="true">
    <defs>
      <linearGradient :id="gid" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="#3d7bff" />
        <stop offset="1" stop-color="#8a5cff" />
      </linearGradient>
    </defs>
    <rect v-if="!plain" width="64" height="64" rx="16" :fill="`url(#${gid})`" />
    <path
      d="M14 25.5c0-4.4 3.6-8 8-8h17.6c2.2 0 4.2.9 5.7 2.4l8.4 8.6c1.6 1.6 1.6 4.3-.1 5.8-2.6 2.5-5.6 4.3-9 4.9 1.6-2 1.8-3.8.9-5.2L40.2 39c-1.6 2.3-4.2 3.6-7 3.6H22c-4.4 0-8-3.6-8-8z"
      :fill="plain ? `url(#${gid})` : '#fff'"
    />
    <path d="M14 33.5c-2.6 2.8-4.5 6.6-5 11 3.2-2.4 6.8-3.6 10.6-3.8" :fill="plain ? `url(#${gid})` : '#fff'" opacity=".55" />
    <circle cx="27" cy="29.6" r="4.6" fill="#ffb020" />
    <circle cx="28" cy="29.2" r="2" fill="#121624" />
  </svg>
</template>

<style scoped>
.brand-mark {
  flex: none;
  display: block;
}
</style>
