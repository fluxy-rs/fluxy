import { For, Show } from "solid-js";
import { LoaderCircle, Radio, ShieldCheck, Unplug, Wifi } from "lucide-solid";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { EmptyState } from "./EmptyState";
import { Button } from "../ui";
import { shortId } from "../lib/format";

export function NearbyDevices(props: { compact?: boolean }) {
  const engine = useEngine();
  const { t } = useI18n();
  const nearby = () => (engine.snapshot()?.peers ?? []).filter((peer) => !peer.trusted);
  const pairingBusy = () => engine.pairingBusy() || Boolean(engine.pairing());

  function pair(deviceId: string) {
    // The button stays focusable while a challenge is pending so that closing
    // the dialog returns focus to the control that opened it.
    if (pairingBusy()) return;
    void engine.pairStart(deviceId);
  }

  return (
    <Show
      when={!props.compact}
      fallback={
        <section class="flex flex-col gap-2" aria-labelledby="nearby-rail-heading">
          <div class="text-sm font-medium">
            <span id="nearby-rail-heading">{t("rail.nearby")}</span>
            <span class="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground">
              {nearby().length}
            </span>
          </div>
          <Show
            when={nearby().length > 0}
            fallback={
              <div class="px-4 py-6 text-center text-sm text-muted-foreground">
                <p class="text-body-sm">{t("rail.noNearby")}</p>
                <span>{t("rail.noNearbyBody")}</span>
              </div>
            }
          >
            <ul class="flex flex-col divide-y divide-border">
              <For each={nearby()}>
                {(peer) => (
                  <li>
                    <div class="flex min-w-0 flex-1 flex-col gap-0.5">
                      <span class="truncate text-sm font-medium" title={peer.name}>
                        {peer.name}
                      </span>
                      <span class="font-mono text-label text-muted-foreground">
                        {peer.platform} · {shortId(peer.device_id)}
                      </span>
                    </div>
                    <button
                      class="shrink-0"
                      type="button"
                      aria-label={t("pairing.pairWith", { name: peer.name })}
                      title={t("pairing.pairWith", { name: peer.name })}
                      aria-disabled={pairingBusy() || undefined}
                      aria-busy={engine.pairing()?.peer_device_id === peer.device_id || undefined}
                      onClick={() => pair(peer.device_id)}
                    >
                      <Show
                        when={engine.pairing()?.peer_device_id === peer.device_id}
                        fallback={<Radio size={14} aria-hidden="true" />}
                      >
                        <LoaderCircle size={14} class="animate-spin" aria-hidden="true" />
                      </Show>
                    </button>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </section>
      }
    >
      <section
        class="overflow-hidden rounded-lg border border-border bg-card"
        aria-labelledby="nearby-heading"
      >
        <header class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
          <div>
            <h2 id="nearby-heading">{t("nearby.title")}</h2>
            <p class="text-body-sm text-muted-foreground">{t("nearby.body")}</p>
          </div>
          <span class="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground">
            {nearby().length}
          </span>
        </header>
        <Show
          when={nearby().length > 0}
          fallback={
            <EmptyState
              icon={<Wifi size={22} />}
              title={t("nearby.emptyTitle")}
              body={t("nearby.emptyBody")}
              note={t("nearby.emptyNote")}
            />
          }
        >
          <ul class="grid grid-cols-[repeat(auto-fill,minmax(16rem,1fr))] gap-3 p-4">
            <For each={nearby()}>
              {(peer) => (
                <li class="flex flex-col gap-2 rounded-lg border border-border p-3">
                  <div
                    class="grid size-9 shrink-0 place-items-center rounded-lg bg-muted text-muted-foreground"
                    aria-hidden="true"
                  >
                    <Unplug size={17} />
                  </div>
                  <div class="flex min-w-0 flex-1 flex-col gap-0.5">
                    <div class="truncate text-sm font-medium">{peer.name}</div>
                    <div class="flex items-center gap-2 text-label text-muted-foreground">
                      <span>{peer.platform}</span>
                      <span class="font-mono text-label">{shortId(peer.device_id, 18)}</span>
                    </div>
                  </div>
                  <Button
                    variant="outline"
                    size="xs"
                    aria-disabled={pairingBusy() || undefined}
                    aria-busy={engine.pairing()?.peer_device_id === peer.device_id || undefined}
                    onClick={() => pair(peer.device_id)}
                    icon={
                      engine.pairing()?.peer_device_id === peer.device_id ? (
                        <LoaderCircle size={13} class="animate-spin" />
                      ) : (
                        <ShieldCheck size={13} />
                      )
                    }
                  >
                    {t("pairing.pairAction")}
                  </Button>
                </li>
              )}
            </For>
          </ul>
        </Show>
        <p class="px-4 py-3 text-xs text-muted-foreground">{t("nearby.note")}</p>
      </section>
    </Show>
  );
}
