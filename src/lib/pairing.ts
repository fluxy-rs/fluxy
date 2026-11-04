import type { AppSnapshot } from "../bindings";

/**
 * What a pairing code carries.
 *
 * The fingerprint is included because the whole point of scanning is to pair
 * without waiting for discovery: the scanner gets an address and the key to
 * pin, connects, and both screens still compare a six-digit code before trust
 * is stored. A photographed code cannot establish trust on its own - the SAS
 * step cannot be skipped by whoever took the picture.
 */
export type PairingPayload = {
  v: 1;
  t: "fluxy-pair";
  id: string;
  name: string;
  addr: string;
  port: number;
  fp: string;
};

export function pairingPayload(snapshot: AppSnapshot | null): string {
  const status = snapshot?.status;
  if (!status) return "";
  const addr = snapshot?.network?.local_address ?? "";
  const payload: PairingPayload = {
    v: 1,
    t: "fluxy-pair",
    id: status.device_id,
    name: status.device_name,
    addr,
    port: status.listen_port,
    fp: status.identity_fingerprint,
  };
  return JSON.stringify(payload);
}

/** Null when the code is not ours or is missing what pairing needs. */
export function parsePairingPayload(raw: string): PairingPayload | null {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof value !== "object" || value === null) return null;
  const payload = value as Partial<PairingPayload>;
  if (payload.t !== "fluxy-pair") return null;
  if (!payload.id || !payload.fp) return null;
  return {
    v: 1,
    t: "fluxy-pair",
    id: payload.id,
    name: payload.name ?? "",
    addr: payload.addr ?? "",
    port: payload.port ?? 0,
    fp: payload.fp,
  };
}
