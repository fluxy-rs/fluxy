import { fireEvent, screen, waitFor } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { ChatWorkspace } from "./ChatWorkspace";
import { makeConversation, makeMessage, makeSnapshot, makeTransfer } from "../test/fixtures";
import { createMockEngine } from "../test/mock-engine";
import { renderWithEngine } from "../test/render";

describe("ChatWorkspace", () => {
  it("opens a received file when its card is clicked", () => {
    const snapshot = makeSnapshot({
      chat: { conversations: [makeConversation()], messages: [] },
      transfers: [makeTransfer({ direction: "receive", state: "completed" })],
    });
    const engine = createMockEngine({
      snapshot: () => snapshot,
      openReceivedTransfer: vi.fn(async () => {}),
    });

    renderWithEngine(() => <ChatWorkspace />, engine);
    fireEvent.click(screen.getByText("report.bin"));

    expect(engine.openReceivedTransfer).toHaveBeenCalledWith("transfer-1");
  });

  it("offers the file actions on a completed transfer", async () => {
    const snapshot = makeSnapshot({
      chat: { conversations: [makeConversation()], messages: [] },
      transfers: [makeTransfer({ state: "completed" })],
    });
    const engine = createMockEngine({ snapshot: () => snapshot });

    renderWithEngine(() => <ChatWorkspace />, engine);
    fireEvent.contextMenu(screen.getByText("report.bin"));

    expect(await screen.findByRole("menuitem", { name: "显示传输详情" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "在目录中显示" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "删除此文件" })).toBeInTheDocument();
  });

  it("shows no menu while a file is still moving", () => {
    const snapshot = makeSnapshot({
      chat: { conversations: [makeConversation()], messages: [] },
      transfers: [makeTransfer({ state: "transferring" })],
    });
    const engine = createMockEngine({ snapshot: () => snapshot });

    renderWithEngine(() => <ChatWorkspace />, engine);
    fireEvent.contextMenu(screen.getByText("report.bin"));

    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("shows the rate next to the progress of a moving transfer", () => {
    const snapshot = makeSnapshot({
      chat: { conversations: [makeConversation()], messages: [] },
      transfers: [makeTransfer({ state: "transferring", bytes_per_second: 12 * 1024 * 1024 })],
    });
    const engine = createMockEngine({ snapshot: () => snapshot });

    const { container } = renderWithEngine(() => <ChatWorkspace />, engine);

    expect(container.textContent).toContain("12.0 MB/s");
  });

  it("renders the first conversation and its messages", () => {
    const snapshot = makeSnapshot({
      chat: {
        conversations: [makeConversation()],
        messages: [
          makeMessage({ message_id: "sent-1", text: "Hello" }),
          makeMessage({
            message_id: "received-1",
            direction: "received",
            state: "received",
            text: "Hi there",
          }),
        ],
      },
    });
    const engine = createMockEngine({ snapshot: () => snapshot });

    renderWithEngine(() => <ChatWorkspace />, engine);

    expect(screen.getByRole("heading", { name: "Peer One" })).toBeInTheDocument();
    expect(screen.getByText("Hello")).toBeInTheDocument();
    expect(screen.getByText("Hi there")).toBeInTheDocument();
    expect(screen.getByLabelText("输入消息")).toBeInTheDocument();
  });

  it("sends a trimmed draft and clears the composer", async () => {
    const engine = createMockEngine({
      snapshot: () => makeSnapshot(),
      sendChat: vi.fn(async () => makeMessage({ text: "New message" })),
    });

    renderWithEngine(() => <ChatWorkspace />, engine);

    const textarea = screen.getByLabelText("输入消息");
    fireEvent.input(textarea, { target: { value: "  New message  " } });
    fireEvent.click(screen.getByRole("button", { name: "发送" }));

    await waitFor(() => expect(engine.sendChat).toHaveBeenCalledWith("peer-1", "New message"));
    await waitFor(() => expect(textarea).toHaveValue(""));
  });

  it("sends on Enter and keeps Shift+Enter for a newline", async () => {
    const sendChat = vi.fn(async () => makeMessage());
    const engine = createMockEngine({
      snapshot: () => makeSnapshot(),
      sendChat,
    });

    renderWithEngine(() => <ChatWorkspace />, engine);
    const textarea = screen.getByLabelText("输入消息");

    fireEvent.input(textarea, { target: { value: "with shift" } });
    fireEvent.keyDown(textarea, { key: "Enter", shiftKey: true });
    expect(sendChat).not.toHaveBeenCalled();

    fireEvent.input(textarea, { target: { value: "send now" } });
    fireEvent.keyDown(textarea, { key: "Enter", shiftKey: false });
    await waitFor(() => expect(sendChat).toHaveBeenCalledWith("peer-1", "send now"));
  });

  it("keeps the draft and recovers the composer when sending fails", async () => {
    const sendChat = vi.fn(async () => {
      throw new Error("offline");
    });
    const engine = createMockEngine({
      snapshot: () => makeSnapshot(),
      sendChat,
    });

    renderWithEngine(() => <ChatWorkspace />, engine);
    const textarea = screen.getByLabelText("输入消息");

    fireEvent.input(textarea, { target: { value: "keep me" } });
    fireEvent.click(screen.getByRole("button", { name: "发送" }));

    await waitFor(() => expect(sendChat).toHaveBeenCalledWith("peer-1", "keep me"));
    await waitFor(() => expect(textarea).toHaveValue("keep me"));
    expect(screen.getByRole("button", { name: "发送" })).toBeEnabled();
  });

  it("shows a failed message with its retry action", async () => {
    const retryChat = vi.fn(async () => makeMessage());
    const snapshot = makeSnapshot({
      chat: {
        conversations: [makeConversation()],
        messages: [
          makeMessage({
            message_id: "failed-1",
            state: "failed",
            error_code: "connection_lost",
            error_message: "socket closed",
            retryable: true,
            text: "Lost message",
          }),
        ],
      },
    });
    const engine = createMockEngine({
      snapshot: () => snapshot,
      retryChat,
    });

    renderWithEngine(() => <ChatWorkspace />, engine);

    expect(screen.getByText("连接已断开，消息未送达；可以重试。")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "重试发送给 Peer One 的消息" }));
    await waitFor(() => expect(retryChat).toHaveBeenCalledWith("peer-1", "failed-1"));
  });

  it("replaces the composer with an offline hint", () => {
    const snapshot = makeSnapshot({
      peers: [],
      chat: {
        conversations: [makeConversation({ online: false, session_state: "idle" })],
        messages: [],
      },
    });
    const engine = createMockEngine({ snapshot: () => snapshot });

    renderWithEngine(() => <ChatWorkspace />, engine);

    expect(screen.queryByLabelText("输入消息")).not.toBeInTheDocument();
    expect(
      screen.getByText("设备当前离线。等它重新出现在局域网后即可继续聊天。"),
    ).toBeInTheDocument();
  });
});
