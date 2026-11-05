import { createEffect, createMemo, createSignal, onCleanup, type Accessor } from "solid-js";
import type { Middleware, MiddlewareData, Placement, ReferenceElement, Strategy } from "./types";
import { computePosition } from "./core/compute-position";
import { autoUpdate as attachAutoUpdate, type AutoUpdateOptions } from "./auto-update";

type MaybeAccessor<T> = T | Accessor<T>;

function unwrap<T>(value: MaybeAccessor<T> | undefined, fallback: T): T {
  if (value === undefined) return fallback;
  return typeof value === "function" ? (value as Accessor<T>)() : value;
}

export interface CreatePositionerOptions {
  placement?: MaybeAccessor<Placement>;
  strategy?: MaybeAccessor<Strategy>;
  middleware?: MaybeAccessor<Middleware[]>;
  autoUpdate?: boolean | AutoUpdateOptions;
}

export interface Positioner {
  x: Accessor<number>;
  y: Accessor<number>;
  placement: Accessor<Placement>;
  strategy: Accessor<Strategy>;
  middlewareData: Accessor<MiddlewareData>;
  isPositioned: Accessor<boolean>;
  update: () => void;
  floatingStyles: Accessor<{
    position: Strategy;
    top: string;
    left: string;
    transform: string;
  }>;
}

/**
 *
 *
 * @example
 * ```tsx
 * const [reference, setReference] = createSignal<HTMLElement>();
 * const [floating, setFloating] = createSignal<HTMLElement>();
 * const pos = createPositioner(reference, floating, {
 *   placement: "bottom-start",
 *   middleware: [offset(8), flip(), shift({ padding: 8 })],
 * });
 *
 * <button ref={setReference}>trigger</button>
 * <Show when={open()}>
 *   <Portal>
 *   </Portal>
 * </Show>
 * ```
 */
export function createPositioner(
  reference: Accessor<ReferenceElement | null | undefined>,
  floating: Accessor<HTMLElement | null | undefined>,
  options: CreatePositionerOptions = {},
): Positioner {
  const [x, setX] = createSignal(0);
  const [y, setY] = createSignal(0);
  const [placement, setPlacement] = createSignal<Placement>(unwrap(options.placement, "bottom"));
  const [strategy, setStrategy] = createSignal<Strategy>(unwrap(options.strategy, "absolute"));
  const [middlewareData, setMiddlewareData] = createSignal<MiddlewareData>({});
  const [isPositioned, setIsPositioned] = createSignal(false);

  const update = () => {
    const refEl = reference();
    const floatEl = floating();
    if (!refEl || !floatEl) {
      setIsPositioned(false);
      return;
    }

    const result = computePosition(refEl, floatEl, {
      placement: unwrap(options.placement, "bottom"),
      strategy: unwrap(options.strategy, "absolute"),
      middleware: unwrap(options.middleware, []),
    });

    setX(result.x);
    setY(result.y);
    setPlacement(result.placement);
    setStrategy(result.strategy);
    setMiddlewareData(result.middlewareData);
    setIsPositioned(true);
  };

  createEffect(() => {
    const refEl = reference();
    const floatEl = floating();
    unwrap(options.placement, "bottom");
    unwrap(options.strategy, "absolute");
    unwrap(options.middleware, []);

    if (!refEl || !floatEl) {
      setIsPositioned(false);
      return;
    }

    update();

    if (options.autoUpdate !== false) {
      const cleanup = attachAutoUpdate(
        refEl,
        floatEl,
        update,
        typeof options.autoUpdate === "object" ? options.autoUpdate : {},
      );
      onCleanup(cleanup);
    }
  });

  const floatingStyles = createMemo(() => ({
    position: strategy(),
    top: "0px",
    left: "0px",
    transform: `translate(${Math.round(x())}px, ${Math.round(y())}px)`,
  }));

  return {
    x,
    y,
    placement,
    strategy,
    middlewareData,
    isPositioned,
    update,
    floatingStyles,
  };
}
