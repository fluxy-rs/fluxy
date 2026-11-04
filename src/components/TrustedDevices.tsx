import { For, Show, createSignal } from "solid-js";
import { LoaderCircle, ShieldCheck, Trash2 } from "lucide-solid";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { Button, Dialog, toast } from "../ui";
import { formatDate, shortId } from "../lib/format";

export function TrustedDevices() {
  const engine = useEngine();
  const { t, locale } = useI18n();
  const [target, setTarget] = createSignal<{ device_id: string; name: string } | null>(null);
  const [removing, setRemoving] = createSignal(false);

  const trusted = () => (engine.snapshot()?.peers ?? []).filter((peer) => peer.trusted);

  async function remove() {
    const peer = target();
    if (!peer) return;
    setRemoving(true);
    try {
      await engine.removeTrust(peer.device_id);
      toast.success(t("trust.removed", { name: peer.name }));
      setTarget(null);
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      toast.error(t("trust.removeFailed", { message }));
    } finally {
      setRemoving(false);
    }
  }

  return (
    <section class="fx-panel" aria-labelledby="trust-heading">
      <header class="fx-panel-head">
        <div>
          <h2 id="trust-heading">{t("settings.trustTitle")}</h2>
          <p>{t("settings.trustBody")}</p>
        </div>
        <span class="fx-count">{trusted().length}</span>
      </header>
      <Show
        when={trusted().length > 0}
        fallback={
          <div class="fx-trust-empty">
            <ShieldCheck size={18} aria-hidden="true" />
            <div>
              <strong>{t("trust.emptyTitle")}</strong>
              <p>{t("trust.emptyBody")}</p>
            </div>
          </div>
        }
      >
        <ul class="fx-trust-list">
          <For each={trusted()}>
            {(peer) => (
              <li class="fx-trust-row">
                <span
                  class="fx-status-dot"
                  classList={{ "is-ready": peer.online, "is-offline": !peer.online }}
                  aria-hidden="true"
                />
                <div class="fx-trust-main">
                  <strong>{peer.name}</strong>
                  <span class="fx-trust-meta">
                    {peer.platform} · {t("trust.pairedAt", {
                      date: formatDate(peer.paired_at_ms, locale()),
                    })}
                  </span>
                  <span class="fx-numeric fx-muted">{shortId(peer.device_id, 18)}</span>
                </div>
                <Button
                  variant="outline"
                  size="xs"
                  onClick={() => setTarget({ device_id: peer.device_id, name: peer.name })}
                  icon={<Trash2 size={13} />}
                >
                  {t("trust.remove")}
                </Button>
              </li>
            )}
          </For>
        </ul>
      </Show>
      <p class="fx-note">{t("trust.note")}</p>

      <Dialog
        open={Boolean(target())}
        onOpenChange={(open) => {
          if (!open && !removing()) setTarget(null);
        }}
        title={t("trust.removeTitle", { name: target()?.name ?? "" })}
        description={t("trust.removeBody")}
        width="sm"
        footer={
          <>
            <Button variant="outline" onClick={() => setTarget(null)} disabled={removing()}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="danger"
              onClick={() => void remove()}
              disabled={removing()}
              icon={
                removing() ? (
                  <LoaderCircle size={14} class="fx-spin" />
                ) : (
                  <Trash2 size={14} />
                )
              }
            >
              {t("trust.removeConfirm")}
            </Button>
          </>
        }
      >
        <p class="fx-dialog-warning">{t("trust.removeWarning")}</p>
      </Dialog>
    </section>
  );
}
