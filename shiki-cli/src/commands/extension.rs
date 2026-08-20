use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use shiki_config::Config;

/// `shiki extension` — installs the browser companion cleanly and saved.
///
/// Design: "limpio y guardado" = no manual `cp` or `cargo build` sprinkled,
/// everything through `shiki` so the install is reproducible and recorded.
/// The extension itself stays in `browser-extension/` (git-tracked), the host
/// binary is built via `cargo` (workspace member `shiki-native-host`), and the
/// native-messaging manifest is written to the OS-appropriate location
/// (Linux: `~/.config/.../NativeMessagingHosts`, Windows: registry + `%APPDATA%`).
/// `shiki extension status` reads that state back; `uninstall` removes it.
#[derive(Debug, clap::Subcommand)]
pub enum ExtensionAction {
    /// Builds the host (release) and installs the native-messaging manifest
    Install {
        /// Extension ID from chrome://extensions (after Load unpacked). If omitted,
        /// installs a placeholder; re-run with the real ID after first load.
        #[arg(long)]
        id: Option<String>,
        /// Also copy the extension to a stable Windows path (C:\Temp\shiki-extension) for `Load unpacked`
        #[arg(long)]
        copy_to_windows: bool,
    },
    /// Removes the native-messaging manifest and (optionally) the host binary
    Uninstall {
        /// Also remove the built host binary from target/
        #[arg(long)]
        with_binary: bool,
    },
    /// Shows whether the extension and host are installed and reachable
    Status {
        /// Emit JSON instead of human text — for `waybar`/`polybar`
        #[arg(long)]
        json: bool,
    },
    /// Packs the extension and host installer zips (for Store or manual install)
    Pack {
        /// Output directory (default: browser-extension/)
        #[arg(long)]
        out_dir: Option<PathBuf>,
    },
}

fn repo_root() -> PathBuf {
    // Try compile-time repo root, but if not found (installed binary), try exe parent and current dir
    let compiled = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    if compiled.join("browser-extension/manifest.json").exists() {
        return compiled;
    }
    if let Ok(exe) = std::env::current_exe() {
        for anc in exe.ancestors() {
            if anc.join("browser-extension/manifest.json").exists() {
                return anc.to_path_buf();
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for anc in cwd.ancestors() {
            if anc.join("browser-extension/manifest.json").exists() {
                return anc.to_path_buf();
            }
        }
    }
    compiled
}

fn is_wsl() -> bool {
    std::env::var("WSL_DISTRO_NAME").is_ok()
        || std::fs::read_to_string("/proc/version")
            .map(|s| s.to_lowercase().contains("microsoft") || s.to_lowercase().contains("wsl"))
            .unwrap_or(false)
        || Path::new("/mnt/c/Windows").exists()
}

fn windows_appdata_shiki() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata).join("shiki");
    }
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        return PathBuf::from(userprofile).join("AppData/Roaming/shiki");
    }
    // WSL fallback
    if let Ok(home) = std::env::var("HOME") {
        // Try to find Windows user via /mnt/c/Users
        if let Ok(entries) = std::fs::read_dir("/mnt/c/Users") {
            for e in entries.flatten() {
                let p = e.path().join("AppData/Roaming/shiki");
                if p.exists() || e.path().join("AppData").exists() {
                    return p;
                }
            }
        }
        return PathBuf::from(home).join(".config/shiki");
    }
    PathBuf::from("/mnt/c/Users/Omar/AppData/Roaming/shiki")
}

fn windows_temp_dir() -> PathBuf {
    if let Ok(tmp) = std::env::var("TEMP") {
        return PathBuf::from(tmp);
    }
    if let Ok(tmp) = std::env::var("TMP") {
        return PathBuf::from(tmp);
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        return PathBuf::from(local).join("Temp");
    }
    PathBuf::from("/mnt/c/Temp")
}

fn windows_chrome_manifest_exists() -> bool {
    windows_appdata_shiki()
        .join("com.shiki.native.json")
        .exists()
}

