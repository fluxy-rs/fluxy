import { Monitor, Moon, RefreshCw, Sun } from "lucide-solid";
import { useI18n } from "../i18n";
import { IconButton } from "../ui";
import { useEngine } from "../state/engine";
import { useTheme, type ThemePreference } from "../theme";

function themeKey(preference: ThemePreference) {
  if (preference === "light") return "topbar.themeLight";
  if (preference === "dark") return "topbar.themeDark";
  return "topbar.themeSystem";
}

/**
 * One line of chrome: whether the engine is up, and the two things worth a
 * click from every page. The transport counters that used to sit here (seq,
 * snapshot version, event count) are diagnostics, not something a person
 * reading their messages needs; the debug bridge still exposes them.
 */
export function Topbar() {
  const engine = useEngine();
  const { t } = useI18n();
  const theme = useTheme();

  const statusText = () =>
    engine.loading()
      ? t("status.connecting")
      : engine.snapshot()
        ? t("status.ready")
        : t("status.offline");

  const statusClass = () =>
    engine.loading()
      ? "is-pending"
      : engine.snapshot()
        ? "is-ready"
        : "is-offline";

  const ThemeIcon = () =>
    theme.preference() === "light" ? (
      <Sun size={16} />
    ) : theme.preference() === "dark" ? (
      <Moon size={16} />
    ) : (
      <Monitor size={16} />
    );

  return (
    <header class="fx-topbar">
      <div class="fx-topbar-context">
        <span class="fx-mobile-brand">Fluxy</span>
        <span class={`fx-status-dot ${statusClass()}`} aria-hidden="true" />
        <span class="fx-topbar-status">{statusText()}</span>
        <span class="fx-topbar-sep" aria-hidden="true" />
        <span class="fx-muted">{t("app.lanOnly")}</span>
      </div>
      <div class="fx-topbar-actions">
        <IconButton
          label={`${t("topbar.theme")}: ${t(themeKey(theme.preference()))}`}
          onClick={theme.cycle}
        >
          <ThemeIcon />
        </IconButton>
        <IconButton
          label={t("topbar.refresh")}
          onClick={() => void engine.refresh()}
          disabled={engine.loading()}
        >
          <RefreshCw size={16} class={engine.loading() ? "fx-spin" : ""} />
        </IconButton>
      </div>
    </header>
  );
}
