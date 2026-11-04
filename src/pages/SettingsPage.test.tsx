import { fireEvent, render, screen } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import { SettingsPage } from "./SettingsPage";
import { ThemeProvider } from "../theme";
import { I18nProvider } from "../i18n";
import { createMockEngine, MockEngineProvider } from "../test/mock-engine";
import { makeSnapshot } from "../test/fixtures";

function renderPage() {
  const engine = createMockEngine({ snapshot: () => makeSnapshot() });
  return render(() => (
    <ThemeProvider>
      <I18nProvider>
        <MockEngineProvider engine={engine}>
          <SettingsPage />
        </MockEngineProvider>
      </I18nProvider>
    </ThemeProvider>
  ));
}

describe("SettingsPage", () => {
  it("exposes every settings section the plan lists", () => {
    renderPage();

    // The read-mostly panels (identity, interface, diagnostics) moved to
    // /settings/advanced; this page keeps what a user actually adjusts.
    // Appearance and language are now rows inside one panel, so they are
    // labelled spans rather than headings.
    for (const heading of ["本机设备名", "接收"]) {
      expect(screen.getByRole("heading", { name: heading })).toBeInTheDocument();
    }
    expect(screen.getByRole("button", { name: "轮换本机身份" })).toBeInTheDocument();
  });

  it("applies the theme choice and remembers it", () => {
    renderPage();

    fireEvent.click(screen.getByRole("button", { name: /深色/ }));

    expect(localStorage.getItem("fluxy.theme")).toBe("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(screen.getByRole("button", { name: /深色/ })).toHaveAttribute("aria-pressed", "true");
  });

  it("switches the interface language", () => {
    renderPage();

    fireEvent.click(screen.getByRole("button", { name: "English" }));

    expect(localStorage.getItem("fluxy.locale")).toBe("en");
    expect(screen.getByRole("heading", { name: "Settings" })).toBeInTheDocument();
    expect(document.documentElement.lang).toBe("en");
  });
});
