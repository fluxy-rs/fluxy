import { type Accessor, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import type { ToastT } from "./Toast.types";

export const TOAST_LIFETIME = 4000;
export const EXIT_ANIMATION_MS = 200;

export interface UseToastLifecycleOptions {
  toast: Accessor<ToastT>;
  duration: Accessor<number | undefined>;
  onRemove: (id: string) => void;
}

export interface ToastLifecycle {
  animationState: Accessor<"open" | "closed">;
  close: () => void;
}

/**
 *
 */
export function useToastLifecycle(options: UseToastLifecycleOptions): ToastLifecycle {
  const [animationState, setAnimationState] = createSignal<"open" | "closed">("closed");

  onMount(() => {
    const raf = requestAnimationFrame(() => setAnimationState("open"));
    onCleanup(() => cancelAnimationFrame(raf));
  });

  const close = () => {
    if (animationState() !== "open") return;

    setAnimationState("closed");
    options.toast().onDismiss?.(options.toast());
    setTimeout(() => options.onRemove(options.toast().id), EXIT_ANIMATION_MS);
  };

  createEffect(() => {
    if (options.toast().delete && animationState() === "open") {
      close();
    }
  });

  createEffect(() => {
    const current = options.toast();
    if (
      current.delete ||
      current.type === "loading" ||
      current.duration === Infinity ||
      current.duration === 0
    ) {
      return;
    }

    const delay = current.duration ?? options.duration() ?? TOAST_LIFETIME;
    const timer = setTimeout(() => {
      current.onAutoClose?.(current);
      close();
    }, delay);

    onCleanup(() => clearTimeout(timer));
  });

  return { animationState, close };
}
