import type { BaseProps, PolymorphicProps } from "../../types";

interface BaseCheckboxProps {
  defaultChecked?: boolean;
  checked?: boolean;
  onChange?: (checked: boolean) => void;
  name?: string;
  disabled?: boolean;
}

export type CheckboxProps = Omit<
  PolymorphicProps<"span", BaseProps & BaseCheckboxProps, false>,
  "children" | "onClick"
>;
