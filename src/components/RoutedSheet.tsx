import { Show, createSignal, onMount, type JSX } from "solid-js";
import { Sheet, SheetContent } from "../ui/sheet";

/**
 * A settings sub-page that is a side sheet on a desktop and a page on a phone.
 *
 * The route does not change: `/help` and `/settings/advanced` stay addressable,
 * so a refresh or a deep link still lands on the right thing. Only the shape
 * changes with the breakpoint, because a phone has no room for a second panel
 * and the back button is the expected way out there.
 */
export function RoutedSheet(props: { title: string; description?: string; children: JSX.Element }) {
  const [wide, setWide] = createSignal(false);

  onMount(() => {
    const query = window.matchMedia("(min-width: 768px)");
    setWide(query.matches);
    const onChange = (event: MediaQueryListEvent) => setWide(event.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  });

  // Plain history rather than the router's navigate: this component is also
  // rendered without a Router in the page tests, where the hook throws.
  const close = () => {
    window.location.hash = "#/settings";
  };

  return (
    <Show
      when={wide()}
      fallback={
        <div class="mx-auto flex w-full max-w-[55rem] flex-col gap-4">{props.children}</div>
      }
    >
      <Sheet
        open
        onOpenChange={(open) => {
          if (!open) close();
        }}
      >
        <SheetContent
          side="right"
          class="w-[min(40rem,92vw)] flex-col gap-4 overflow-y-auto p-6 sm:max-w-none"
          aria-label={props.title}
        >
          {props.children}
        </SheetContent>
      </Sheet>
    </Show>
  );
}
