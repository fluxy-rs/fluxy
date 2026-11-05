import type { ValidComponent } from "solid-js";
import type { ButtonProps } from "../button";
import type { DialogProps, DialogSurfaceProps } from "../dialog";
import type { BaseProps, PolymorphicProps } from "../types";

export type SheetSide = "top" | "right" | "bottom" | "left";

export type SheetProps = DialogProps;

export type SheetTriggerProps<T extends ValidComponent> = PolymorphicProps<T>;

export type SheetCloseProps = ButtonProps<"button">;

export type SheetContentProps = DialogSurfaceProps & {
  side?: SheetSide;
  showCloseButton?: boolean;
};

export type SheetHeaderProps = PolymorphicProps<"div", BaseProps, false>;
export type SheetFooterProps = PolymorphicProps<"div", BaseProps, false>;
export type SheetTitleProps = PolymorphicProps<"h2", BaseProps, false>;
export type SheetDescriptionProps = PolymorphicProps<"p", BaseProps, false>;
