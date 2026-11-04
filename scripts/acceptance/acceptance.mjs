#!/usr/bin/env node
// M2-09 acceptance driver.
//
// Drives the real CLI over loopback to exercise the acceptance matrix from
// docs/DEVELOPMENT_PLAN.md 11.2. Scenarios:
//
//   local    pair two peers, transfer one file, compare root hashes
//   outage   cut the UDP path mid-transfer and resume (T-02)
//   restart  kill the receiver mid-transfer and resume (T-03)
//   bench    transfer a large file with a receiver memory ceiling (T-01)
//   faults   run the protocol-level fault suite (T-02/T-03/T-04/T-06)
//   matrix   run everything above and print the T-01..T-06 report
//
// Every process-level scenario pairs fresh data directories first, so the
// transfer itself never needs a SAS prompt.

import { readdirSync, rmSync, statSync } from "node:fs";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import {
  CliProcess,
  createFile,
  defaultBinary,
  formatBytes,
  isWindows,
  parseSize,
  pairPeers,
  run,
  sleep,
  startMemorySampler,
  startRelay,
  tempDir,
} from "./lib.mjs";

const USAGE = [
  "usage: node scripts/acceptance/acceptance.mjs [scenario] [options]",
  "",
  "scenarios: local (default), outage, restart, bench, faults, matrix",
  "",
  "options:",
  "  --bin PATH        CLI binary (default target/debug/fluxy-cli[.exe])",
  "  --no-build        skip cargo build -p fluxy-cli",
  "  --release         use the release profile (needed for the throughput target)",
  "  --block-size SIZE chunk size for the sender (default 4MiB)",
  "  --size SIZE       payload size for local/restart/matrix (default 64MiB)",
  "  --file PATH       existing file for bench (default: generated --size)",
  "  --outage SECONDS  outage length for the outage scenario (default 30)",
  "  --timeout SECONDS receiver offer timeout (default 300)",
  "  --transfer-id ID  reuse a transfer id (default: random)",
  "  --keep            keep the temporary data directory",
  "  --json            print the report as JSON",
  "",
  "T-05 (revoked file access) needs an ACL change on the source file and is",
  "documented as a manual step in scripts/acceptance/README.md.",
  "",
].join("\n");

function parseOptions(argv) {
  const options = {
    scenario: "local",
    size: "64MiB",
    outage: 30,
    timeout: 300,
    keep: false,
    json: false,
    build: true,
  };
  const rest = [];
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = () => {
      index += 1;
      if (index >= argv.length) throw new Error(arg + " needs a value");
      return argv[index];
    };
    switch (arg) {
      case "--bin":
        options.bin = next();
        break;
      case "--no-build":
        options.build = false;
        break;
      case "--release":
        options.release = true;
        break;
      case "--block-size":
        options.blockSize = next();
        break;
      case "--size":
        options.size = next();
        break;
      case "--file":
        options.file = next();
        break;
      case "--outage":
        options.outage = Number.parseFloat(next());
        break;
      case "--timeout":
        options.timeout = Number.parseFloat(next());
        break;
      case "--transfer-id":
        options.transferId = next();
        break;
      case "--keep":
        options.keep = true;
        break;
      case "--data-dir":
        options.dataDir = next();
        break;
      case "--json":
        options.json = true;
        break;
      case "--help":
      case "-h":
        options.help = true;
        break;
      default:
        if (arg.startsWith("--")) throw new Error("unknown option: " + arg);
        rest.push(arg);
    }
  }
  if (rest.length > 0) options.scenario = rest[0];
  return options;
}

/// Bytes already on disk under a directory tree.
function dirSize(path) {
  let total = 0;
  let entries = [];
  try {
    entries = readdirSync(path, { withFileTypes: true });
  } catch {
    return 0;
  }
  for (const entry of entries) {
    const child = join(path, entry.name);
    if (entry.isDirectory()) {
      total += dirSize(child);
    } else {
      try {
        total += statSync(child).size;
      } catch {
        // The file went away between listing and stat.
      }
    }
  }
  return total;
}

