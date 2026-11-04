import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { HashRouter, Route } from "@solidjs/router";
import { createSignal } from "solid-js";
import { describe, expect, it, vi } from "vitest";
import { AppShell } from "../components/AppShell";
import { ChatWorkspace } from "../components/ChatWorkspace";
import { renderWithEngine } from "./render";
import { ChatPage } from "../pages/ChatPage";
import { DevicePage } from "../pages/DevicePage";
import { I18nProvider } from "../i18n";
import { ThemeProvider } from "../theme";
import type { PairingChallenge } from "../state/engine";
import { createMockEngine, MockEngineProvider } from "./mock-engine";
import { makePairing, makePeer, makeSnapshot, makeTransfer } from "./fixtures";
import type { EngineContextValue } from "../state/engine";

/**
 * Keyboard coverage for the main flow. jsdom does not synthesise the click a
 * browser fires for Enter or Space on a native control, so each step asserts
 * two things: the control is a real focusable element, and activating it
 * completes the step. The parts that are genuine key events - the dialog focus
 * trap, Escape, and the chat composer - are driven with keydown.
 */
function renderApp(engine: Partial<EngineContextValue>, hash = "#/"): ReturnType<typeof render> {
  window.location.hash = hash;
  const mock = createMockEngine(engine);
  return render(() => (
    <ThemeProvider>
      <I18nProvider>
        <MockEngineProvider engine={mock}>
          <HashRouter root={AppShell}>
            <Route path="/" component={DevicePage} />
            <Route path="/devices/:deviceId" component={DevicePage} />
            <Route path="/chat" component={ChatPage} />
            <Route path="/chat" component={ChatPage} />
          </HashRouter>
        </MockEngineProvider>
      </I18nProvider>
    </ThemeProvider>
  ));
}

function dialog(): HTMLElement {
  const surface = document.querySelector<HTMLElement>(".fx-dialog");
  if (!surface) throw new Error("no dialog is open");
  return surface;
}

function focusableIn(surface: HTMLElement): HTMLElement[] {
  return Array.from(
    surface.querySelectorAll<HTMLElement>(
      'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
    ),
  ).filter((element) => element.offsetParent !== null);
}

describe("keyboard main flow", () => {
  it("starts at the skip link, which moves focus into the content", () => {
    renderApp({ snapshot: () => makeSnapshot() });

    const skip = screen.getByRole("link", { name: "跳到主要内容" });
    expect(skip).toHaveAttribute("href", "#main");

    fireEvent.click(skip);

    expect(document.activeElement).toBe(document.getElementById("main"));
  });

  it("pairs a nearby device without a pointer", async () => {
    const [challenge, setChallenge] = createSignal<PairingChallenge | null>(null);
    const pairStart = vi.fn(async () => {
      setChallenge(makePairing({ sas: "123456" }));
    });
    const pairConfirm = vi.fn(async () => {});
    renderApp({
      snapshot: () =>
        makeSnapshot({
          peers: [makePeer({ trusted: false, online: true })],
          chat: { conversations: [], messages: [] },
        }),
      pairing: challenge,
      pairStart,
      pairConfirm,
    });

    const deviceRow = screen.getByRole("link", { name: /Peer One/ });
    fireEvent.click(deviceRow);
    const pairButton = await screen.findByRole("button", { name: "配对" });
    expect(pairButton.tagName).toBe("BUTTON");
    pairButton.focus();
    fireEvent.click(pairButton);

    await waitFor(() => expect(pairStart).toHaveBeenCalledWith("peer-1"));
    const confirm = await screen.findByRole("button", { name: "码一致，确认配对" });
    expect(confirm).toBeDisabled();

    await waitFor(() => expect(dialog().contains(document.activeElement)).toBe(true));

    const acknowledge = screen.getByRole("checkbox");
    expect(acknowledge.tagName).toBe("INPUT");
    acknowledge.focus();
    fireEvent.click(acknowledge);
    expect(confirm).toBeEnabled();

    confirm.focus();
    fireEvent.click(confirm);
    await waitFor(() => expect(pairConfirm).toHaveBeenCalledTimes(1));
  });

  it("traps Tab inside the pairing dialog and cancels it with Escape", async () => {
    const [challenge, setChallenge] = createSignal<PairingChallenge | null>(null);
    const pairCancel = vi.fn(async () => {
      setChallenge(null);
    });
    renderApp({
      snapshot: () =>
        makeSnapshot({
          peers: [makePeer({ trusted: false, online: true })],
          chat: { conversations: [], messages: [] },
        }),
      pairing: challenge,
      pairStart: vi.fn(async () => {
        setChallenge(makePairing());
      }),
      pairCancel,
    });

    const deviceRow = screen.getByRole("link", { name: /Peer One/ });
    fireEvent.click(deviceRow);
    const pairButton = await screen.findByRole("button", { name: "配对" });
    pairButton.focus();
    fireEvent.click(pairButton);
    await waitFor(() => expect(dialog()).toBeInTheDocument());

    const items = focusableIn(dialog());
    expect(items.length).toBeGreaterThan(1);

    items[0].focus();
    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(items[items.length - 1]);

    fireEvent.keyDown(document, { key: "Tab" });
    expect(document.activeElement).toBe(items[0]);

    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(pairCancel).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(document.activeElement).toBe(pairButton));
  });

  it("answers an incoming transfer offer from the keyboard", async () => {
    const offer = makeTransfer({ state: "offered", direction: "receive" });
    const respondTransfer = vi.fn(async () => makeTransfer({ state: "transferring" }));
    renderApp(
      {
        snapshot: () => makeSnapshot({ transfers: [offer] }),
        incomingOffer: () => offer,
        respondTransfer,
      },
      "#/transfers",
    );

    const accept = within(await waitFor(() => dialog())).getByRole("button", {
      name: "接收",
    });
    accept.focus();
    fireEvent.click(accept);

    await waitFor(() => expect(respondTransfer).toHaveBeenCalledWith(offer.transfer_id, true));
  });

  it("keeps the chat entry and composer on native controls", async () => {
    // The workspace is props-driven, so this asserts the controls themselves
    // rather than how the chat route happens to be mounted.
    renderWithEngine(() => <ChatWorkspace />, createMockEngine({ snapshot: () => makeSnapshot() }));

    const conversation = await screen.findByRole("button", { name: /Peer One/ });
    expect(conversation.tagName).toBe("BUTTON");
    fireEvent.click(conversation);

    const composer = await screen.findByLabelText("输入消息");
    expect(composer.tagName).toBe("TEXTAREA");
  });
});
