import { Input as NeutInput } from "../input";
import { splitProps, type ComponentProps } from "solid-js";

type Props = ComponentProps<typeof NeutInput>;

export function Input(props: Props) {
  const [local, rest] = splitProps(props, ["class"]);
  return <NeutInput {...rest} class={`fx-input ${local.class ?? ""}`} />;
}