function field(line, key) {
  for (const token of line.split(/\s+/)) {
    const at = token.indexOf("=");
    if (at > 0 && token.slice(0, at) === key) return token.slice(at + 1);
  }
  return undefined;
}

class Report {
  constructor() {
    this.rows = [];
  }

  add(id, name, status, detail) {
    this.rows.push({ id, name, status, detail });
    const line = "  " + id.padEnd(6) + name.padEnd(38) + status + (detail ? "  " + detail : "");
    console.log(line);
  }

  print() {
    console.log("");
    console.log("Acceptance matrix");
    for (const row of this.rows) {
      console.log(
        "  " + row.id.padEnd(6) + row.status.padEnd(9) + row.name +
          (row.detail ? "  (" + row.detail + ")" : ""),
      );
    }
    const failed = this.rows.filter((row) => row.status === "FAIL").length;
    console.log("");
    console.log(failed === 0 ? "Result: PASS" : "Result: FAIL (" + failed + " failed)");
    return failed === 0;
  }
}

async function prepare(options, label) {
  if (options.build) {
    const profile = options.release ? ["--release"] : [];
    console.log("[build] cargo build " + profile.join(" ") + " -p fluxy-cli");
    const build = await run("cargo", ["build", ...profile, "-p", "fluxy-cli"]);
    if (build.code !== 0) {
      throw new Error("cargo build -p fluxy-cli failed:\n" + build.stderr);
    }
  }
  const binary = options.bin || defaultBinary(options.release);
  const root = options.dataDir || tempDir("fluxy-accept");
  const dirs = { root, receiver: join(root, "receiver"), sender: join(root, "sender") };
  console.log("[" + label + "] pairing two peers in " + root);
  const paired = await pairPeers(binary, dirs);
  return { binary, root, dirs, paired };
}

function startReceiver(binary, dirs, options, label) {
  return new CliProcess(
    binary,
    [
      "--data-dir",
      dirs.receiver,
      "transfer-recv",
      "--listen",
      options.listen || "127.0.0.1:0",
      "--dev-loopback",
      "--timeout",
      String(options.timeout),
    ],
    { label },
  );
}

function startSender(binary, dirs, options, paired, addr, source, transferId) {
  const args = [
    "--data-dir",
    dirs.sender,
    "transfer-send",
    addr,
    "--device",
    paired.device,
    "--file",
    source,
    "--dev-loopback",
  ];
  if (transferId) args.push("--transfer-id", transferId);
  if (options.blockSize) args.push("--block-size", String(parseSize(options.blockSize)));
  return new CliProcess(binary, args, { label: "transfer-send" });
}

async function receiverAddress(receiver) {
  const line = await receiver.waitForLine("FLUXY_XFER role=recv listen=", 30000);
  return field(line, "listen");
}

function outcome(process) {
  const line = process.xferLines().find((candidate) => candidate.includes("status="));
  if (!line) return { status: "missing" };
  return process.fields();
}

async function runLocal(options, report, label = "local") {
  const { binary, root, dirs, paired } = await prepare(options, label);
  const size = parseSize(options.size);
  const source = createFile(join(root, "payload.bin"), size);
  const receiver = startReceiver(binary, dirs, options, "transfer-recv");
  const addr = await receiverAddress(receiver);
  const sender = startSender(binary, dirs, options, paired, addr, source, options.transferId);
  const sendCode = await sender.waitForExit(900000);
  const recvCode = await receiver.waitForExit(900000);
  const sent = sender.fields();
  const received = receiver.fields();
  const same = sent.root_hash && sent.root_hash === received.root_hash;
  const ok = sendCode === 0 && recvCode === 0 && same && received.size === String(size);
  report.add(
    "T-01",
    "single file transfer, root hash match",
    ok ? "PASS" : "FAIL",
    formatBytes(size) + ", sender " + (sent.throughput_mib_s || "?") + " MiB/s, receiver " +
      (received.throughput_mib_s || "?") + " MiB/s, root " + (received.root_hash || "?").slice(0, 16),
  );
  if (!ok) {
    console.log(sender.stdout);
    console.log(sender.stderr);
    console.log(receiver.stdout);
    console.log(receiver.stderr);
  }
  if (!options.keep) rmSync(root, { recursive: true, force: true });
  return ok;
}

