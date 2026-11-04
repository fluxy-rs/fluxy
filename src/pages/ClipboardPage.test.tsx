import { fireEvent, screen, waitFor, within } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { ClipboardPage } from "./ClipboardPage";
import {
  makePeer,
  makeSettings,
  makeShare,
  makeSnapshot,
} from "../test/fixtures";
import { createMockEngine } from "../test/mock-engine";
import { renderWithEngine } from "../test/render";

describe("ClipboardPage", () => {
  it("sends the current clipboard to the selected online peer", async () => {
    const shareClipboard = vi.fn(async () => makeShare({ direction: "sent" }));
    const engine = createMockEngine({
      snapshot: () => makeSnapshot(),
      shareClipboard,
    });

    renderWithEngine(() => <ClipboardPage />, engine);

    fireEvent.click(
      screen.getByRole("button", { name: "发送当前剪贴板" }),
    );
    await waitFor(() =>
      expect(shareClipboard).toHaveBeenCalledWith("peer-1"),
    );
  });

  it("updates the per-device receive policy", async () => {
    const setClipboardPolicy = vi.fn(async () => {});
    const engine = createMockEngine({
      snapshot: () => makeSnapshot(),
      setClipboardPolicy,
    });

    renderWithEngine(() => <ClipboardPage />, engine);
    const policy = screen.getByRole("group", {
      name: "Peer One 的接收策略",
    });

    fireEvent.click(within(policy).getByRole("button", { name: "自动接收" }));
    await waitFor(() =>
      expect(setClipboardPolicy).toHaveBeenCalledWith("peer-1", "auto"),
    );
  });

  it("warns when a device is authorized to auto-receive", () => {
    const engine = createMockEngine({
      snapshot: () =>
        makeSnapshot({
          peers: [makePeer({ clipboard_policy: "auto" })],
        }),
    });

    renderWithEngine(() => <ClipboardPage />, engine);

    expect(
      screen.getByText(
        "自动接收会覆盖本机当前剪贴板，只对明确选择“自动接收”的设备生效。",
      ),
    ).toBeInTheDocument();
  });

  it("accepts a pending incoming offer explicitly", async () => {
    const acceptClipboard = vi.fn(async () =>
      makeShare({ state: "accepted" }),
    );
    const engine = createMockEngine({
      snapshot: () =>
        makeSnapshot({ clipboard: { shares: [makeShare()] } }),
      acceptClipboard,
    });

    renderWithEngine(() => <ClipboardPage />, engine);

    expect(screen.getByText("clipboard preview")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "接收并写入" }));
    await waitFor(() =>
      expect(acceptClipboard).toHaveBeenCalledWith("peer-1", "offer-1"),
    );
  });

  it("rejects a pending incoming offer without writing", async () => {
    const rejectClipboard = vi.fn(async () =>
      makeShare({ state: "rejected" }),
    );
    const engine = createMockEngine({
      snapshot: () =>
        makeSnapshot({ clipboard: { shares: [makeShare()] } }),
      rejectClipboard,
    });

    renderWithEngine(() => <ClipboardPage />, engine);

    const incoming = screen
      .getByRole("heading", { name: "接收邀约" })
      .closest("section");
    expect(incoming).not.toBeNull();
    fireEvent.click(
      within(incoming as HTMLElement).getByRole("button", { name: "拒绝" }),
    );
    await waitFor(() =>
      expect(rejectClipboard).toHaveBeenCalledWith("peer-1", "offer-1"),
    );
  });

  it("turns automatic clipboard sending on and persists the choice", async () => {
    const setAutoClipboard = vi.fn(async () =>
      makeSettings({ auto_clipboard: true }),
    );
    const engine = createMockEngine({
      snapshot: () => makeSnapshot(),
      setAutoClipboard,
    });

    renderWithEngine(() => <ClipboardPage />, engine);

    const toggle = screen.getByRole("switch", { name: /自动发送/ });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    fireEvent.click(toggle);
    await waitFor(() => expect(setAutoClipboard).toHaveBeenCalledWith(true));
  });

  it("explains that received content is never sent back when auto send is on", () => {
    const engine = createMockEngine({
      snapshot: () =>
        makeSnapshot({ settings: makeSettings({ auto_clipboard: true }) }),
    });

    renderWithEngine(() => <ClipboardPage />, engine);

    expect(
      screen.getByRole("switch", { name: /自动发送/ }),
    ).toHaveAttribute("aria-checked", "true");
    expect(
      screen.getByText("将发送给 1 台在线可信设备。"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "收到并写入的内容不会再自动发回，因此两台设备同时开启也不会互相循环。",
      ),
    ).toBeInTheDocument();
  });

  it("shows the offline empty state when no trusted peer is online", () => {
    const engine = createMockEngine({
      snapshot: () =>
        makeSnapshot({
          peers: [makePeer({ online: false })],
          clipboard: { shares: [] },
        }),
    });

    renderWithEngine(() => <ClipboardPage />, engine);

    expect(screen.getByText("没有在线的可信设备")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "发送当前剪贴板" }),
    ).not.toBeInTheDocument();
  });
});