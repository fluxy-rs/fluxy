/* @refresh reload */
import { Show } from "solid-js";
import { render } from "solid-js/web";
import { HashRouter, Route } from "@solidjs/router";
import { AppShell } from "./components/AppShell";
import { I18nProvider } from "./i18n";
import { EngineProvider, useEngine } from "./state/engine";
import { ThemeProvider } from "./theme";
import { ClipboardPage } from "./pages/ClipboardPage";
import { HelpPage } from "./pages/HelpPage";
import { ChatPage } from "./pages/ChatPage";
import { DevicePage } from "./pages/DevicePage";
import { MobilePage } from "./pages/MobilePage";
import { SettingsPage } from "./pages/SettingsPage";
import { AdvancedSettingsPage } from "./pages/AdvancedSettingsPage";
import "./styles.css";

/// On a phone the desktop home page (device grid, drag and drop) is the wrong
/// entrance; the platform reports android and the send page becomes home.
/// Desktop opens straight into the conversation workspace: the list of people
/// you can talk to and the open thread, side by side. A phone still needs the
/// device page first, because pairing has nowhere else to live there.
/// Desktop opens on the device list - the one thing a new install has to do is
/// pair. A phone keeps its own device page, because pairing there lives behind
/// the header actions.
function RootPage() {
  const engine = useEngine();
  const mobile = () => engine.snapshot()?.status.platform === "android";
  return (
    <Show when={mobile()} fallback={<DevicePage />}>
      <MobilePage />
    </Show>
  );
}

/// One device's details. Desktop shows it beside the list; a phone has no room
/// for that, so it falls back to the list page.
function DeviceDetail() {
  const engine = useEngine();
  const mobile = () => engine.snapshot()?.status.platform === "android";
  return (
    <Show when={mobile()} fallback={<DevicePage />}>
      <MobilePage />
    </Show>
  );
}

render(
  () => (
    <ThemeProvider>
      <I18nProvider>
        <EngineProvider>
          <HashRouter root={AppShell}>
            <Route path="/" component={RootPage} />
            <Route path="/devices/:deviceId" component={DeviceDetail} />
            <Route path="/send" component={MobilePage} />
            <Route path="/chat" component={ChatPage} />
            <Route path="/chat/:deviceId" component={ChatPage} />
                <Route path="/clipboard" component={ClipboardPage} />
            <Route path="/settings" component={SettingsPage} />
            <Route path="/settings/advanced" component={AdvancedSettingsPage} />
            <Route path="/help" component={HelpPage} />
          </HashRouter>
        </EngineProvider>
      </I18nProvider>
    </ThemeProvider>
  ),
  document.getElementById("root") as HTMLElement,
);