async function runOutage(options, report, label = "outage") {
  const { binary, root, dirs, paired } = await prepare(options, label);
  const size = parseSize(options.size);
  const source = createFile(join(root, "payload.bin"), size);
  const transferId = options.transferId || randomUUID();

  const receiver = startReceiver(binary, dirs, options, "transfer-recv");
  const receiverAddr = await receiverAddress(receiver);
  const relay = await startRelay(Number.parseInt(receiverAddr.split(":")[1], 10));

  let sender = startSender(
    binary,
    dirs,
    options,
    paired,
    relay.addr,
    source,
    transferId,
  );
  // Cut the link once the receiver has some of the file on disk. The progress
  // line is throttled to two seconds, so the partial file is the reliable
  // signal that the transfer itself is moving (not just the pre-hash).
  const partialDir = join(dirs.receiver, "received");
  const deadline = Date.now() + 20000;
  let onDisk = 0;
  while (Date.now() < deadline) {
    onDisk = dirSize(partialDir);
    if (onDisk > 0 || sender.exited || receiver.exited) break;
    await sleep(20);
  }
  if (onDisk > 0) {
    console.log("[" + label + "] the receiver has " + formatBytes(onDisk) + " on disk");
  }
  console.log("[" + label + "] cutting the UDP path for " + options.outage + "s");
  relay.setOutage(true);
  await sleep(options.outage * 1000);
  relay.setOutage(false);
  const stats = relay.stats();
  relay.close();

  await sender.waitForExit(900000);
  await receiver.waitForExit(900000);
  const received = outcome(receiver);
  let detail = "cut " + options.outage + "s, dropped " + stats.dropped + " datagrams";
  let ok = received.status === "ok" && received.root_hash;
  let resumed = false;
  if (!ok) {
    console.log("[" + label + "] the first run failed; retrying with the same transfer id");
    const retryReceiver = startReceiver(binary, dirs, options, "transfer-recv-retry");
    const retryAddr = await receiverAddress(retryReceiver);
    const retrySender = startSender(
      binary,
      dirs,
      options,
      paired,
      retryAddr,
      source,
      transferId,
    );
    const retrySendCode = await retrySender.waitForExit(900000);
    const retryRecvCode = await retryReceiver.waitForExit(900000);
    const second = outcome(retryReceiver);
    resumed = second.resumed === "true";
    ok =
      retrySendCode === 0 &&
      retryRecvCode === 0 &&
      second.status === "ok" &&
      resumed &&
      Number(second.missing_bytes_before) < size;
    detail += ", resumed " + formatBytes(Number(second.missing_bytes_before || 0)) + " of " +
      formatBytes(size);
    if (!ok) {
      console.log(retrySender.stdout);
      console.log(retryReceiver.stdout);
      console.log(retryReceiver.stderr);
    }
  } else {
    detail += ", recovered in place";
  }
  report.add("T-02", "link outage and resume", ok ? "PASS" : "FAIL", detail);
  if (!options.keep) rmSync(root, { recursive: true, force: true });
  return ok;
}

