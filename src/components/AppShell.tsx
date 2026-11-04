import { Show, createEffect, type ParentProps } from "solid-js";
import { useLocation, useNavigate } from "@solidjs/router";
import { TooltipProvider } from "@neut/ui";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { useTheme } from "../theme";
import { ErrorBanner, ErrorState } from "./ErrorState";
import { LoadingState } from "./LoadingState";
import { PairingDialog } from "./PairingDialog";
import { MobileNav } from "./MobileNav";
import { Sidebar } from "./Sidebar";
import { Topbar } from "./Topbar";
import { ToastOutlet, toast } from "../ui";
import { IncomingOfferDialog } from "./IncomingOfferDialog";

function titleKey(pathname: string): string {
  if (pathname.startsWith("/transfers")) return "transfers.title";
  if (pathname.startsWith("/clipboard")) return "clipboard.title";
  if (pathname.startsWith("/settings")) return "settings.title";
  if (pathname.startsWith("/help")) return "help.title";
  return "home.title";
}

export function AppShell(props: ParentProps) {
  const engine = useEngine();
  const { t } = useI18n();
  const theme = useTheme();
  const location = useLocation();
  const navigate = useNavigate();

  /// The desktop device page is a two-column layout that fills the viewport; a
  /// phone has no such page, so it must keep scrolling normally. This cannot be
  /// `engine.desktop`, which only asks whether the Tauri bridge exists and is
  /// equally true on Android.
  const deviceColumn = () =>
    engine.desktop && engine.snapshot()?.status.platform !== "android";

  createEffect(() => {
    document.title = `${t(titleKey(location.pathname))} · Fluxy`;
  });

  createEffect(() => {
    // Keep the window behind the status bar on the page background. The colour
    // comes from the stylesheet so a theme change moves both together; the read
    // is deferred because the stylesheet applies after this effect runs.
    theme.resolved();
    queueMicrotask(() => {
      const value = getComputedStyle(document.documentElement)
        .getPropertyValue("--fx-bg")
        .trim();
      if (value) void engine.setChromeColor(value).catch(() => undefined);
    });
  });

  // The tray cannot drive the router, so it raises a shell request instead.
  createEffect(() => {
    const request = engine.shellRequest();
    if (!request) return;
    engine.clearShellRequest();
    if (request.type === "help_requested") navigate("/help");
  });

  createEffect(() => {
    const notice = engine.pairingNotice();
    if (!notice) return;
    if (notice.kind === "succeeded") {
      toast.success(t("pairing.successToast", { name: notice.device_name }));
    } else if (notice.code === "pairing_expired") {
      toast.error(t("pairing.expiredToast"));
    } else if (
      notice.code === "pairing_rejected" ||
      notice.code === "sas_mismatch"
    ) {
      toast.error(t("pairing.rejectedToast"));
    } else {
      toast.error(t("pairing.failedToast", { message: notice.message }));
    }
    engine.clearPairingNotice();
  });

  return (
    <TooltipProvider delay={420} closeDelay={100}>
      <div class="fx-shell">
        <a
          class="fx-skip"
          href="#main"
          onClick={(event) => {
            event.preventDefault();
            document.getElementById("main")?.focus();
          }}
        >
          {t("nav.skip")}
        </a>
        <Sidebar />
        <div class="fx-workspace">
          <Topbar />
          <main
            id="main"
            class="fx-content"
            classList={{
              // Sections that own the full height - the conversation workspace
              // and, on a desktop, the device list - must not scroll as a whole
              // page: their columns scroll independently.
              //
              // The platform check is not `engine.desktop`: that only asks
              // whether the Tauri bridge exists, which is equally true on
              // Android. Using it here locked the phone's device page to
              // overflow:hidden, so the page could not be dragged at all.
              "fx-content-fixed":
                location.pathname.startsWith("/chat") ||
                (deviceColumn() &&
                  (location.pathname === "/" ||
                    location.pathname.startsWith("/devices"))),
            }}
            tabindex="-1"
          >
            <Show
              when={engine.snapshot()}
              fallback={
                <Show
                  when={engine.error()}
                  fallback={<LoadingState />}
                >
                  {(failure) => (
                    <ErrorState
                      failure={failure()}
                      desktop={engine.desktop}
                      onRetry={() => void engine.refresh()}
                    />
                  )}
                </Show>
              }
            >
              <Show when={engine.error()}>
                {(failure) => (
                  <ErrorBanner
                    failure={failure()}
                    onRetry={() => void engine.refresh()}
                  />
                )}
              </Show>
              {props.children}
            </Show>
          </main>
        </div>
        <MobileNav />
        <PairingDialog />
        <ToastOutlet />
      </div>
      {/* Outside .fx-shell: the dialog renders a container even when closed, and
          as a grid item it claimed a whole row of the shell - 102 px of empty
          space above every page. */}
      <IncomingOfferDialog />
    </TooltipProvider>
  );
}
