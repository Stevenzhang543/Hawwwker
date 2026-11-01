import type { App, Directive } from "vue";

function reduced(): boolean {
  return document.documentElement.dataset.motion === "reduced" || matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** v-ripple：按下时从指针位置扩散的涟漪 */
export const vRipple: Directive<HTMLElement> = {
  mounted(el) {
    el.classList.add("ripple-host");
    const onDown = (e: PointerEvent) => {
      if (e.button !== 0 || reduced() || (el as HTMLButtonElement).disabled) return;
      const r = el.getBoundingClientRect();
      const size = Math.max(r.width, r.height) * 2.2;
      const s = document.createElement("span");
      s.className = "ripple";
      s.style.width = s.style.height = `${size}px`;
      s.style.left = `${e.clientX - r.left - size / 2}px`;
      s.style.top = `${e.clientY - r.top - size / 2}px`;
      el.appendChild(s);
      window.setTimeout(() => s.remove(), 700);
    };
    el.addEventListener("pointerdown", onDown);
    (el as HTMLElement & { _ripple?: (e: PointerEvent) => void })._ripple = onDown;
  },
  unmounted(el) {
    const h = (el as HTMLElement & { _ripple?: (e: PointerEvent) => void })._ripple;
    if (h) el.removeEventListener("pointerdown", h);
  },
};

/** v-spec：写入 --mx/--my 供镜面高光使用 */
export const vSpec: Directive<HTMLElement> = {
  mounted(el) {
    el.classList.add("spec");
    const onMove = (e: PointerEvent) => {
      const r = el.getBoundingClientRect();
      el.style.setProperty("--mx", `${e.clientX - r.left}px`);
      el.style.setProperty("--my", `${e.clientY - r.top}px`);
    };
    el.addEventListener("pointermove", onMove);
    (el as HTMLElement & { _spec?: (e: PointerEvent) => void })._spec = onMove;
  },
  unmounted(el) {
    const h = (el as HTMLElement & { _spec?: (e: PointerEvent) => void })._spec;
    if (h) el.removeEventListener("pointermove", h);
  },
};

/** v-tilt：轻微 3D 倾斜 + 镜面高光（值为最大角度，默认 4°） */
export const vTilt: Directive<HTMLElement, number | undefined> = {
  mounted(el, binding) {
    el.classList.add("spec");
    const max = binding.value ?? 4;
    el.style.transition = "transform 420ms var(--spring-soft), box-shadow 280ms";
    const onMove = (e: PointerEvent) => {
      const r = el.getBoundingClientRect();
      const x = (e.clientX - r.left) / r.width;
      const y = (e.clientY - r.top) / r.height;
      el.style.setProperty("--mx", `${e.clientX - r.left}px`);
      el.style.setProperty("--my", `${e.clientY - r.top}px`);
      if (reduced()) return;
      el.style.transform = `perspective(900px) rotateX(${(0.5 - y) * max}deg) rotateY(${(x - 0.5) * max}deg) translateY(-2px)`;
    };
    const onLeave = () => {
      el.style.transform = "";
    };
    el.addEventListener("pointermove", onMove);
    el.addEventListener("pointerleave", onLeave);
    (el as HTMLElement & { _tilt?: [(e: PointerEvent) => void, () => void] })._tilt = [onMove, onLeave];
  },
  unmounted(el) {
    const h = (el as HTMLElement & { _tilt?: [(e: PointerEvent) => void, () => void] })._tilt;
    if (h) {
      el.removeEventListener("pointermove", h[0]);
      el.removeEventListener("pointerleave", h[1]);
    }
  },
};

/** v-magnet：按钮被指针轻微吸引 */
export const vMagnet: Directive<HTMLElement, number | undefined> = {
  mounted(el, binding) {
    const k = binding.value ?? 0.18;
    const onMove = (e: PointerEvent) => {
      if (reduced()) return;
      const r = el.getBoundingClientRect();
      const dx = e.clientX - (r.left + r.width / 2);
      const dy = e.clientY - (r.top + r.height / 2);
      el.style.translate = `${dx * k}px ${dy * k}px`;
    };
    const onLeave = () => {
      el.style.translate = "";
    };
    el.style.transition = (el.style.transition ? el.style.transition + ", " : "") + "translate 380ms var(--spring)";
    el.addEventListener("pointermove", onMove);
    el.addEventListener("pointerleave", onLeave);
    (el as HTMLElement & { _mag?: [(e: PointerEvent) => void, () => void] })._mag = [onMove, onLeave];
  },
  unmounted(el) {
    const h = (el as HTMLElement & { _mag?: [(e: PointerEvent) => void, () => void] })._mag;
    if (h) {
      el.removeEventListener("pointermove", h[0]);
      el.removeEventListener("pointerleave", h[1]);
    }
  },
};

export function installDirectives(app: App): void {
  app.directive("ripple", vRipple);
  app.directive("spec", vSpec);
  app.directive("tilt", vTilt);
  app.directive("magnet", vMagnet);
}
