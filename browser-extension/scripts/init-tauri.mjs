#!/usr/bin/env node
// Scaffolds a minimal Tauri harness to preview the extension popup in a desktop window
// without Chrome. Useful for "npm run dev:tauri" quick iteration on popup.html/css/js.
// - Creates src-tauri/ if missing, Tauri 2.x style, loading browser-extension/src/popup.html
// - Requires `cargo` + `npm`. First run will install @tauri/cli.
import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";

const extDir = path.resolve(import.meta.dirname, "..");
const tauriDir = path.join(extDir, "src-tauri");

if (fs.existsSync(tauriDir)) {
  console.log(`src-tauri already exists at ${tauriDir}, skipping init.`);
  console.log(`Run: npm run dev:tauri`);
  process.exit(0);
}

console.log(`Scaffolding Tauri harness at ${tauriDir}...`);
fs.mkdirSync(tauriDir, { recursive: true });

// Minimal tauri.conf.json that points to popup.html
const conf = {
  build: { beforeDevCommand: "", devUrl: "http://localhost:1420", beforeBuildCommand: "", frontendDist: "../src" },
  app: { windows: [{ title: "Shiki Capture (Tauri preview)", width: 400, height: 640, resizable: true }], security: { csp: null } },
  bundle: { active: true, targets: "all", icon: ["icons/icon128.png"] },
  productName: "shiki-capture-tauri",
  version: "0.1.0",
  identifier: "com.shiki.capture.tauri",
};
fs.writeFileSync(path.join(tauriDir, "tauri.conf.json"), JSON.stringify(conf, null, 2));

// Cargo.toml for tauri
fs.writeFileSync(path.join(tauriDir, "Cargo.toml"), `
[package]
name = "shiki-capture-tauri"
version = "0.1.0"
edition = "2021"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
serde_json = "1"
serde = { version = "1", features = ["derive"] }
tauri = { version = "2", features = [] }
`.trim() + "\n");

fs.writeFileSync(path.join(tauriDir, "build.rs"), `fn main() { tauri_build::build() }\n`);

// Add a simple main that serves src/ via Tauri
fs.mkdirSync(path.join(tauriDir, "src"), { recursive: true });
fs.writeFileSync(path.join(tauriDir, "src", "main.rs"), `
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri");
}
`.trim() + "\n");

console.log(`Done. Now run:`);
console.log(`  npm install -D @tauri/cli`);
console.log(`  npm run dev:tauri`);
console.log(`Note: Tauri preview loads popup.html but native messaging (Chrome host) won't work there — use it only for CSS/layout iteration. For real capture, use dev:chrome.`);
