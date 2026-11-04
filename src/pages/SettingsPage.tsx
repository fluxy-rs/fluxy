import { For, createSignal } from "solid-js";
import { LoaderCircle, Monitor, Moon, RotateCcw, ShieldAlert, Sun } from "lucide-solid";
import { useEngine } from "../state/engine";
import { LocalDeviceSettings } from "../components/LocalDeviceSettings";
import { SettingsLinkRow } from "../components/SettingsRow";
import { useI18n, type Locale } from "../i18n";
import { useTheme, type ThemePreference } from "../theme";
import { Button, Dialog, toast } from "../ui";

const themeOptions: Array<{
  value: ThemePreference;
  key: string;
  icon: typeof Sun;
}> = [
  { value: "system", key: "settings.themeSystem", icon: Monitor },
  { value: "light", key: "settings.themeLight", icon: Sun },
  { value: "dark", key: "settings.themeDark", icon: Moon },
];

const localeOptions: Array<{ value: Locale; key: string }> = [
  { value: "zh-CN", key: "settings.languageZh" },
  { value: "en", key: "settings.languageEn" },
];

export function SettingsPage() {
  const engine = useEngine();
  const { t, locale, setLocale } = useI18n();
  const theme = useTheme();
  const [dialogOpen, setDialogOpen] = createSignal(false);
  const [rotating, setRotating] = createSignal(false);
  // Android 16 gates LAN access behind a runtime permission. Without it
  // discovery silently finds nothing, so the page has to say so rather than
  // looking like there is simply no peer around.
  let rotateTrigger: HTMLButtonElement | undefined;

  const snapshot = () => engine.snapshot()!;
  const status = () => snapshot().status;

  async function rotateIdentity() {
    setRotating(true);
    try {
      await engine.rotate();
      toast.success(t("settings.rotateSuccess"));
      setDialogOpen(false);
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      toast.error(t("settings.rotateFailure", { message }));
    } finally {
      setRotating(false);
    }
  }

  return (
    <div class="fx-page fx-page-narrow">
      <header class="fx-page-head">
        <div>
          <h1>{t("settings.title")}</h1>
          <p>{t("settings.subtitle")}</p>
        </div>
        <div class="fx-page-meta">
          <span class="fx-meta-chip">
            {t("settings.thisVersion", { version: status().version })}
          </span>
        </div>
      </header>

      <div class="fx-settings-grid">
        <section class="fx-panel" aria-labelledby="general-heading">
          <header class="fx-panel-head">
            <div>
              <h2 id="general-heading">{t("settings.generalTitle")}</h2>
            </div>
          </header>

          <div class="fx-setting-row">
            <div class="fx-setting-copy">
              <span class="fx-setting-label">{t("settings.appearanceTitle")}</span>
            </div>
            <div class="fx-setting-control">
              <div class="fx-choice-row" role="group" aria-label={t("settings.appearanceTitle")}>
                <For each={themeOptions}>
                  {(option) => {
                    const Icon = option.icon;
                    return (
                      <button
                        class="fx-choice"
                        classList={{
                          "is-selected": theme.preference() === option.value,
                        }}
                        type="button"
                        aria-pressed={theme.preference() === option.value}
                        onClick={() => theme.setPreference(option.value)}
                      >
                        <Icon size={16} aria-hidden="true" />
                        <span>{t(option.key)}</span>
                      </button>
                    );
                  }}
                </For>
              </div>
            </div>
          </div>

          <div class="fx-setting-row">
            <div class="fx-setting-copy">
              <span class="fx-setting-label">{t("settings.languageTitle")}</span>
            </div>
            <div class="fx-setting-control">
              <div class="fx-choice-row" role="group" aria-label={t("settings.languageTitle")}>
                <For each={localeOptions}>
                  {(option) => (
                    <button
                      class="fx-choice"
                      classList={{ "is-selected": locale() === option.value }}
                      type="button"
                      aria-pressed={locale() === option.value}
                      onClick={() => setLocale(option.value)}
                    >
                      <span>{t(option.key)}</span>
                    </button>
                  )}
                </For>
              </div>
            </div>
          </div>
        </section>

        <LocalDeviceSettings />

        <section class="fx-panel" aria-labelledby="more-heading">
          <header class="fx-panel-head">
            <div>
              <h2 id="more-heading">{t("settings.moreTitle")}</h2>
            </div>
          </header>
          <SettingsLinkRow
            label={t("settings.clipboardLink")}
            description={t("settings.clipboardBody")}
            href="/clipboard"
          />
          <SettingsLinkRow
            label={t("settings.advancedTitle")}
            description={t("settings.advancedBody")}
            href="/settings/advanced"
          />
          <SettingsLinkRow
            label={t("settings.helpTitle")}
            description={t("settings.helpBody")}
            href="/help"
          />
        </section>

        <section class="fx-panel fx-panel-wide fx-danger-zone" aria-labelledby="rotate-heading">
          <header class="fx-panel-head">
            <div>
              <h2 id="rotate-heading">{t("settings.rotateTitle")}</h2>
              <p>{t("settings.rotateBody")}</p>
            </div>
            <ShieldAlert size={18} aria-hidden="true" />
          </header>
          <div class="fx-danger-actions">
            <Button
              ref={(element) => {
                rotateTrigger = element;
              }}
              variant="danger"
              onClick={() => {
                rotateTrigger?.focus();
                setDialogOpen(true);
              }}
              icon={<RotateCcw size={14} />}
            >
              {t("settings.rotateButton")}
            </Button>
          </div>
        </section>
      </div>

      <p class="fx-page-footnote">{t("settings.localOnlyNote")}</p>

      <Dialog
        open={dialogOpen()}
        onOpenChange={setDialogOpen}
        title={t("settings.rotateDialogTitle")}
        description={t("settings.rotateDialogBody")}
        width="sm"
        returnFocus={rotateTrigger}
        footer={
          <>
            <Button variant="outline" onClick={() => setDialogOpen(false)} disabled={rotating()}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="danger"
              onClick={() => void rotateIdentity()}
              disabled={rotating()}
              icon={
                rotating() ? <LoaderCircle size={14} class="fx-spin" /> : <RotateCcw size={14} />
              }
            >
              {t("settings.rotateConfirm")}
            </Button>
          </>
        }
      >
        <p class="fx-dialog-warning">{t("settings.rotateWarning")}</p>
      </Dialog>
    </div>
  );
}
