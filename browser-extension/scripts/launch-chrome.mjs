#!/usr/bin/env node
// Launches Chromium with Shiki extension loaded, using an isolated profile
// so it doesn't conflict with your main browser instance.
// - Builds/uses existing shiki-native-host (caller should have run host:install once)
// - Starts chromium with --load-extension and --user-data-dir=/tmp/shiki-chrome-test
// - Prints extension ID after 2s by reading Preferences
import { spawn, execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";

const isHeadless = process.argv.includes("--headless");
const extensionDir = path.resolve(import.meta.dirname, "..");
const profileDir = "/tmp/shiki-chrome-test";
const hostBin = path.resolve(extensionDir, "../target/release/shiki-native-host");

function chromiumBin() {
  for (const cand of ["chromium", "chromium-browser", "google-chrome", "google-chrome-stable"]) {
    try {
      execSync(`which ${cand}`, { stdio: "ignore" });
      return cand;
    } catch {}
  }
  return "chromium";
}

function ensureProfile() {
  fs.mkdirSync(profileDir, { recursive: true });
}

function launch() {
  const bin = chromiumBin();
  ensureProfile();
  const args = [
    `--user-data-dir=${profileDir}`,
    `--load-extension=${extensionDir}`,
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-features=Translate",
    // keep native messaging working
    "--enable-features=NativeMessaging",
    // open a test page with instructions
    "about:blank",
  ];
  if (isHeadless) {
    args.push("--headless=new", "--disable-gpu", "--no-sandbox", "--dump-dom");
  } else {
    // WSL DISPLAY is :0, ensure we pass through
    if (process.env.DISPLAY) args.push(`--display=${process.env.DISPLAY}`);
  }

  console.log(`\n==> Launching ${bin} with extension: ${extensionDir}`);
  console.log(`    profile: ${profileDir}`);
  console.log(`    host bin: ${hostBin} ${fs.existsSync(hostBin) ? "(found)" : "(NOT built yet — run npm run host:build)"}`);
  console.log(`    args: ${args.join(" ")}\n`);

  if (isHeadless) {
    // just test that chrome can start
    const proc = spawn(bin, args, { stdio: "inherit" });
    proc.on("close", (code) => process.exit(code ?? 0));
    return;
  }

  // detached so it keeps running after this script exits
  const proc = spawn(bin, args, {
    detached: true,
    stdio: "ignore",
    env: { ...process.env },
  });
  proc.unref();

  console.log(`✔ Chromium launched (pid ${proc.pid}), detached.`);
  console.log(`  - Extension popup: click the puzzle icon -> Shiki Capture`);
  console.log(`  - chrome://extensions should show "Shiki Capture 0.1.0" (ID will be assigned)`);
  console.log(`  - To fix native host after first load:`);
  console.log(`      1) Copy the Extension ID from chrome://extensions`);
  console.log(`      2) Run: npm run host:install -- --extension-id <ID>`);
  console.log(`         or:  ./host/install.sh --extension-id <ID>`);
  console.log(`      3) Then click Reload on the extension`);
  console.log(`  - Popup should show "daemon: on/off" — if it says "host not installed", the ID step is missing.`);
  console.log(`  - Logs: chrome://extensions -> service worker -> Inspect views`);
  console.log(`  - To kill test profile: pkill chromium; rm -rf ${profileDir}`);

  // Try to read extension ID after a short delay
  setTimeout(() => {
    try {
      const prefsPath = path.join(profileDir, "Default", "Preferences");
      if (!fs.existsSync(prefsPath)) {
        console.log(`\n(Preferences not yet written — open chrome://extensions to see ID)`);
        return;
      }
      const prefs = JSON.parse(fs.readFileSync(prefsPath, "utf8"));
      // Extensions are under extensions.settings — keys are IDs
      const settings = prefs?.extensions?.settings || {};
      let found = null;
      for (const [id, meta] of Object.entries(settings)) {
        const loc = meta?.manifest?.name || "";
        const pathHint = meta?.path || "";
        if (loc.includes("Shiki") || pathHint.includes("browser-extension") || meta?.manifest?.description?.includes("Shiki")) {
          found = { id, loc, pathHint };
          break;
        }
      }
      if (found) {
        console.log(`\n✔ Detected extension ID: ${found.id}`);
        console.log(`  Run now: ./host/install.sh --extension-id ${found.id}`);
        // auto-patch if we found it
        try {
          execSync(`bash ./host/install.sh --extension-id ${found.id}`, {
            cwd: extensionDir,
            stdio: "inherit",
          });
          console.log(`✔ Host manifest auto-patched for ${found.id}. Reload extension in chrome://extensions.`);
        } catch (e) {
          console.log(`  Auto-patch failed, run manually.`);
        }
      } else {
        console.log(`\n(Extension ID not yet in Preferences — try again in 2s after extension loads. Keys: ${Object.keys(settings).slice(0,3).join(", ") || "none"})`);
      }
    } catch (e) {
      console.log(`(Could not read Preferences: ${e.message})`);
    }
  }, 4000);
}

launch();
