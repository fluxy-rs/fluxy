import { splitProps, type ComponentProps } from "solid-js";

export function Textarea(props: ComponentProps<"textarea">) {
  const [local, rest] = splitProps(props, ["class"]);
  return <textarea {...rest} class={`fx-input fx-textarea ${local.class ?? ""}`} />;
}
