import { toRaw } from "vue";

/** 深拷贝（可处理 Vue 响应式代理；structuredClone 无法克隆 Proxy） */
export function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(toRaw(v))) as T;
}
