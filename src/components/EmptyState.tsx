import type { JSX } from "solid-js";
import { Empty, EmptyDescription, EmptyMedia, EmptyTitle } from "../ui/empty";

/**
 * The shared "nothing here yet" panel.
 *
 * A thin wrapper over the component library's Empty so every caller gets the
 * same spacing and colours without repeating the composition. `body` is the
 * sentence under the title; `note` is the smaller print below it.
 */
export function EmptyState(props: {
  icon: JSX.Element;
  title: string;
  body?: string;
  note?: string;
  action?: JSX.Element;
}) {
  return (
    <Empty class="py-10">
      <EmptyMedia variant="icon">{props.icon}</EmptyMedia>
      <EmptyTitle>{props.title}</EmptyTitle>
      {props.body ? <EmptyDescription>{props.body}</EmptyDescription> : null}
      {props.note ? <p class="text-xs text-muted-foreground">{props.note}</p> : null}
      {props.action}
    </Empty>
  );
}
