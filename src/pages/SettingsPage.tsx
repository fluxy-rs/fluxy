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
    <div class="mx-auto flex w-full max-w-[55rem] flex-col gap-4">
      <header class="flex items-end justify-between gap-4">
        <div>
          <h1 class="text-display font-[620] leading-[1.04] tracking-[-0.035em]">
            {t("settings.title")}
          </h1>
          <p class="text-body-sm text-muted-foreground">{t("settings.subtitle")}</p>
        </div>
        <div class="flex items-center gap-2">
          <span class="rounded-full border border-border px-2.5 py-1 text-xs text-muted-foreground">
            {t("settings.thisVersion", { version: status().version })}
          </span>
        </div>
      </header>

      <div class="flex flex-col gap-4">
        <section
          class="overflow-hidden rounded-lg border border-border bg-card"
          aria-labelledby="general-heading"
        >
          <header class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
            <div>
              <h2 id="general-heading" class="text-title font-[620] tracking-[-0.012em]">
                {t("settings.generalTitle")}
              </h2>
            </div>
          </header>

          <div class="flex items-center justify-between gap-4 px-4 py-3 not-last:border-b not-last:border-border">
            <div class="min-w-0">
              <span class="text-sm font-medium">{t("settings.appearanceTitle")}</span>
            </div>
            <div class="flex shrink-0 items-center gap-2">
              <div class="flex gap-1" role="group" aria-label={t("settings.appearanceTitle")}>
                <For each={themeOptions}>
                  {(option) => {
                    const Icon = option.icon;
                    return (
                      <button
                        class="flex items-center gap-1.5 rounded-lg border border-border px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted aria-pressed:border-primary aria-pressed:bg-primary aria-pressed:text-primary-foreground"
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

          <div class="flex items-center justify-between gap-4 px-4 py-3 not-last:border-b not-last:border-border">
            <div class="min-w-0">
              <span class="text-sm font-medium">{t("settings.languageTitle")}</span>
            </div>
            <div class="flex shrink-0 items-center gap-2">
              <div class="flex gap-1" role="group" aria-label={t("settings.languageTitle")}>
                <For each={localeOptions}>
                  {(option) => (
                    <button
                      class="flex items-center gap-1.5 rounded-lg border border-border px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted aria-pressed:border-primary aria-pressed:bg-primary aria-pressed:text-primary-foreground"
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

        <section
          class="overflow-hidden rounded-lg border border-border bg-card"
          aria-labelledby="more-heading"
        >
          <header class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
            <div>
              <h2 id="more-heading" class="text-title font-[620] tracking-[-0.012em]">
                {t("settings.moreTitle")}
              </h2>
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

        <section
          class="rounded-lg border border-destructive/30 bg-destructive/5"
          aria-labelledby="rotate-heading"
        >
          <header class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
            <div>
              <h2 id="rotate-heading" class="text-title font-[620] tracking-[-0.012em]">
                {t("settings.rotateTitle")}
              </h2>
              <p class="text-body-sm text-muted-foreground">{t("settings.rotateBody")}</p>
            </div>
            <ShieldAlert size={18} aria-hidden="true" />
          </header>
          <div class="flex items-center gap-2 px-4 py-3">
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

      <p class="mt-2 text-xs text-muted-foreground">{t("settings.localOnlyNote")}</p>

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
                rotating() ? (
                  <LoaderCircle size={14} class="animate-spin" />
                ) : (
                  <RotateCcw size={14} />
                )
              }
            >
              {t("settings.rotateConfirm")}
            </Button>
          </>
        }
      >
        <p class="text-xs text-destructive">{t("settings.rotateWarning")}</p>
      </Dialog>
    </div>
  );
}
