import { Show, type JSX } from "solid-js";

/**
 * The heading every route opens with.
 *
 * Pages used to hand-write the same `<header><h1>` pair, and the sheets had
 * drifted to a smaller heading with different tracking, so `/help` and
 * `/settings/advanced` looked like a different product from `/settings`.
 * Holding the type scale, the subtitle treatment and the trailing slot in one
 * component means a page cannot silently stop matching its neighbours.
 *
 * `subtitle` takes markup as well as a string: the Android device page puts the
 * device name and port there in the mono face, and that is content, not style.
 */
export function PageTitle(props: {
  title: string;
  subtitle?: JSX.Element;
  /** Badges or buttons that sit to the right, aligned with the heading. */
  aside?: JSX.Element;
}) {
  return (
    <header class="flex items-end justify-between gap-4">
      <div class="min-w-0">
        <h1 class="text-display font-[620] leading-[1.04] tracking-[-0.035em]">{props.title}</h1>
        <Show when={props.subtitle}>
          <p class="text-body-sm text-muted-foreground">{props.subtitle}</p>
        </Show>
      </div>
      <Show when={props.aside}>
        <div class="flex shrink-0 items-center gap-2">{props.aside}</div>
      </Show>
    </header>
  );
}
