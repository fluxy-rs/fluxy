import { Show, createEffect, mergeProps, splitProps, type ValidComponent } from "solid-js";
import { Dynamic } from "solid-js/web";
import { ArrowDown } from "lucide-solid";
import { clsx } from "../../utils";
import { useMessageScrollerContext } from "../message-scroller.context";
import type { MessageScrollerButtonProps } from "../message-scroller.types";

/**
 */
export function MessageScrollerButton(props: MessageScrollerButtonProps) {
  const ctx = useMessageScrollerContext("MessageScrollerButton");
  const merged = mergeProps(
    {
      direction: "end" as const,
      behavior: "smooth" as ScrollBehavior,
      variant: "secondary",
      size: "icon-sm",
      component: "button" as const,
      type: "button" as const,
    },
    props,
  );
  const [local, rest] = splitProps(merged, [
    "class",
    "classList",
    "direction",
    "behavior",
    "variant",
    "size",
    "children",
    "component",
    "onClick",
    "type",
    "tabIndex",
  ]);

  const active = () => (local.direction === "end" ? ctx.scrollableEnd() : ctx.scrollableStart());

  const handleClick = (event: MouseEvent & { currentTarget: Element }) => {
    if (!active()) return;
    const handler = local.onClick as
      | ((event: MouseEvent) => void)
      | [(data: unknown, event: MouseEvent) => void, unknown]
      | undefined;
    if (Array.isArray(handler)) handler[0](handler[1], event);
    else handler?.(event);
    if (event.defaultPrevented) return;
    (event.currentTarget as HTMLElement).blur();
    if (local.direction === "start") {
      ctx.scrollToStart({ behavior: local.behavior });
    } else {
      ctx.scrollToEnd({ behavior: local.behavior });
    }
  };

  return (
    <Dynamic
      {...rest}
      component={local.component as ValidComponent}
      ref={(el: HTMLElement) => {
        createEffect(() => {
          el.inert = !active();
        });
      }}
      type={local.type}
      data-slot="message-scroller-button"
      data-direction={local.direction}
      data-variant={local.variant}
      data-size={local.size}
      data-active={active() ? "true" : "false"}
      tabIndex={active() ? (local.tabIndex ?? 0) : -1}
      aria-label={local.direction === "end" ? "Scroll to end" : "Scroll to start"}
      onClick={handleClick}
      class={clsx(
        "absolute left-1/2 z-10 inline-flex size-8 -translate-x-1/2 items-center justify-center rounded-full border border-border bg-background text-foreground shadow-md transition-[translate,scale,opacity] duration-200 hover:bg-muted hover:text-foreground",
        "data-[active=false]:pointer-events-none data-[active=false]:scale-95 data-[active=false]:opacity-0 data-[active=false]:duration-400 data-[active=false]:ease-[cubic-bezier(0.7,0,0.84,0)]",
        "data-[active=true]:translate-y-0 data-[active=true]:scale-100 data-[active=true]:opacity-100 data-[active=true]:ease-[cubic-bezier(0.23,1,0.32,1)]",
        "data-[direction=end]:bottom-4 data-[direction=end]:data-[active=false]:translate-y-full",
        "data-[direction=start]:top-4 data-[direction=start]:data-[active=false]:-translate-y-full",
        "rtl:translate-x-1/2 data-[direction=start]:[&_svg]:rotate-180",
        local.class,
      )}
      classList={local.classList}
    >
      <Show
        when={local.children}
        fallback={
          <>
            <ArrowDown class="size-4" />
            <span class="sr-only">
              {local.direction === "end" ? "Scroll to end" : "Scroll to start"}
            </span>
          </>
        }
      >
        {(children) => children()}
      </Show>
    </Dynamic>
  );
}
