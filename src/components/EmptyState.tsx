import { Show, type JSX } from "solid-js";

export function EmptyState(props: {
  icon?: JSX.Element;
  title: string;
  body: string;
  note?: string;
  action?: JSX.Element;
}) {
  return (
    <section class="fx-empty">
      <Show when={props.icon}>
        <div class="fx-empty-icon" aria-hidden="true">
          {props.icon}
        </div>
      </Show>
      <h3>{props.title}</h3>
      <p>{props.body}</p>
      <Show when={props.note}>
        <p class="fx-empty-note">{props.note}</p>
      </Show>
      <Show when={props.action}>
        <div class="fx-empty-action">{props.action}</div>
      </Show>
    </section>
  );
}
