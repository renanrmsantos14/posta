import { useSyncExternalStore } from "react";

/** O mesmo corte do CSS: abaixo dele o app vira uma coluna (celular). */
const mq = window.matchMedia("(max-width: 720px)");
const subscribe = (cb: () => void) => {
  mq.addEventListener("change", cb);
  return () => mq.removeEventListener("change", cb);
};

export const useNarrow = () => useSyncExternalStore(subscribe, () => mq.matches);