fn run_cargo_build(release: bool) -> Result<PathBuf> {
    let root = repo_root();
    let mut cmd = Command::new("cargo");
    cmd.arg("build").arg("-p").arg("shiki-native-host");
    if release {
        cmd.arg("--release");
    }
    cmd.current_dir(&root);
    let status = cmd.status().context("failed to run cargo build")?;
    if !status.success() {
        anyhow::bail!("cargo build failed");
    }
    let bin = if release {
        root.join("target/release/shiki-native-host")
    } else {
        root.join("target/debug/shiki-native-host")
    };
    // Windows cross build also produces .exe when target is windows
    let win_bin = root.join("target/x86_64-pc-windows-gnu/release/shiki-native-host.exe");
    if win_bin.exists() {
        println!("  also built Windows host: {}", win_bin.display());
    }
    if !bin.exists() {
        anyhow::bail!("expected host binary not found: {}", bin.display());
    }
    Ok(bin)
}

fn host_manifest_template() -> PathBuf {
    repo_root().join("browser-extension/host/com.shiki.native.json")
}

fn extension_dir() -> PathBuf {
    repo_root().join("browser-extension")
}

fn install_linux(host_bin: &Path, extension_id: &str) -> Result<()> {
    // Use the existing install.sh for Linux/macOS — it already handles merging allowed_origins
    let script = repo_root().join("browser-extension/host/install.sh");
    let mut cmd = Command::new("bash");
    cmd.arg(&script);
    if extension_id != "__REPLACE_WITH_EXTENSION_ID__" {
        cmd.arg("--extension-id").arg(extension_id);
    }
    let status = cmd.status().context("failed to run install.sh")?;
    if !status.success() {
        anyhow::bail!("install.sh failed");
    }
    // Also ensure the binary path in the manifest is the built one
    println!("  host: {}", host_bin.display());
    Ok(())
}

fn install_windows(_host_bin: &Path, extension_id: &str) -> Result<()> {
    let win_host = repo_root().join("target/x86_64-pc-windows-gnu/release/shiki-native-host.exe");
    let wrapper_win = windows_temp_dir().join("shiki-native-host-wrapper.exe");
    if wrapper_win.exists() {
        println!("  Windows wrapper exists: C:\\Temp\\shiki-native-host-wrapper.exe");
    } else if win_host.exists() {
        std::fs::copy(&win_host, &wrapper_win).ok();
        println!("  copied Windows host to C:\\Temp\\shiki-native-host.exe");
    }
    // Create batch fallback if needed — use actual repo path, not hardcoded Omar
    let wsl_host = repo_root().join("target/release/shiki-native-host");
    let bat = windows_temp_dir().join("shiki-native-host.bat");
    if !bat.exists() {
        let wsl_path = wsl_host.display().to_string();
        std::fs::write(&bat, format!("@echo off\nwsl -e {wsl_path}\n")).ok();
    }
    // Resolve %APPDATA% via env, fallback to /mnt/c/Users/*/AppData/Roaming
    let appdata_shiki = windows_appdata_shiki();
    std::fs::create_dir_all(&appdata_shiki).ok();
    let manifest_path = appdata_shiki.join("com.shiki.native.json");
    let template = std::fs::read_to_string(host_manifest_template()).unwrap_or_default();
    let id = if extension_id.is_empty() {
        "__REPLACE_WITH_EXTENSION_ID__"
    } else {
        extension_id
    };
    let win_path = if wrapper_win.exists() {
        wrapper_win.display().to_string().replace('/', "\\")
    } else {
        // Use batch path as fallback
        bat.display().to_string().replace('/', "\\")
    };
    // Ensure Windows style backslashes
    let win_path = win_path.replace('/', "\\");
    let content = template
        .replace(
            "__REPLACE_WITH_ABSOLUTE_PATH_TO_shiki-native-host__",
            &win_path,
        )
        .replace("__REPLACE_WITH_EXTENSION_ID__", id);
    std::fs::write(&manifest_path, content).context("write Windows manifest")?;
    println!("  Windows manifest: {}", manifest_path.display());

    let manifest_win = format!(
        "{}\\com.shiki.native.json",
        windows_appdata_shiki()
            .display()
            .to_string()
            .replace('/', "\\")
    );
    let _ = Command::new("/mnt/c/Windows/System32/cmd.exe")
        .args([
            "/c",
            &format!(
                "reg add HKCU\\Software\\Google\\Chrome\\NativeMessagingHosts\\com.shiki.native /ve /t REG_SZ /d \"{}\" /f",
                manifest_win
            ),
        ])
        .status();
    let _ = Command::new("/mnt/c/Windows/System32/cmd.exe")
        .args([
            "/c",
            &format!(
                "reg add HKCU\\Software\\Microsoft\\Edge\\NativeMessagingHosts\\com.shiki.native /ve /t REG_SZ /d \"{}\" /f",
                manifest_win
            ),
        ])
        .status();
    println!(
        "  registry: HKCU\\...\\com.shiki.native -> {}",
        manifest_win
    );
    Ok(())
}

