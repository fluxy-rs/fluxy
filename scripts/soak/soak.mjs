#!/usr/bin/env node
// Soak harness for the release candidate (M5-06).
//
// Runs a long, boring loop of the real CLI: pair once, then chat and transfer a
// similarly sized file over and over, verifying every received file by SHA-256.
// It is deliberately boring - the point is that nothing drifts after an hour.
//
//   node scripts/soak/soak.mjs --minutes 8 --size-mib 8
//   node scripts/soak/soak.mjs --minutes 1440 --size-mib 8 --interval-seconds 30
//
// Evidence lands in .soak/log.jsonl (one JSON object per round) and .soak/summary.json.

import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
  appendFileSync,
} from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "..", "..");
const IS_WINDOWS = process.platform === "win32";
const BIN = path.join(
  ROOT,
  "target",
  IS_WINDOWS ? "release" : "release",
  "fluxy-cli" + (IS_WINDOWS ? ".exe" : ""),
);
const SOAK = path.join(ROOT, ".soak");

function arg(name, fallback) {
  const index = process.argv.indexOf("--" + name);
  if (index === -1) return fallback;
  const value = process.argv[index + 1];
  return value === undefined ? fallback : value;
}

const MINUTES = Number(arg("minutes", "60"));
const SIZE_MIB = Number(arg("size-mib", "8"));
const INTERVAL_MS = Number(arg("interval-seconds", "5")) * 1000;
const CHAT_EVERY = Number(arg("chat-every", "1"));
const NO_BUILD = process.argv.includes("--no-build");

function log(message) {
  console.log("[" + new Date().toISOString() + "] " + message);
}

function sha256(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

class Proc {
  constructor(args, options) {
    const settings = options || {};
    this.stdout = "";
    this.stderr = "";
    this.exit = null;
    this.child = spawn(settings.bin || BIN, args, {
      cwd: ROOT,
      env: Object.assign({}, process.env, settings.env || {}),
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    });
    this.child.stdout.setEncoding("utf8");
    this.child.stderr.setEncoding("utf8");
    this.child.stdout.on("data", (chunk) => {
      this.stdout += chunk;
    });
    this.child.stderr.on("data", (chunk) => {
      this.stderr += chunk;
    });
    this.child.on("exit", (code) => {
      this.exit = code;
    });
  }
  writeLine(text) {
    if (!this.child.stdin.destroyed) this.child.stdin.write(text + "\n");
  }
  async waitFor(regex, timeoutMs, label) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const match = this.stdout.match(regex);
      if (match) return match;
      if (this.exit !== null) {
        throw new Error(label + ": process exited " + this.exit + "\n" + this.stderr);
      }
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error(
      label +
        ": timed out after " +
        timeoutMs +
        " ms\nstdout:\n" +
        this.stdout +
        "\nstderr:\n" +
        this.stderr,
    );
  }
  async stop() {
    if (this.exit !== null) return;
    this.child.stdin.end();
    await new Promise((resolve) => setTimeout(resolve, 500));
    if (this.exit === null) this.child.kill();
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
}

async function runCli(args, input, timeoutMs) {
  const proc = new Proc(args);
  if (input) proc.writeLine(input);
  const deadline = Date.now() + (timeoutMs || 120000);
  while (proc.exit === null && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  if (proc.exit === null) {
    proc.child.kill();
    throw new Error("client timed out: " + args.join(" "));
  }
  return { code: proc.exit, stdout: proc.stdout, stderr: proc.stderr };
}

function memoryMegabytes() {
  const result = spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-Command",
      "(Get-Process -Name fluxy-cli -ErrorAction SilentlyContinue | Measure-Object -Property WorkingSet64 -Sum).Sum",
    ],
    { encoding: "utf8", windowsHide: true },
  );
  const value = Number((result.stdout || "").trim());
  return Number.isFinite(value) ? Math.round(value / (1024 * 1024)) : null;
}

