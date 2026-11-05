import { For, Show, createSignal } from "solid-js";
import { LoaderCircle, ShieldCheck, Trash2 } from "lucide-solid";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { Button, Dialog, toast } from "../ui";
import { formatDate, shortId } from "../lib/format";

export function TrustedDevices(props: {
  /**
   * Skip the card shell. The phone already wraps this in a <details>, and two
   * nested rounded borders read as a broken layout rather than a section.
   */
  bare?: boolean;
}) {
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
    <section
      class="overflow-hidden"
      classList={{ "rounded-lg border border-border bg-card": !props.bare }}
      aria-labelledby="trust-heading"
    >
      <header class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
        <div>
          <h2 id="trust-heading" class="text-title font-[620] tracking-[-0.012em]">
            {t("settings.trustTitle")}
          </h2>
          <p class="text-body-sm text-muted-foreground">{t("settings.trustBody")}</p>
        </div>
        <span class="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground">
          {trusted().length}
        </span>
      </header>
      <Show
        when={trusted().length > 0}
        fallback={
          <div class="px-4 py-6 text-center text-sm text-muted-foreground">
            <ShieldCheck size={18} aria-hidden="true" />
            <div>
              <strong>{t("trust.emptyTitle")}</strong>
              <p class="text-body-sm text-muted-foreground">{t("trust.emptyBody")}</p>
            </div>
          </div>
        }
      >
        <ul class="flex flex-col divide-y divide-border">
          <For each={trusted()}>
            {(peer) => (
              <li class="flex items-center gap-3 px-4 py-2.5">
                <span
                  class="size-2 shrink-0 rounded-full bg-muted-foreground/40"
                  classList={{ "is-ready": peer.online, "is-offline": !peer.online }}
                  aria-hidden="true"
                />
                <div class="flex min-w-0 flex-1 flex-col gap-0.5">
                  <strong>{peer.name}</strong>
                  <span class="flex shrink-0 items-center gap-2 text-label text-muted-foreground">
                    {peer.platform} ·{" "}
                    {t("trust.pairedAt", {
                      date: formatDate(peer.paired_at_ms, locale()),
                    })}
                  </span>
                  <span class="font-mono text-label text-muted-foreground">
                    {shortId(peer.device_id, 18)}
                  </span>
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
      <p class="px-4 py-3 text-xs text-muted-foreground">{t("trust.note")}</p>

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
                removing() ? <LoaderCircle size={14} class="animate-spin" /> : <Trash2 size={14} />
              }
            >
              {t("trust.removeConfirm")}
            </Button>
          </>
        }
      >
        <p class="text-xs text-destructive">{t("trust.removeWarning")}</p>
      </Dialog>
    </section>
  );
}