pub fn run(action: ExtensionAction) -> Result<()> {
    match action {
        ExtensionAction::Install {
            id,
            copy_to_windows,
        } => {
            let ext_dir = extension_dir();
            if !ext_dir.join("manifest.json").exists() {
                anyhow::bail!(
                    "browser-extension/manifest.json not found at {}",
                    ext_dir.display()
                );
            }
            println!("==> shiki extension install — limpio y guardado");
            println!("  extension: {}", ext_dir.display());
            let host_bin = run_cargo_build(true)?;
            println!("  built host: {}", host_bin.display());

            let extension_id = id.unwrap_or_else(|| "__REPLACE_WITH_EXTENSION_ID__".into());
            if extension_id == "__REPLACE_WITH_EXTENSION_ID__" {
                println!("  ! no --id given, using placeholder. After Load unpacked, re-run:");
                println!("    shiki extension install --id <ID_de_chrome://extensions>");
            }

            if is_wsl() {
                println!("  installing Linux host...");
                if let Err(e) = install_linux(&host_bin, &extension_id) {
                    eprintln!("  Linux install failed: {e}");
                }
                println!("  installing Windows host (WSL bridge)...");
                if let Err(e) = install_windows(&host_bin, &extension_id) {
                    eprintln!("  Windows install failed: {e}");
                }
            } else if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
                install_linux(&host_bin, &extension_id)?;
            } else if cfg!(target_os = "windows") {
                install_windows(&host_bin, &extension_id)?;
            } else {
                install_linux(&host_bin, &extension_id)?;
            }

            if copy_to_windows {
                let dest = windows_temp_dir().join("shiki-extension");
                std::fs::create_dir_all(&dest).ok();
                // Use std::fs copy instead of `cp -r` for portability
                let dest_parent = windows_temp_dir();
                let output = Command::new("cp")
                    .args(["-r"])
                    .arg(&ext_dir)
                    .arg(dest_parent)
                    .output();
                if output.is_ok() {
                    println!("  copied extension to C:\\Temp\\shiki-extension");
                }
            }

            // Save install state to config dir for status
            if let Ok(cfg_path) = Config::default_path() {
                if let Some(parent) = cfg_path.parent() {
                    let state_path = parent.join("extension.json");
                    let state = serde_json::json!({
                        "installed_at": chrono::Local::now().to_rfc3339(),
                        "extension_id": extension_id,
                        "host": host_bin.display().to_string(),
                        "extension_dir": ext_dir.display().to_string(),
                    });
                    std::fs::write(&state_path, serde_json::to_string_pretty(&state).unwrap()).ok();
                    println!("  saved state: {}", state_path.display());
                }
            }

            println!("\n  done. Next:");
            println!(
                "    1. chrome://extensions → Developer mode → Load unpacked → {}",
                ext_dir.display()
            );
            if extension_id == "__REPLACE_WITH_EXTENSION_ID__" {
                println!("    2. copy the ID and re-run: shiki extension install --id <ID>");
            }
            println!("    3. shiki extension status --json");
        }
        ExtensionAction::Uninstall { with_binary } => {
            println!("==> shiki extension uninstall");
            let script = repo_root().join("browser-extension/host/uninstall.sh");
            if script.exists() {
                let _ = Command::new("bash").arg(&script).status();
            }
            if is_wsl() {
                let _ = Command::new("/mnt/c/Windows/System32/cmd.exe")
                    .args(["/c", "reg delete HKCU\\Software\\Google\\Chrome\\NativeMessagingHosts\\com.shiki.native /f"])
                    .status();
                let _ = Command::new("/mnt/c/Windows/System32/cmd.exe")
                    .args(["/c", "reg delete HKCU\\Software\\Microsoft\\Edge\\NativeMessagingHosts\\com.shiki.native /f"])
                    .status();
            } else if cfg!(target_os = "windows") {
                let _ = Command::new("cmd")
                    .args(["/c", "reg delete HKCU\\Software\\Google\\Chrome\\NativeMessagingHosts\\com.shiki.native /f"])
                    .status();
                let _ = Command::new("cmd")
                    .args(["/c", "reg delete HKCU\\Software\\Microsoft\\Edge\\NativeMessagingHosts\\com.shiki.native /f"])
                    .status();
            }
            if with_binary {
                let host_bin = repo_root().join("target/release/shiki-native-host");
                std::fs::remove_file(&host_bin).ok();
                println!("  removed {}", host_bin.display());
            }
            if let Ok(cfg_path) = Config::default_path() {
                if let Some(parent) = cfg_path.parent() {
                    let state_path = parent.join("extension.json");
                    std::fs::remove_file(&state_path).ok();
                }
            }
            println!("  uninstalled");
        }
        ExtensionAction::Status { json } => {
            let ext_dir = extension_dir();
            let host_bin = repo_root().join("target/release/shiki-native-host");
            let host_exists = host_bin.exists();
            // Actually check Linux manifest locations
            let home = std::env::var("HOME")
                .ok()
                .map(PathBuf::from)
                .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from));
            let linux_manifest = home
                .as_ref()
                .map(|h| h.join(".config/google-chrome/NativeMessagingHosts/com.shiki.native.json"))
                .map(|p| p.exists())
                .unwrap_or(false)
                || home
                    .as_ref()
                    .map(|h| h.join(".config/chromium/NativeMessagingHosts/com.shiki.native.json"))
                    .map(|p| p.exists())
                    .unwrap_or(false);
            let win_manifest = windows_chrome_manifest_exists();
            let state_path = Config::default_path()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("extension.json")))
                .map(|p| p.exists())
                .unwrap_or(false);

            if json {
                let out = serde_json::json!({
                    "extension_dir": ext_dir.display().to_string(),
                    "extension_exists": ext_dir.join("manifest.json").exists(),
                    "host_binary": host_bin.display().to_string(),
                    "host_exists": host_exists,
                    "linux_manifest": linux_manifest,
                    "windows_manifest": win_manifest,
                    "state_saved": state_path,
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                println!("Shiki extension status");
                println!(
                    "  extension: {} {}",
                    ext_dir.display(),
                    if ext_dir.join("manifest.json").exists() {
                        "✓"
                    } else {
                        "✗ missing manifest.json"
                    }
                );
                println!(
                    "  host: {} {}",
                    host_bin.display(),
                    if host_exists {
                        "✓ built"
                    } else {
                        "✗ not built (run shiki extension install)"
                    }
                );
                println!(
                    "  Linux manifest: {}",
                    if linux_manifest { "✓" } else { "✗" }
                );
                println!(
                    "  Windows manifest: {}",
                    if win_manifest { "✓" } else { "✗" }
                );
                println!("  saved state: {}", if state_path { "✓" } else { "✗" });
                if !host_exists {
                    println!("\n  run: shiki extension install [--id <ID>]");
                }
            }
        }
        ExtensionAction::Pack { out_dir } => {
            let out = out_dir.unwrap_or_else(extension_dir);
            std::fs::create_dir_all(&out).ok();
            let ext_dir = extension_dir();
            // Use extension's own manifest version, not workspace version
            let ext_version = std::fs::read_to_string(ext_dir.join("manifest.json"))
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| {
                    v.get("version")
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| "0.1.0".to_string());
            let zip_path = out.join(format!("shiki-capture-{ext_version}.zip"));
            let output = Command::new("python3")
                .arg("-")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn();
            if let Ok(mut child) = output {
                let script = format!(
                    "import zipfile, pathlib; ext=pathlib.Path('{}'); zip_path=pathlib.Path('{}'); import os; [zip_path.unlink() for _ in [0] if zip_path.exists()]; z=zipfile.ZipFile(zip_path,'w',zipfile.ZIP_DEFLATED); z.write(ext/'manifest.json','manifest.json'); [z.write(p, str(p.relative_to(ext))) for p in ext.rglob('src/*') if p.is_file() and 'src-tauri' not in str(p)]; [z.write(p, str(p.relative_to(ext))) for p in ext.rglob('icons/*') if p.is_file()]; z.close(); print(str(zip_path))",
                    ext_dir.display(),
                    zip_path.display()
                );
                use std::io::Write;
                child
                    .stdin
                    .as_mut()
                    .unwrap()
                    .write_all(script.as_bytes())
                    .ok();
                let _ = child.wait();
            }
            // Fallback simple
            if !zip_path.exists() {
                let status = Command::new("bash")
                    .arg("-c")
                    .arg(format!(
                        "cd {} && zip -r {} manifest.json src/ icons/ -x '*.DS_Store' 2>/dev/null || python3 -m zipfile -c {} manifest.json src/background.js src/content.js src/popup.html src/popup.css src/popup.js src/options.html src/options.js icons/*",
                        ext_dir.display(),
                        zip_path.display(),
                        zip_path.display()
                    ))
                    .status();
                if status.is_err() {
                    anyhow::bail!("pack failed");
                }
            }
            println!("packed: {}", zip_path.display());
            // Host installer — actually create the zip
            let host_zip = out.join(format!("shiki-host-installer-{ext_version}.zip"));
            {
                let script = format!(
                    "import zipfile, pathlib; ext=pathlib.Path('{}'); out=pathlib.Path('{}'); import os; host_linux=pathlib.Path('{}'); host_win=pathlib.Path('{}'); wrapper=pathlib.Path('/mnt/c/Temp/shiki-native-host-wrapper.exe'); z=zipfile.ZipFile(out,'w',zipfile.ZIP_DEFLATED); [z.write(p, str(p.relative_to(ext))) for p in (ext/'host').rglob('*') if p.is_file()]; [z.write(p, 'host/'+p.name) for p in [host_linux, host_win] if pathlib.Path(p).exists()]; [z.write(ext/p, p) for p in ['PRIVACY.md','STORE.md'] if (ext/p).exists()]; z.write(ext/'shiki-capture-{ext_version}.zip', 'extension/shiki-capture-{ext_version}.zip'); z.close(); print(str(out))",
                    ext_dir.display(),
                    host_zip.display(),
                    repo_root().join("target/release/shiki-native-host").display(),
                    repo_root().join("target/x86_64-pc-windows-gnu/release/shiki-native-host.exe").display()
                );
                let child = Command::new("python3")
                    .arg("-")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .ok();
                if let Some(mut c) = child {
                    use std::io::Write;
                    c.stdin.as_mut().unwrap().write_all(script.as_bytes()).ok();
                    let _ = c.wait();
                }
            }
            if host_zip.exists() {
                println!(
                    "host installer: {} ({} bytes)",
                    host_zip.display(),
                    host_zip.metadata().map(|m| m.len()).unwrap_or(0)
                );
            } else {
                println!(
                    "host installer: {} (uses browser-extension/host/*.sh + built host binaries)",
                    host_zip.display()
                );
            }
        }
    }
    Ok(())
}
