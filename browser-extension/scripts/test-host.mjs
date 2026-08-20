#!/usr/bin/env node
// Quick test for shiki-native-host without Chrome
// - Builds host if needed, then sends native messaging framed JSON via stdin
// - Supports --isolated to use temp XDG dirs (no touching real notebooks)
import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const isolated = process.argv.includes("--isolated");
let tmpDir = null;
let env = { ...process.env };
if (isolated) {
  tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "shiki-host-test-"));
  const configHome = path.join(tmpDir, "config");
  const dataHome = path.join(tmpDir, "data");
  fs.mkdirSync(configHome, { recursive: true });
  fs.mkdirSync(dataHome, { recursive: true });
  env.XDG_CONFIG_HOME = configHome;
  env.XDG_DATA_HOME = dataHome;
  console.log(`Using isolated XDG: ${tmpDir}`);
}

const hostBin = path.resolve(import.meta.dirname, "../../target/debug/shiki-native-host");
const hostBinRelease = path.resolve(import.meta.dirname, "../../target/release/shiki-native-host");
const bin = fs.existsSync(hostBin) ? hostBin : hostBinRelease;
if (!fs.existsSync(bin)) {
  console.error(`Host not built: ${bin}\nRun: cargo build -p shiki-native-host`);
  process.exit(1);
}
console.log(`Using host: ${bin}`);

function send(messages) {
  return new Promise((resolve, reject) => {
    const proc = spawn(bin, [], { env, stdio: ["pipe", "pipe", "pipe"] });
    let outBuf = Buffer.alloc(0);
    let responses = [];
    proc.stdout.on("data", (chunk) => {
      outBuf = Buffer.concat([outBuf, chunk]);
      while (outBuf.length >= 4) {
        const len = outBuf.readUInt32LE(0);
        if (outBuf.length < 4 + len) break;
        const body = outBuf.subarray(4, 4 + len);
        outBuf = outBuf.subarray(4 + len);
        try {
          responses.push(JSON.parse(body.toString()));
        } catch (e) {
          console.error("bad json", body.toString());
        }
        if (responses.length === messages.length) {
          proc.kill();
          resolve(responses);
        }
      }
    });
    proc.stderr.on("data", (d) => process.stderr.write(d));
    proc.on("error", reject);
    proc.on("close", () => resolve(responses));

    for (const m of messages) {
      const data = Buffer.from(JSON.stringify(m));
      const hdr = Buffer.alloc(4);
      hdr.writeUInt32LE(data.length, 0);
      proc.stdin.write(hdr);
      proc.stdin.write(data);
    }
    // don't close immediately, let host reply then we kill
    setTimeout(() => {
      if (responses.length < messages.length) {
        console.log(`\nOnly ${responses.length}/${messages.length} replies, closing...`);
        proc.kill();
        resolve(responses);
      }
    }, 3000);
  });
}

const msgs = [
  { action: "ping" },
  { action: "list_notebooks" },
  { action: "capture", text: "test from npm host:test — " + new Date().toISOString(), tags: ["browser","test"] },
  { action: "list_folders", notebook: "personal" },
  { action: "capture", text: "daily test", daily: true },
];

console.log(`Sending ${msgs.length} messages...`);
const res = await send(msgs);
for (let i = 0; i < res.length; i++) {
  console.log(`\n--- ${msgs[i].action} ->`);
  console.log(JSON.stringify(res[i], null, 2));
}
if (res.length < msgs.length) {
  console.log(`\n⚠ Only ${res.length} responses. Host may have crashed or blocked.`);
  process.exit(1);
}
const fails = res.filter(r => r.ok === false);
if (fails.length) {
  console.log(`\n⚠ ${fails.length} failed:`, fails.map(f=>f.error).join("; "));
  process.exit(1);
}
console.log(`\n✔ All ${res.length} host calls ok. ${isolated ? `Data in ${tmpDir}` : `Check real notebooks in ${env.XDG_DATA_HOME || "~/.local/share/shiki"}`}`);

// cleanup isolated
if (tmpDir) {
  // keep for inspection? remove after 5s
  console.log(`(isolated dir kept at ${tmpDir} — rm -rf it when done)`);
}
