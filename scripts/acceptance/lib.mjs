// Shared helpers for the M2-09 acceptance driver.
//
// The driver drives the real CLI: two data directories are paired over
// loopback, then one process sends a file while the other receives it. Every
// helper here is process-level on purpose, so the same code can cut the link,
// kill a receiver, or sample the receiver's memory while it works.

import { spawn } from "node:child_process";
import { createSocket } from "node:dgram";
import { closeSync, mkdirSync, openSync, writeSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

export const isWindows = process.platform === "win32";

export function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export function defaultBinary(release = false) {
  const profile = release ? "release" : "debug";
  return isWindows
    ? "target/" + profile + "/fluxy-cli.exe"
    : "target/" + profile + "/fluxy-cli";
}

export function formatBytes(bytes) {
  const mib = bytes / (1024 * 1024);
  if (mib >= 1024) return (mib / 1024).toFixed(2) + " GiB";
  return mib.toFixed(1) + " MiB";
}

export function parseSize(text) {
  const match = /^(\d+(?:\.\d+)?)\s*(b|kib|mib|gib|kb|mb|gb)?$/i.exec(String(text).trim());
  if (!match) throw new Error("cannot read size: " + text);
  const value = Number.parseFloat(match[1]);
  const unit = (match[2] || "b").toLowerCase();
  const factors = { b: 1, kib: 1024, mib: 1024 * 1024, gib: 1024 * 1024 * 1024 };
  const factor = factors[unit] || factors[unit[0] + "ib"];
  return Math.round(value * factor);
}

/// Run a process to completion and collect its output.
export function run(command, args, options = {}) {
  return new Promise((resolve) => {
    const child = spawn(command, args, {
      cwd: options.cwd,
      env: { ...process.env, ...(options.env || {}) },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
      if (options.onStdout) options.onStdout(chunk.toString());
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
      if (options.onStderr) options.onStderr(chunk.toString());
    });
    child.on("error", (error) => resolve({ code: -1, stdout, stderr: String(error) }));
    child.on("close", (code) => resolve({ code, stdout, stderr }));
  });
}

/// A long-lived CLI process whose stdout lines can be awaited.
export class CliProcess {
  constructor(binary, args, options = {}) {
    this.binary = binary;
    this.args = args;
    this.lines = [];
    this.stdout = "";
    this.stderr = "";
    this.exited = false;
    this.exitCode = null;
    this.waiters = [];
    this.label = options.label || args.join(" ");
    this.confirmSas = options.confirmSas !== false;
    this.child = spawn(binary, args, {
      cwd: options.cwd,
      env: { ...process.env, ...(options.env || {}) },
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.child.stdout.on("data", (chunk) => this.#consume(chunk.toString()));
    this.child.stderr.on("data", (chunk) => {
      this.stderr += chunk;
      // The SAS prompt is written to stderr, so answer it here too.
      if (this.confirmSas && !this.answeredSas && chunk.toString().includes("SAS:")) {
        this.answeredSas = true;
        this.child.stdin.write("yes\n");
      }
    });
    this.child.on("close", (code) => {
      this.exited = true;
      this.exitCode = code;
      this.#notify();
    });
    this.child.on("error", (error) => {
      this.stderr += String(error);
      this.exited = true;
      this.exitCode = -1;
      this.#notify();
    });
  }

  #consume(text) {
    this.stdout += text;
    this.pending = (this.pending || "") + text;
    const parts = this.pending.split(/\r?\n/);
    // The last part may be an incomplete line; keep it for the next chunk.
    this.pending = parts.pop();
    for (const line of parts) {
      if (line.length === 0) continue;
      this.lines.push(line);
    }
    // The SAS prompt has no trailing newline, so answer it as soon as it shows.
    if (this.confirmSas && !this.answeredSas && this.pending.includes("SAS:")) {
      this.answeredSas = true;
      this.child.stdin.write("yes\n");
    }
    this.#notify();
  }

  #notify() {
    for (const waiter of this.waiters.splice(0)) waiter();
  }

  /// Wait until a line matching the prefix appears, or the process exits.
  async waitForLine(prefix, timeoutMs = 30000) {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      const found = this.lines.find((line) => line.includes(prefix));
      if (found) return found;
      if (this.exited) throw new Error(this.label + " exited early: " + this.stderr.trim());
      const remaining = deadline - Date.now();
      if (remaining <= 0) {
        throw new Error(
          this.label + " never printed " + prefix + "; saw: " + this.lines.join(" | "),
        );
      }
      await new Promise((resolve) => {
        const timer = setTimeout(resolve, Math.min(remaining, 200));
        this.waiters.push(() => {
          clearTimeout(timer);
          resolve();
        });
      });
    }
  }

  /// Every machine-readable line, in order.
  xferLines() {
    return this.lines.filter((line) => line.includes("FLUXY_XFER"));
  }

  /// First value of a `key=value` token anywhere in stdout, for fields that
  /// only appear on a non-status line (the sender's pre-hash report).
  fieldFrom(key) {
    return fieldFromText(this.stdout, key);
  }

  /// Parse the machine-readable line into a field map, preferring the final
  /// status line over the earlier banner lines.
  fields(prefix = "FLUXY_XFER") {
    const candidates = this.lines.filter((candidate) => candidate.includes(prefix));
    const line = candidates.find((candidate) => candidate.includes("status=")) || candidates[0];
    if (!line) return {};
    const map = {};
    for (const token of line.slice(line.indexOf(prefix) + prefix.length).trim().split(/\s+/)) {
      const at = token.indexOf("=");
      if (at > 0) map[token.slice(0, at)] = token.slice(at + 1);
    }
    return map;
  }

  /// The last progress value printed by a sender or receiver.
  lastProgress() {
    let value = null;
    for (const line of this.xferLines()) {
      const match = /progress=(\d+)\/(\d+)/.exec(line);
      if (match) value = { done: Number(match[1]), total: Number(match[2]) };
    }
    return value;
  }

  async waitForExit(timeoutMs = 600000) {
    const deadline = Date.now() + timeoutMs;
    while (!this.exited) {
      const remaining = deadline - Date.now();
      if (remaining <= 0) throw new Error(this.label + " did not exit in time");
      await sleep(Math.min(remaining, 100));
    }
    return this.exitCode;
  }

  kill() {
    if (!this.exited) {
      try {
        this.child.kill("SIGKILL");
      } catch {
        // The process already went away.
      }
    }
  }
}

/// A UDP relay with an outage switch, mirroring the Rust chaos proxy.
export async function startRelay(targetPort, targetHost = "127.0.0.1") {
  const socket = createSocket("udp4");
  await new Promise((resolve, reject) => {
    socket.once("error", reject);
    socket.bind(0, "127.0.0.1", resolve);
  });
  let outage = false;
  let client = null;
  let dropped = 0;
  let forwarded = 0;
  socket.on("message", (message, rinfo) => {
    const fromTarget = rinfo.address === targetHost && rinfo.port === targetPort;
    if (fromTarget) {
      if (client === null) return;
      if (outage) {
        dropped += 1;
        return;
      }
      forwarded += 1;
      socket.send(message, client.port, client.address);
      return;
    }
    client = { address: rinfo.address, port: rinfo.port };
    if (outage) {
      dropped += 1;
      return;
    }
    forwarded += 1;
    socket.send(message, targetPort, targetHost);
  });
  return {
    port: socket.address().port,
    addr: "127.0.0.1:" + socket.address().port,
    setOutage(value) {
      outage = value;
    },
    stats() {
      return { forwarded, dropped };
    },
    close() {
      try {
        socket.close();
      } catch {
        // Already closed.
      }
    },
  };
}

/// Write a file of the given size without holding it in memory.
export function createFile(path, size) {
  const fd = openSync(path, "w");
  const block = Buffer.alloc(1 << 20);
  for (let index = 0; index < block.length; index += 1) {
    block[index] = (index * 31 + 7) & 0xff;
  }
  let written = 0;
  while (written < size) {
    const take = Math.min(block.length, size - written);
    writeSync(fd, block, 0, take);
    written += take;
  }
  closeSync(fd);
  return path;
}

/// Peak resident memory of a process, sampled while it works.
export function startMemorySampler(pid, intervalMs = 200) {
  let peak = 0;
  let samples = 0;
  let stopped = false;
  const tick = async () => {
    if (stopped) return;
    const value = await residentBytes(pid);
    if (value !== null) {
      peak = Math.max(peak, value);
      samples += 1;
    }
    if (!stopped) setTimeout(tick, intervalMs);
  };
  setTimeout(tick, 0);
  return {
    stop() {
      stopped = true;
      return { peak, samples };
    },
  };
}

export async function residentBytes(pid) {
  if (isWindows) {
    const result = await run("powershell", [
      "-NoProfile",
      "-Command",
      "(Get-Process -Id " + pid + " -ErrorAction SilentlyContinue).WorkingSet64",
    ]);
    const value = Number.parseInt(result.stdout.trim(), 10);
    return Number.isFinite(value) ? value : null;
  }
  try {
    const text = await import("node:fs").then((fs) =>
      fs.readFileSync("/proc/" + pid + "/status", "utf8"),
    );
    const match = /VmRSS:\s+(\d+)\s+kB/.exec(text);
    return match ? Number.parseInt(match[1], 10) * 1024 : null;
  } catch {
    return null;
  }
}

/// Pull a `key=value` token out of free text (the serve banner is line based).
export function fieldFromText(text, key) {
  for (const token of text.split(/\s+/)) {
    const at = token.indexOf("=");
    if (at > 0 && token.slice(0, at) === key) return token.slice(at + 1);
  }
  return undefined;
}

export function tempDir(prefix) {
  const base = join(tmpdir(), prefix + "-" + Date.now().toString(36) + "-" + Math.floor(Math.random() * 1e6));
  mkdirSync(base, { recursive: true });
  return base;
}

/// Pair two CLI data directories over loopback and return the receiver's
/// device id plus its fingerprint. Both sides confirm the SAS automatically.
export async function pairPeers(binary, dirs, options = {}) {
  const listen = options.listen || "127.0.0.1:0";
  const server = new CliProcess(
    binary,
    ["--data-dir", dirs.receiver, "serve", "--listen", listen, "--dev-loopback", "--once"],
    { label: "serve" },
  );
  try {
    const listenLine = await server.waitForLine("listen=", 20000);
    await server.waitForLine("fingerprint=", 20000);
    const addr = listenLine.slice(listenLine.indexOf("listen=") + "listen=".length).trim();
    const fingerprint = fieldFromText(server.stdout, "fingerprint");
    const device = fieldFromText(server.stdout, "device");
    if (!fingerprint || !device) {
      throw new Error("the pairing server did not report its identity: " + server.stdout);
    }
    const client = new CliProcess(
      binary,
      [
        "--data-dir",
        dirs.sender,
        "pair",
        addr,
        "--fingerprint",
        fingerprint,
        "--dev-loopback",
      ],
      { label: "pair" },
    );
    const clientCode = await client.waitForExit(60000);
    if (clientCode !== 0) {
      throw new Error("pairing failed: " + (client.stdout + client.stderr).trim());
    }
    await server.waitForExit(20000);
    return { addr, fingerprint, device };
  } finally {
    server.kill();
  }
}