async function main() {
  if (!existsSync(BIN)) {
    throw new Error("missing " + BIN + "; run cargo build --release -p fluxy-cli first");
  }
  if (!NO_BUILD) {
    log("building fluxy-cli (release)");
    const build = spawnSync("cargo", ["build", "-q", "--release", "-p", "fluxy-cli"], {
      cwd: ROOT,
      stdio: "inherit",
      shell: IS_WINDOWS,
    });
    if (build.status !== 0) throw new Error("cargo build failed");
  }

  rmSync(SOAK, { recursive: true, force: true });
  mkdirSync(SOAK, { recursive: true });
  const serverDir = path.join(SOAK, "server");
  const clientDir = path.join(SOAK, "client");
  const payloadDir = path.join(SOAK, "payload");
  mkdirSync(payloadDir, { recursive: true });
  const size = SIZE_MIB * 1024 * 1024;
  const payload = Buffer.alloc(size);
  for (let index = 0; index < size; index += 1) payload[index] = (index * 31 + 7) & 0xff;
  const sourcePath = path.join(payloadDir, "soak-source.bin");
  writeFileSync(sourcePath, payload);
  writeFileSync(path.join(SOAK, "log.jsonl"), "");

  log("starting server");
  const server = new Proc([
    "--data-dir",
    serverDir,
    "serve",
    "--listen",
    "127.0.0.1:0",
    "--dev-loopback",
    "--accept-files",
  ]);
  const confirm = setInterval(() => server.writeLine("yes"), 250);
  try {
    const listen = (await server.waitFor(/^listen=(\S+)$/m, 30000, "listen"))[1];
    const device = (await server.waitFor(/^device=(\S+)$/m, 30000, "device"))[1];
    const fingerprint = (
      await server.waitFor(/^fingerprint=([0-9a-f]{64})$/m, 30000, "fingerprint")
    )[1];
    log("server at " + listen + ", pairing");

    const pair = await runCli(
      ["--data-dir", clientDir, "pair", listen, "--fingerprint", fingerprint, "--dev-loopback"],
      "yes",
    );
    if (pair.code !== 0 || !/^paired device=/m.test(pair.stdout)) {
      throw new Error("pairing failed: " + pair.stderr);
    }
    clearInterval(confirm);

    const deadline = Date.now() + MINUTES * 60 * 1000;
    let rounds = 0;
    let failures = 0;
    let bytes = 0;
    const rates = [];
    const memory = [];

    while (Date.now() < deadline) {
      const started = Date.now();
      rounds += 1;
      const name = "soak-" + rounds + ".bin";
      const target = path.join(serverDir, "received", name);
      const source = path.join(payloadDir, name);
      writeFileSync(source, payload);
      try {
        if (rounds % CHAT_EVERY === 0) {
          const chat = await runCli(
            [
              "--data-dir",
              clientDir,
              "chat",
              listen,
              "--device",
              device,
              "--message",
              "soak round " + rounds,
              "--dev-loopback",
            ],
            null,
            60000,
          );
          if (chat.code !== 0) throw new Error("chat exit " + chat.code + ": " + chat.stderr);
        }
        const send = await runCli(
          [
            "--data-dir",
            clientDir,
            "transfer-send",
            listen,
            "--device",
            device,
            "--file",
            source,
            "--dev-loopback",
          ],
          null,
          300000,
        );
        if (send.code !== 0) throw new Error("transfer exit " + send.code + ": " + send.stderr);
        if (!existsSync(target)) throw new Error("received file missing: " + target);
        const sent = sha256(source);
        const received = sha256(target);
        if (sent !== received) throw new Error("hash mismatch on " + name);
        const seconds = (Date.now() - started) / 1000;
        const rate = SIZE_MIB / seconds;
        rates.push(rate);
        bytes += size;
        const memoryMegabytes_ = memoryMegabytes();
        memory.push(memoryMegabytes_);
        appendFileSync(
          path.join(SOAK, "log.jsonl"),
          JSON.stringify({
            round: rounds,
            ok: true,
            seconds: Number(seconds.toFixed(2)),
            mebibytes_per_second: Number(rate.toFixed(1)),
            memory_mb: memoryMegabytes_,
          }) + "\n",
        );
        log("round " + rounds + " ok in " + seconds.toFixed(2) + " s");
      } catch (error) {
        failures += 1;
        appendFileSync(
          path.join(SOAK, "log.jsonl"),
          JSON.stringify({
            round: rounds,
            ok: false,
            error: String(error.message || error),
          }) + "\n",
        );
        log("round " + rounds + " FAILED: " + error.message);
        if (failures >= 3) throw new Error("three failed rounds; stopping");
      }
      rmSync(source, { force: true });
      await new Promise((resolve) =>
        setTimeout(resolve, Math.max(0, INTERVAL_MS - (Date.now() - started))),
      );
    }

    const summary = {
      minutes: MINUTES,
      rounds,
      failures,
      bytes,
      mebibytes: Math.round(bytes / (1024 * 1024)),
      rate_min: rates.length ? Number(Math.min(...rates).toFixed(1)) : null,
      rate_avg: rates.length
        ? Number((rates.reduce((a, b) => a + b, 0) / rates.length).toFixed(1))
        : null,
      rate_max: rates.length ? Number(Math.max(...rates).toFixed(1)) : null,
      memory_first_mb: memory.find((value) => value !== null) ?? null,
      memory_last_mb: [...memory].reverse().find((value) => value !== null) ?? null,
      started_at: new Date(Date.now() - MINUTES * 60 * 1000).toISOString(),
      finished_at: new Date().toISOString(),
    };
    writeFileSync(path.join(SOAK, "summary.json"), JSON.stringify(summary, null, 2) + "\n");
    log("soak finished: " + JSON.stringify(summary));
    if (failures > 0) process.exitCode = 1;
  } finally {
    clearInterval(confirm);
    await server.stop();
  }
}

main().catch((error) => {
  console.error("soak failed: " + (error.stack || error.message || error));
  process.exit(1);
});
