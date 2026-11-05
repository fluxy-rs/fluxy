import { Portal } from "solid-js/web";
import type { DrawerPortalProps } from "./DrawerPortal.types";

export const DrawerPortal = (props: DrawerPortalProps) => (
  <Portal>
    <div data-slot="drawer-portal" {...props} />
  </Portal>
);
