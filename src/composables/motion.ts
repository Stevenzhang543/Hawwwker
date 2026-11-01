import { onBeforeUnmount, onMounted, ref, watch, type Ref } from "vue";

/** 滚动联动模糊：把容器滚动距离映射为 0..1 写到 target 的 --sb 变量 */
export function useScrollBlur(scroller: Ref<HTMLElement | null>, target: Ref<HTMLElement | null>, distance = 140) {
  let raf = 0;
  const onScroll = () => {
    if (raf) return;
    raf = requestAnimationFrame(() => {
      raf = 0;
      const el = scroller.value;
      const t = target.value;
      if (!el || !t) return;
      const v = Math.min(1, el.scrollTop / distance);
      t.style.setProperty("--sb", v.toFixed(3));
    });
  };
  onMounted(() => scroller.value?.addEventListener("scroll", onScroll, { passive: true }));
  onBeforeUnmount(() => {
    scroller.value?.removeEventListener("scroll", onScroll);
    cancelAnimationFrame(raf);
  });
  return { onScroll };
}

/** 数字滚动 */
export function useCountUp(source: Readonly<Ref<number>>, ms = 900) {
  const value = ref(0);
  let raf = 0;
  const run = (to: number) => {
    cancelAnimationFrame(raf);
    const from = value.value;
    const reduced = document.documentElement.dataset.motion === "reduced";
    if (reduced || !Number.isFinite(to)) {
      value.value = to;
      return;
    }
    const t0 = performance.now();
    const step = (t: number) => {
      const p = Math.min(1, (t - t0) / ms);
      const e = 1 - Math.pow(1 - p, 3);
      value.value = from + (to - from) * e;
      if (p < 1) raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
  };
  watch(source, (v) => run(v), { immediate: true });
  onBeforeUnmount(() => cancelAnimationFrame(raf));
  return value;
}

/** 打字机：逐字揭示文本 */
export function useTypewriter(source: Readonly<Ref<string>>, cps = 90) {
  const shown = ref("");
  let timer = 0;
  watch(
    source,
    (text) => {
      window.clearInterval(timer);
      if (!text) {
        shown.value = "";
        return;
      }
      if (document.documentElement.dataset.motion === "reduced") {
        shown.value = text;
        return;
      }
      if (!text.startsWith(shown.value)) shown.value = "";
      const step = Math.max(1, Math.round(cps / 30));
      timer = window.setInterval(() => {
        if (shown.value.length >= text.length) {
          window.clearInterval(timer);
          return;
        }
        shown.value = text.slice(0, shown.value.length + step);
      }, 33);
    },
    { immediate: true },
  );
  onBeforeUnmount(() => window.clearInterval(timer));
  return shown;
}

/** 当前窗口宽度（响应式） */
export function useWindowWidth() {
  const w = ref(typeof window === "undefined" ? 1360 : window.innerWidth);
  const onResize = () => (w.value = window.innerWidth);
  onMounted(() => window.addEventListener("resize", onResize));
  onBeforeUnmount(() => window.removeEventListener("resize", onResize));
  return w;
}
