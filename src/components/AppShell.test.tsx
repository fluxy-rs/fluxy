import { render, screen, waitFor } from "@solidjs/testing-library";
import { HashRouter, Route } from "@solidjs/router";
import { createSignal } from "solid-js";
import { describe, expect, it, vi } from "vitest";
import { AppShell } from "./AppShell";
import { HomePage } from "../pages/HomePage";
import { I18nProvider } from "../i18n";
import { ThemeProvider } from "../theme";
import type { ShellRequest } from "../state/engine";
import { createMockEngine, MockEngineProvider } from "../test/mock-engine";
import { makeSnapshot } from "../test/fixtures";

function renderShell(shellRequest: () => ShellRequest | null, clear: () => void) {
  const engine = createMockEngine({
    snapshot: () => makeSnapshot(),
    shellRequest,
    clearShellRequest: clear,
  });
  window.location.hash = "#/";
  return render(() => (
    <ThemeProvider>
      <I18nProvider>
        <MockEngineProvider engine={engine}>
          <HashRouter root={AppShell}>
            <Route path="/" component={HomePage} />
            <Route path="/help" component={() => <div>HELP ROUTE</div>} />
          </HashRouter>
        </MockEngineProvider>
      </I18nProvider>
    </ThemeProvider>
  ));
}

describe("AppShell", () => {
  it("navigates to the help page when the tray asks for it", async () => {
    const [request, setRequest] = createSignal<ShellRequest | null>(null);
    const clearShellRequest = vi.fn(() => setRequest(null));
    renderShell(request, clearShellRequest);

    expect(screen.queryByText("HELP ROUTE")).not.toBeInTheDocument();

    setRequest({ type: "help_requested" });

    await waitFor(() => expect(screen.getByText("HELP ROUTE")).toBeInTheDocument());
    // The request is consumed so a later re-render does not navigate again.
    await waitFor(() => expect(clearShellRequest).toHaveBeenCalled());
  });
});
