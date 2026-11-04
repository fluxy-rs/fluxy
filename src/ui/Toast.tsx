import { Toaster, toast as neutToast } from "@neut/ui";

export const toast = neutToast;

export function ToastOutlet() {
  return (
    <Toaster
      position="bottom-right"
      closeButton
      duration={4200}
      visibleToasts={3}
      toastOptions={{ class: "fx-toast" }}
    />
  );
}