async function runRestart(options, report, label = "restart") {
  const { binary, root, dirs, paired } = await prepare(options, label);
  const size = parseSize(options.size);
  const source = createFile(join(root, "payload.bin"), size);
  const transferId = options.transferId || randomUUID();

  const receiver = startReceiver(binary, dirs, options, "transfer-recv");
  const addr = await receiverAddress(receiver);
  const sender = startSender(binary, dirs, options, paired, addr, source, transferId);

  // Wait for the receiver to put some of the file on disk, then kill it. The
  // progress line is throttled to two seconds, so the partial file itself is
  // the reliable signal for a short transfer.
  const partialDir = join(dirs.receiver, "received");
  const deadline = Date.now() + 60000;
  let checkpointed = 0;
  while (Date.now() < deadline) {
    checkpointed = dirSize(partialDir);
    if (checkpointed >= Math.floor(size * 0.2) || sender.exited) break;
    await sleep(20);
  }
  console.log("[" + label + "] killing the receiver after " + formatBytes(checkpointed));
  receiver.kill();
  try {
    await receiver.waitForExit(60000);
  } catch {
    // A killed process may not report an exit code promptly.
  }
  try {
    await sender.waitForExit(60000);
  } catch {
    sender.kill();
  }
  const first = outcome(receiver);
  if (first.status === "ok") {
    report.add("T-03", "receiver restart and resume", "FAIL", "the transfer finished before the kill");
    if (!options.keep) rmSync(root, { recursive: true, force: true });
    return false;
  }

  const retryReceiver = startReceiver(binary, dirs, options, "transfer-recv-restart");
  const retryAddr = await receiverAddress(retryReceiver);
  const retrySender = startSender(binary, dirs, options, paired, retryAddr, source, transferId);
  const retrySendCode = await retrySender.waitForExit(900000);
  const retryRecvCode = await retryReceiver.waitForExit(900000);
  const second = outcome(retryReceiver);
  const sent = retrySender.fields();
  const ok =
    retrySendCode === 0 &&
    retryRecvCode === 0 &&
    second.status === "ok" &&
    second.resumed === "true" &&
    Number(second.missing_bytes_before) < size &&
    sent.root_hash === second.root_hash;
  report.add(
    "T-03",
    "receiver restart and resume",
    ok ? "PASS" : "FAIL",
    "killed after " + formatBytes(checkpointed) + ", resumed " +
      formatBytes(Number(second.missing_bytes_before || 0)) + " of " + formatBytes(size),
  );
  if (!ok) {
    console.log(retrySender.stdout);
    console.log(retryReceiver.stdout);
    console.log(retryReceiver.stderr);
  }
  if (!options.keep) rmSync(root, { recursive: true, force: true });
  return ok;
}

async function runBench(options, report, label = "bench") {
  const { binary, root, dirs, paired } = await prepare(options, label);
  let source = options.file;
  let size;
  if (source) {
    size = statSync(source).size;
  } else {
    size = parseSize(options.size);
    source = join(root, "payload.bin");
    console.log("[" + label + "] generating " + formatBytes(size) + " in " + source);
    if (isWindows) {
      const created = await run("fsutil", ["file", "createnew", source, String(size)]);
      if (created.code !== 0) createFile(source, size);
    } else {
      const created = await run("truncate", ["-s", String(size), source]);
      if (created.code !== 0) createFile(source, size);
    }
  }

  const receiver = startReceiver(binary, dirs, options, "transfer-recv");
  const addr = await receiverAddress(receiver);
  const sampler = startMemorySampler(receiver.child.pid, 200);
  const started = Date.now();
  const sender = startSender(binary, dirs, options, paired, addr, source, options.transferId);
  const sendCode = await sender.waitForExit(3600000);
  const recvCode = await receiver.waitForExit(3600000);
  const elapsed = (Date.now() - started) / 1000;
  const memory = sampler.stop();
  const sent = sender.fields();
  const received = receiver.fields();
  const peakMib = memory.peak / (1024 * 1024);
  const hashOk = sent.root_hash && sent.root_hash === received.root_hash;
  const underCeiling = memory.samples === 0 || peakMib <= 200;
  const ok = sendCode === 0 && recvCode === 0 && hashOk && underCeiling;
  report.add(
    "T-01",
    "large transfer with a memory ceiling",
    ok ? "PASS" : "FAIL",
    formatBytes(size) + " in " + elapsed.toFixed(1) + "s (" +
      (size / (1024 * 1024) / elapsed).toFixed(0) + " MiB/s), prehash " +
      (sender.fieldFrom("hash_ms") || "?") + "ms, receiver peak RSS " + peakMib.toFixed(0) + " MiB" +
      (memory.samples ? " (" + memory.samples + " samples)" : " (no samples)"),
  );
  if (!ok) {
    console.log(sender.stdout);
    console.log(sender.stderr);
    console.log(receiver.stdout);
    console.log(receiver.stderr);
  }
  if (!options.keep) rmSync(root, { recursive: true, force: true });
  return ok;
}

