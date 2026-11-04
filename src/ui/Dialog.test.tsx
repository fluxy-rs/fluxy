import { fireEvent, screen, waitFor } from "@solidjs/testing-library";
import { Show, createSignal } from "solid-js";
import { describe, expect, it, vi } from "vitest";
import { Dialog } from "./Dialog";
import { Button } from "./Button";
import { createMockEngine } from "../test/mock-engine";
import { renderWithEngine } from "../test/render";

/**
 * A parent either keeps the dialog mounted and flips `open`, or unmounts it -
 * the pairing dialog disappears as soon as the challenge is withdrawn.
 */
function Host(props: {
  onOpenChange?: (open: boolean) => void;
  unmountWhenClosed?: boolean;
  dismissible?: boolean;
}) {
  const [open, setOpen] = createSignal(false);
  const close = () => setOpen(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        open
      </button>
      <Show when={!props.unmountWhenClosed || open()}>
        <Dialog
          open={open()}
          onOpenChange={(next) => {
            setOpen(next);
            props.onOpenChange?.(next);
          }}
          title="Dialog title"
          dismissible={props.dismissible}
          footer={
            <>
              <Button variant="outline" onClick={close}>
                cancel
              </Button>
              <Button onClick={close}>confirm</Button>
            </>
          }
        >
          <label>
            acknowledge
            <input type="checkbox" />
          </label>
        </Dialog>
      </Show>
    </>
  );
}

function renderDialog(options: Parameters<typeof Host>[0] = {}) {
  renderWithEngine(() => <Host {...options} />, createMockEngine());
  const trigger = screen.getByRole("button", { name: "open" });
  trigger.focus();
  fireEvent.click(trigger);
  return trigger;
}

function surface(): HTMLElement {
  const element = document.querySelector<HTMLElement>(".fx-dialog");
  if (!element) throw new Error("the dialog is not open");
  return element;
}

describe("Dialog", () => {
  it("focuses the first control and loops Tab in both directions", async () => {
    renderDialog();

    const checkbox = screen.getByRole("checkbox");
    const close = screen.getByRole("button", { name: "关闭" });
    await waitFor(() => expect(document.activeElement).toBe(checkbox));

    // The close control is the last tab stop in the surface.
    close.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(document.activeElement).toBe(checkbox);

    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(close);
  });

  it("closes on Escape and reports the change", async () => {
    const onOpenChange = vi.fn();
    renderDialog({ onOpenChange });

    fireEvent.keyDown(document, { key: "Escape" });

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(surface()).toHaveAttribute("data-open", "false");
  });

  it("returns focus to the trigger it captured", async () => {
    const trigger = renderDialog();
    await waitFor(() => expect(surface().contains(document.activeElement)).toBe(true));

    fireEvent.keyDown(document, { key: "Escape" });

    await waitFor(() => expect(document.activeElement).toBe(trigger));
  });

  it("returns focus when the parent unmounts the dialog", async () => {
    const trigger = renderDialog({ unmountWhenClosed: true });
    await waitFor(() => expect(surface().contains(document.activeElement)).toBe(true));

    fireEvent.click(screen.getByRole("button", { name: "cancel" }));

    await waitFor(() => expect(document.querySelector(".fx-dialog")).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(trigger));
  });
  it("cannot be dismissed while an answer is still required", async () => {
    // A transfer offer blocks the sender until it is accepted or rejected, so
    // Escape and the corner control must not close it.
    renderDialog({ dismissible: false, unmountWhenClosed: true });
    const dialog = surface();

    fireEvent.keyDown(document, { key: "Escape" });
    expect(dialog.isConnected).toBe(true);
    expect(screen.queryByRole("button", { name: "关闭" })).toBeNull();

    // The footer buttons still decide the outcome.
    fireEvent.click(screen.getByRole("button", { name: "confirm" }));
    await waitFor(() => expect(dialog.isConnected).toBe(false));
  });
});
