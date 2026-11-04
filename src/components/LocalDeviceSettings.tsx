import { Show, createEffect, createSignal, on } from "solid-js";
import { FolderOpen, Check, LoaderCircle } from "lucide-solid";
import { SettingsRow } from "./SettingsRow";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { Button, Input, toast } from "../ui";

/**
 * Device name and desktop-notification preferences. Both are persisted by the
 * engine; the name is also advertised over mDNS, so saving a new one restarts
 * the LAN runtime and the panel says so before the user commits.
 */
export function LocalDeviceSettings() {
  const engine = useEngine();
  const { t } = useI18n();
  const settings = () => engine.snapshot()!.settings;

  const [name, setName] = createSignal(settings().device_name);
  const [savingName, setSavingName] = createSignal(false);
  const [nameError, setNameError] = createSignal<string | null>(null);
  const [savingNotifications, setSavingNotifications] = createSignal(false);
  const [savingAutoAccept, setSavingAutoAccept] = createSignal(false);
  const [savingAutostart, setSavingAutostart] = createSignal(false);
  const [savingReceive, setSavingReceive] = createSignal(false);
  let field: HTMLInputElement | undefined;

  const dirty = () => name().trim() !== settings().device_name;

  // Adopt a name that changed outside this field (identity reset, another
  // window) unless the user is in the middle of typing.
  createEffect(
    on(
      () => settings().device_name,
      (value) => {
        if (document.activeElement !== field) setName(value);
      },
    ),
  );

  function revert() {
    setName(settings().device_name);
    setNameError(null);
  }

  async function saveName() {
    const value = name().trim();
    if (!value) {
      setNameError(t("settings.deviceNameRequired"));
      field?.focus();
      return;
    }
    if (!dirty()) {
      setName(settings().device_name);
      return;
    }
    setSavingName(true);
    setNameError(null);
    try {
      await engine.setDeviceName(value);
      setName(value);
      toast.success(t("settings.deviceNameSaved"));
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      setNameError(message);
      toast.error(t("settings.deviceNameFailed", { message }));
    } finally {
      setSavingName(false);
    }
  }

  async function toggleNotifications() {
    const next = !settings().notifications;
    setSavingNotifications(true);
    try {
      await engine.setNotifications(next);
      toast.success(next ? t("settings.notificationsEnabled") : t("settings.notificationsMuted"));
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      toast.error(t("settings.notificationsFailed", { message }));
    } finally {
      setSavingNotifications(false);
    }
  }

  async function toggleAutoAccept() {
    const next = !settings().auto_accept_transfers;
    setSavingAutoAccept(true);
    try {
      await engine.setAutoAccept(next);
      toast.success(next ? t("settings.autoAcceptOn") : t("settings.autoAcceptOff"));
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      toast.error(t("settings.autoAcceptFailed", { message }));
    } finally {
      setSavingAutoAccept(false);
    }
  }

  async function chooseReceiveDir() {
    if (savingReceive()) return;
    setSavingReceive(true);
    try {
      const picked = await engine.pickReceiveDir(t("settings.receiveChoose"));
      if (!picked) return;
      await engine.setReceiveDir(picked);
      toast.success(t("settings.receiveSaved"));
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      toast.error(t("settings.receiveFailed", { message }));
    } finally {
      setSavingReceive(false);
    }
  }

  async function toggleAutostart() {
    const next = !settings().autostart;
    setSavingAutostart(true);
    try {
      await engine.setAutostart(next);
      toast.success(next ? t("settings.autostartEnabled") : t("settings.autostartDisabled"));
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      toast.error(t("settings.autostartFailed", { message }));
    } finally {
      setSavingAutostart(false);
    }
  }

  return (
    <>
      <section class="fx-panel" aria-labelledby="device-name-heading">
        <header class="fx-panel-head">
          <div>
            <h2 id="device-name-heading">{t("settings.deviceTitle")}</h2>
            <p>{t("settings.deviceBody")}</p>
          </div>
        </header>
        <form
          class="fx-setting-row is-stacked"
          onSubmit={(event) => {
            event.preventDefault();
            void saveName();
          }}
        >
          <div class="fx-setting-copy">
            <label class="fx-setting-label" for="device-name">
              {t("settings.nameLabel")}
            </label>
            <Show
              when={nameError()}
              fallback={
                <span class="fx-setting-desc" id="device-name-hint">
                  {t("settings.deviceNameHint")}
                </span>
              }
            >
              {(message) => (
                <span class="fx-setting-desc fx-setting-error" id="device-name-error" role="alert">
                  {message()}
                </span>
              )}
            </Show>
          </div>
          <div class="fx-setting-control">
            <Input
              id="device-name"
              ref={field}
              value={name()}
              maxLength={64}
              autocomplete="off"
              spellcheck={false}
              aria-invalid={nameError() ? "true" : undefined}
              aria-describedby={nameError() ? "device-name-error" : "device-name-hint"}
              onInput={(value) => setName(value)}
              onKeyDown={(event) => {
                if (event.key === "Escape") {
                  event.preventDefault();
                  revert();
                }
              }}
            />
            <Button
              type="submit"
              disabled={!dirty() || savingName()}
              icon={savingName() ? <LoaderCircle size={14} class="fx-spin" /> : <Check size={14} />}
            >
              {t("settings.deviceNameSave")}
            </Button>
          </div>
        </form>
      </section>

      <section class="fx-panel" aria-labelledby="receiving-heading">
        <header class="fx-panel-head">
          <div>
            <h2 id="receiving-heading">{t("settings.receivingTitle")}</h2>
            <p>{t("settings.receivingBody")}</p>
          </div>
        </header>

        <SettingsRow
          label={t("settings.notificationsToggle")}
          description={t("settings.notificationsBody")}
        >
          <button
            class="fx-switch-inline"
            type="button"
            role="switch"
            aria-checked={settings().notifications}
            aria-label={t("settings.notificationsToggle")}
            disabled={savingNotifications()}
            onClick={() => void toggleNotifications()}
          >
            <span class="fx-switch-track" aria-hidden="true">
              <span class="fx-switch-thumb" />
            </span>
          </button>
        </SettingsRow>

        <SettingsRow
          label={t("settings.autoAcceptToggle")}
          description={t("settings.autoAcceptBody")}
        >
          <button
            class="fx-switch-inline"
            type="button"
            role="switch"
            aria-checked={settings().auto_accept_transfers}
            aria-label={t("settings.autoAcceptToggle")}
            disabled={savingAutoAccept()}
            onClick={() => void toggleAutoAccept()}
          >
            <span class="fx-switch-track" aria-hidden="true">
              <span class="fx-switch-thumb" />
            </span>
          </button>
        </SettingsRow>

        <SettingsRow label={t("settings.receiveTitle")} description={settings().receive_dir}>
          <Button
            variant="outline"
            size="sm"
            disabled={savingReceive()}
            icon={<FolderOpen size={14} />}
            onClick={() => void chooseReceiveDir()}
          >
            {t("settings.receiveChoose")}
          </Button>
        </SettingsRow>
      </section>

      <Show when={engine.desktop}>
        <section class="fx-panel" aria-labelledby="autostart-heading">
          <header class="fx-panel-head">
            <div>
              <h2 id="autostart-heading">{t("settings.autostartTitle")}</h2>
              <p>{t("settings.autostartBody")}</p>
            </div>
          </header>
          <SettingsRow
            label={t("settings.autostartToggle")}
            description={settings().autostart_location || t("settings.autostartBody")}
          >
            <button
              class="fx-switch-inline"
              type="button"
              role="switch"
              aria-checked={settings().autostart}
              aria-label={t("settings.autostartToggle")}
              disabled={savingAutostart()}
              onClick={() => void toggleAutostart()}
            >
              <span class="fx-switch-track" aria-hidden="true">
                <span class="fx-switch-thumb" />
              </span>
            </button>
          </SettingsRow>
        </section>
      </Show>
    </>
  );
}
