import { Button as NeutButton, type ButtonProps } from "@neut/ui";
import { splitProps } from "solid-js";

type Variant = NonNullable<ButtonProps<"button">["variant"]> | "danger";

export type AppButtonProps = Omit<ButtonProps<"button">, "variant"> & {
  variant?: Variant;
  block?: boolean;
};

function mapVariant(variant: Variant | undefined) {
  return variant === "danger" ? "destructive" : (variant ?? "primary");
}

export function Button(props: AppButtonProps) {
  const [local, rest] = splitProps(props, ["class", "variant", "block", "type"]);
  return (
    <NeutButton
      {...rest}
      type={local.type ?? "button"}
      variant={mapVariant(local.variant)}
      class={`fx-button ${local.block ? "fx-button-block" : ""} ${local.class ?? ""}`}
    />
  );
}
