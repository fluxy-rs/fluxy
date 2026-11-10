import { fireEvent, render, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import type { TransferSummary } from "../bindings";
import { I18nProvider } from "../i18n";
import { makeTransfer } from "../test/fixtures";
import { TransferActions } from "./TransferActions";

function show(state: TransferSummary["state"]) {
  const onAction = vi.fn();
  render(() => (
    <I18nProvider>
      <TransferActions transfer={makeTransfer({ state })} busy={false} onAction={onAction} />
    </I18nProvider>
  ));
  return onAction;
}

describe("TransferActions", () => {
  it("offers pause and cancel while a transfer is moving", () => {
    const onAction = show("transferring");

    expect(screen.getByRole("button", { name: "暂停" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "取消" }));

    expect(onAction).toHaveBeenCalledWith("cancel");
  });

  it("offers resume and cancel for a paused transfer", () => {
    const onAction = show("paused");

    expect(screen.queryByRole("button", { name: "暂停" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "继续" }));

    expect(onAction).toHaveBeenCalledWith("resume");
  });

  it("offers retry once a transfer has failed", () => {
    const onAction = show("failed");

    expect(screen.queryByRole("button", { name: "取消" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "重试" }));

    expect(onAction).toHaveBeenCalledWith("retry");
  });

  it("offers nothing on a completed transfer", () => {
    show("completed");

    expect(screen.queryAllByRole("button")).toHaveLength(0);
  });

  it("disables the buttons while an action is in flight", () => {
    render(() => (
      <I18nProvider>
        <TransferActions
          transfer={makeTransfer({ state: "transferring" })}
          busy
          onAction={() => {}}
        />
      </I18nProvider>
    ));

    expect(screen.getByRole("button", { name: "取消" })).toBeDisabled();
  });
});
