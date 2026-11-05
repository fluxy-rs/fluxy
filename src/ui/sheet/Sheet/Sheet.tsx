import { Dialog } from "../../dialog";
import type { SheetProps } from "../sheet.types";

/**
 */
export function Sheet(props: SheetProps) {
  return <Dialog {...props} data-slot="sheet" />;
}
