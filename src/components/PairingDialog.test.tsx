import { fireEvent, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PairingDialog } from "./PairingDialog";
import { makePairing, makeSnapshot } from "../test/fixtures";
import { createMockEngine } from "../test/mock-engine";
import { renderWithEngine } from "../test/render";

afterEach(() => {
  vi.useRealTimers();
});

describe("PairingDialog", () => {
  it("requires the acknowledgement before confirming", async () => {
    const pairConfirm = vi.fn(async () => {});
    const engine = createMockEngine({
      snapshot: () => makeSnapshot(),
      pairing: () => makePairing({ sas: "123456" }),
      pairConfirm,
    });

    renderWithEngine(() => <PairingDialog />, engine);

    expect(screen.getByText("核对短认证码")).toBeInTheDocument();
    expect(screen.getByLabelText("六位短认证码")).toHaveTextContent("123456");
    const confirm = screen.getByRole("button", { name: "码一致，确认配对" });
    expect(confirm).toBeDisabled();

    fireEvent.click(screen.getByRole("checkbox"));
    expect(confirm).toBeEnabled();
    fireEvent.click(confirm);
    await waitFor(() => expect(pairConfirm).toHaveBeenCalledTimes(1));
  });

  it("cancels the challenge when the dialog is closed", async () => {
    const pairCancel = vi.fn(async () => {});
    const engine = createMockEngine({
      snapshot: () => makeSnapshot(),
      pairing: () => makePairing(),
      pairCancel,
    });

    renderWithEngine(() => <PairingDialog />, engine);
    fireEvent.click(screen.getByRole("button", { name: "取消" }));

    await waitFor(() => expect(pairCancel).toHaveBeenCalledTimes(1));
  });

  it("disables confirmation once the code expires", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-22T12:00:00Z"));
    const expiresAt = Date.now() + 500;
    const engine = createMockEngine({
      snapshot: () => makeSnapshot(),
      pairing: () => makePairing({ expires_at_ms: expiresAt }),
    });

    renderWithEngine(() => <PairingDialog />, engine);
    const checkbox = screen.getByRole("checkbox");
    fireEvent.click(checkbox);
    expect(screen.getByRole("button", { name: "码一致，确认配对" })).toBeEnabled();

    await vi.advanceTimersByTimeAsync(700);

    expect(screen.getByText("验证码已失效")).toBeInTheDocument();
    // The visible control is a span with role=checkbox and aria-disabled; the
    // native input behind it is sr-only, so there is no disabled attribute to
    // assert on.
    expect(checkbox).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByRole("button", { name: "码一致，确认配对" })).toBeDisabled();
  });

  it("keeps confirmation disabled while a request is in flight", () => {
    const engine = createMockEngine({
      snapshot: () => makeSnapshot(),
      pairing: () => makePairing(),
      pairingBusy: () => true,
    });

    renderWithEngine(() => <PairingDialog />, engine);

    expect(screen.getByRole("checkbox")).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByRole("button", { name: "码一致，确认配对" })).toBeDisabled();
  });
});
