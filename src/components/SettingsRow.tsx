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
    <div class="fx-setting-row" classList={{ "is-stacked": props.stacked }}>
      <div class="fx-setting-copy">
        <span class="fx-setting-label">{props.label}</span>
        <Show when={props.description}>
          <span class="fx-setting-desc">{props.description}</span>
        </Show>
      </div>
      <Show when={props.children}>
        <div class="fx-setting-control">{props.children}</div>
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
    <a class="fx-setting-row fx-setting-link" href={props.href}>
      <div class="fx-setting-copy">
        <span class="fx-setting-label">{props.label}</span>
        <Show when={props.description}>
          <span class="fx-setting-desc">{props.description}</span>
        </Show>
      </div>
      <span class="fx-setting-control fx-setting-chevron" aria-hidden="true">
        <ChevronRight size={16} />
      </span>
    </a>
  );
}
