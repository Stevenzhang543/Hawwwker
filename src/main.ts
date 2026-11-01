import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import { router } from "./router";
import { installDirectives } from "./composables/directives";
import { useAppStore } from "./stores/app";
import { usePlatformStore } from "./stores/platforms";
import { useSearchStore } from "./stores/search";
import { useNoticeStore } from "./stores/notices";
import { useWatchStore } from "./stores/watch";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/glass.css";
import "./styles/motion.css";

async function boot() {
  const app = createApp(App);
  const pinia = createPinia();
  app.use(pinia);
  installDirectives(app);

  const appStore = useAppStore();
  try {
    const b = await appStore.bootstrap();
    const platforms = usePlatformStore();
    platforms.set(b.platforms, b.logins);
    const notices = useNoticeStore();
    notices.unreadBoot = b.unread;
    const search = useSearchStore();
    await Promise.all([platforms.listen(), search.listen(), notices.listen(), useWatchStore().listen()]);
    if (b.runningRuns.length) void search.resume(b.runningRuns);
  } catch (e) {
    console.error("bootstrap failed", e);
  }

  app.use(router);
  await router.isReady();
  app.mount("#app");
  document.documentElement.classList.add("booted");
}

void boot();
