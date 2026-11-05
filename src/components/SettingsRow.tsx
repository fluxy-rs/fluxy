import { Show, type JSX } from "solid-js";
import { ChevronRight } from "lucide-solid";

/**
 * One settings line: label and explanation on the left, the control on the right.
 *
 * The page used to stack a heading, a paragraph and a full-width control inside
 * its own card, which spent roughly three lines of vertical space per setting.
 * A row keeps the same information at a third of the height, and two rows fit
 * where one card did.
 */
export function SettingsRow(props: {
  label: string;
  description?: string;
  /** Rendered flush right. Omit for a read-only row. */
  children?: JSX.Element;
  /** Stack the control under the text instead of beside it. */
  stacked?: boolean;
}) {
  return (
    <div
      class="flex items-center justify-between gap-4 px-4 py-3 not-last:border-b not-last:border-border"
      classList={{ "flex-col items-stretch": props.stacked }}
    >
      <div class="flex min-w-0 flex-col gap-0.5">
        <span class="text-sm font-medium">{props.label}</span>
        <Show when={props.description}>
          <span class="text-xs text-muted-foreground">{props.description}</span>
        </Show>
      </div>
      <Show when={props.children}>
        <div class="flex shrink-0 items-center gap-2">{props.children}</div>
      </Show>
    </div>
  );
}

/**
 * A row that goes somewhere. The whole line is the target - a phone-sized hit
 * area instead of a few words on the right - and the chevron says so.
 */
export function SettingsLinkRow(props: { label: string; description?: string; href: string }) {
  return (
    <a
      class="flex items-center justify-between gap-4 px-4 py-3 not-last:border-b not-last:border-border hover:bg-muted/50"
      href={props.href}
    >
      <div class="flex min-w-0 flex-col gap-0.5">
        <span class="text-sm font-medium">{props.label}</span>
        <Show when={props.description}>
          <span class="text-xs text-muted-foreground">{props.description}</span>
        </Show>
      </div>
      <span class="flex shrink-0 items-center text-muted-foreground" aria-hidden="true">
        <ChevronRight size={16} />
      </span>
    </a>
  );
}