async function runFaults(options, report) {
  console.log("[faults] cargo test -p udp-chaos --test transfer_faults");
  const result = await run(
    "cargo",
    ["test", "-p", "udp-chaos", "--test", "transfer_faults", "--", "--nocapture"],
    { cwd: process.cwd() },
  );
  const output = result.stdout + result.stderr;
  const outcomes = new Map();
  for (const match of output.matchAll(/^test ([a-z0-9_]+) \.\.\. (ok|FAILED|ignored)/gm)) {
    outcomes.set(match[1], match[2]);
  }
  const mapping = [
    ["T-02", "a_short_outage_recovers_without_losing_blocks", "short outage recovers in place"],
    ["T-03", "a_receiver_restart_resumes_from_its_checkpoint", "restart resumes missing blocks"],
    ["T-04", "a_changed_source_is_refused_with_3002", "changed source refused with 3002"],
    ["T-06", "chat_stays_responsive_while_the_link_is_saturated", "chat p95 under load"],
  ];
  const p95 = /T-06: (\d+) messages, p95 ([0-9.]+)(m?s)/.exec(output);
  let ok = result.code === 0;
  for (const [id, test, name] of mapping) {
    const status = outcomes.get(test);
    const passed = status === "ok";
    if (!passed) ok = false;
    let detail = status || "not run";
    if (id === "T-06" && p95) detail = "p95 " + p95[2] + p95[3] + ", " + p95[1] + " messages";
    report.add(id, name, passed ? "PASS" : "FAIL", detail);
  }
  if (!ok) console.log(output.split("\n").slice(-40).join("\n"));
  return ok;
}

async function main() {
  const options = parseOptions(process.argv.slice(2));
  if (options.help) {
    console.log(USAGE);
    return 0;
  }
  const report = new Report();
  let ok = true;
  switch (options.scenario) {
    case "local":
      ok = await runLocal(options, report);
      break;
    case "outage":
      ok = await runOutage(options, report);
      break;
    case "restart":
      ok = await runRestart(options, report);
      break;
    case "bench":
      ok = await runBench(options, report);
      break;
    case "faults":
      ok = await runFaults(options, report);
      break;
    case "matrix": {
      const faults = await runFaults(options, report);
      const local = await runBench(
        { ...options, size: options.size || "128MiB", file: options.file },
        report,
        "bench",
      );
      const outage = await runOutage({ ...options, outage: options.outage || 3 }, report);
      const restart = await runRestart(options, report);
      report.add(
        "T-05",
        "revoked file access keeps the partial",
        "MANUAL",
        "see scripts/acceptance/README.md",
      );
      ok = faults && local && outage && restart;
      break;
    }
    default:
      console.error("unknown scenario: " + options.scenario);
      console.error(USAGE);
      return 2;
  }
  const passed = report.print();
  if (options.json) console.log(JSON.stringify(report.rows, null, 2));
  return passed && ok ? 0 : 1;
}

main()
  .then((code) => process.exit(code))
  .catch((error) => {
    console.error("acceptance failed: " + (error && error.stack ? error.stack : error));
    process.exit(2);
  });
