import { createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import { For, Show } from "solid-js";
import { Check, X } from "lucide-solid";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { Button, Dialog, toast } from "../ui";
import { formatBytes } from "../lib/format";

/**
 * The accept/reject prompt for incoming files.
 *
 * A sender that picks several files at once produces one offer per file, so a
 * prompt per offer asks the receiver the same question N times and leaves the
 * queue blocked behind each answer. Everything pending from one batch - or, for
 * a plain multi-select, from one peer at one moment - is answered together.
 *
 * This used to live inside the transfers page, which meant an offer only
 * produced a prompt while that page happened to be mounted. A file sent to a
 * phone sitting on the conversation list therefore arrived, waited, and expired
 * with nothing on screen to answer it. The prompt is a property of the
 * connection, not of a page, so it is mounted once in the shell.
 */
export function IncomingOfferDialog() {
  const engine = useEngine();
  const { t } = useI18n();

  const [pending, setPending] = createSignal(false);

  // The sender releases a whole gesture's offers together, so they normally
  // land within a few milliseconds. Holding the list still for one render turns
  // "1 file, then 3" into one stable prompt; a file that still arrives after
  // the answer follows it without a second prompt (the runtime remembers it).
  const ARRIVAL_WINDOW_MS = 350;
  const [incoming, setIncoming] = createSignal(engine.incomingOffers());
  let timer: ReturnType<typeof setTimeout> | undefined;
  // Whether a prompt is already up is tracked outside the signal: reading it
  // inside the effect would make the effect depend on its own write and spin.
  let shown = false;
  createEffect(() => {
    const next = engine.incomingOffers();
    clearTimeout(timer);
    if (!shown) {
      setIncoming(next);
      shown = next.length > 0;
      return;
    }
    timer = setTimeout(() => {
      setIncoming(next);
      shown = next.length > 0;
    }, ARRIVAL_WINDOW_MS);
  });
  onCleanup(() => clearTimeout(timer));

  const first = () => incoming()[0];
  const totalBytes = createMemo(() => incoming().reduce((sum, item) => sum + item.bytes_total, 0));
  const many = () => incoming().length > 1;

  async function respond(accepted: boolean) {
    const offers = incoming();
    if (offers.length === 0 || pending()) return;
    setPending(true);
    try {
      // Answer every offer in the batch: the sender is blocked on each one, so
      // leaving the rest unanswered stalls the queue behind a prompt that has
      // already been dismissed.
      for (const offer of offers) {
        await engine.respondTransfer(offer.transfer_id, accepted);
      }
      toast.success(
        many()
          ? t(accepted ? "transfers.acceptedMany" : "transfers.rejectedMany", {
              count: offers.length,
            })
          : t(accepted ? "transfers.acceptedToast" : "transfers.rejectedToast", {
              name: offers[0].file_name,
            }),
      );
    } catch (cause) {
      toast.error(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setPending(false);
    }
  }

  return (
    <Dialog
      open={incoming().length > 0}
      // Only the two buttons answer this: the sender stays blocked until the
      // offer is accepted or rejected, so nothing else may close it.
      dismissible={false}
      onOpenChange={() => undefined}
      title={t("transfers.offerTitle")}
      description={
        first()
          ? many()
            ? t("transfers.offerBodyMany", {
                name: first().device_name,
                count: incoming().length,
                size: formatBytes(totalBytes()),
              })
            : t("transfers.offerBody", {
                name: first().device_name,
                file: first().file_name,
                size: formatBytes(first().bytes_total),
              })
          : undefined
      }
      width="sm"
      footer={
        <>
          <Button
            variant="outline"
            disabled={pending()}
            onClick={() => void respond(false)}
            icon={<X size={14} />}
          >
            {t("transfers.reject")}
          </Button>
          <Button
            disabled={pending()}
            onClick={() => void respond(true)}
            icon={<Check size={14} />}
          >
            {t("transfers.accept")}
          </Button>
        </>
      }
    >
      <Show when={many()}>
        <ul class="flex max-h-56 flex-col gap-1 overflow-y-auto">
          <For each={incoming()}>
            {(offer) => (
              <li class="flex items-center justify-between gap-3 rounded-md bg-muted/50 px-2.5 py-1.5">
                <span class="min-w-0 flex-1 truncate text-body">{offer.file_name}</span>
                <span class="shrink-0 font-mono text-label text-muted-foreground">
                  {formatBytes(offer.bytes_total)}
                </span>
              </li>
            )}
          </For>
        </ul>
      </Show>
      <p class="text-label text-muted-foreground">{t("transfers.offerNote")}</p>
    </Dialog>
  );
}
