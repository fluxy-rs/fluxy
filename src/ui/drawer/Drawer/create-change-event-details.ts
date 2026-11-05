import { createChangeEventDetails } from "../../utils";
import type { DrawerChangeEventDetails, DrawerChangeReason } from "./Drawer.types";

export function createDrawerChangeEventDetails(
  reason: DrawerChangeReason,
  event?: Event,
  trigger?: Element,
): DrawerChangeEventDetails {
  return createChangeEventDetails(reason, event, trigger);
}
