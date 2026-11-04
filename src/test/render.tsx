import { render } from "@solidjs/testing-library";
import type { JSX } from "solid-js";
import { I18nProvider } from "../i18n";
import type { EngineContextValue } from "../state/engine";
import { MockEngineProvider } from "./mock-engine";

export function renderWithEngine(ui: () => JSX.Element, engine: EngineContextValue) {
  return render(() => (
    <I18nProvider>
      <MockEngineProvider engine={engine}>{ui()}</MockEngineProvider>
    </I18nProvider>
  ));
}
