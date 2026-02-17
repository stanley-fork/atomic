use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::Manager;
use tauri_plugin_shell::ShellExt;

const SIDECAR_PORT: u16 = 44380;
const HEALTH_POLL_INTERVAL_MS: u64 = 100;
const HEALTH_TIMEOUT_MS: u64 = 10_000;

/// Config returned to the frontend so it can connect to the sidecar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalServerConfig {
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    #[serde(rename = "authToken")]
    pub auth_token: String,
}

/// Holds the sidecar child process for cleanup on exit
struct SidecarChild(tauri_plugin_shell::process::CommandChild);

struct SidecarState {
    child: Mutex<Option<SidecarChild>>,
}

#[tauri::command]
fn get_local_server_config(
    config: tauri::State<'_, LocalServerConfig>,
) -> LocalServerConfig {
    config.inner().clone()
}

/// Read or create the local server auth token.
/// This is the ONLY remaining use of atomic-core in the Tauri crate.
fn ensure_local_token(app_data_dir: &std::path::Path, db_path: &std::path::Path) -> String {
    let token_file = app_data_dir.join("local_server_token");

    // Try to read existing token
    if let Ok(token) = std::fs::read_to_string(&token_file) {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return token;
        }
    }

    // Create a new token via atomic-core
    let core = atomic_core::AtomicCore::open_or_create(db_path)
        .expect("Failed to open database for token bootstrap");

    let (_info, raw_token) = core
        .create_api_token("desktop")
        .expect("Failed to create API token");

    std::fs::write(&token_file, &raw_token)
        .expect("Failed to write local server token file");

    raw_token
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data directory");

            std::fs::create_dir_all(&app_data_dir)
                .expect("Failed to create app data directory");

            let db_name = std::env::var("ATOMIC_DB_NAME")
                .map(|name| format!("{}.db", name))
                .unwrap_or_else(|_| "atomic.db".to_string());

            let db_path = app_data_dir.join(&db_name);
            eprintln!("Using database: {:?}", db_path);

            // Bootstrap auth token (only use of atomic-core)
            let auth_token = ensure_local_token(&app_data_dir, &db_path);

            let base_url = format!("http://127.0.0.1:{}", SIDECAR_PORT);
            let config = LocalServerConfig {
                base_url: base_url.clone(),
                auth_token: auth_token.clone(),
            };
            app.manage(config.clone());

            // Check if an Atomic server is already running on the port
            let health_url = format!("{}/health", base_url);
            let already_running = reqwest::blocking::Client::new()
                .get(&health_url)
                .timeout(std::time::Duration::from_millis(500))
                .send()
                .is_ok_and(|r| r.status().is_success());

            if already_running {
                eprintln!("Atomic server already running at {}, reusing it", base_url);
                app.manage(SidecarState {
                    child: Mutex::new(None),
                });
            } else {
                // Spawn atomic-server as a sidecar
                let shell = app.shell();
                let sidecar_cmd = shell
                    .sidecar("atomic-server")
                    .expect("Failed to create sidecar command")
                    .args([
                        "--db-path",
                        db_path.to_str().unwrap(),
                        "serve",
                        "--port",
                        &SIDECAR_PORT.to_string(),
                    ]);

                let (mut rx, child) =
                    sidecar_cmd.spawn().expect("Failed to spawn atomic-server sidecar");

                // Log sidecar output
                tauri::async_runtime::spawn(async move {
                    use tauri_plugin_shell::process::CommandEvent;
                    while let Some(event) = rx.recv().await {
                        match event {
                            CommandEvent::Stdout(line) => {
                                eprintln!("[sidecar stdout] {}", String::from_utf8_lossy(&line));
                            }
                            CommandEvent::Stderr(line) => {
                                eprintln!("[sidecar stderr] {}", String::from_utf8_lossy(&line));
                            }
                            CommandEvent::Terminated(payload) => {
                                eprintln!("[sidecar] terminated: {:?}", payload);
                                break;
                            }
                            CommandEvent::Error(err) => {
                                eprintln!("[sidecar] error: {}", err);
                            }
                            _ => {}
                        }
                    }
                });

                app.manage(SidecarState {
                    child: Mutex::new(Some(SidecarChild(child))),
                });

                // Poll health endpoint until ready
                let start = std::time::Instant::now();
                loop {
                    if start.elapsed().as_millis() as u64 > HEALTH_TIMEOUT_MS {
                        eprintln!("Warning: sidecar health check timed out after {}ms", HEALTH_TIMEOUT_MS);
                        break;
                    }
                    match reqwest::blocking::Client::new()
                        .get(&health_url)
                        .timeout(std::time::Duration::from_millis(500))
                        .send()
                    {
                        Ok(resp) if resp.status().is_success() => {
                            eprintln!("Sidecar ready at {} ({}ms)", base_url, start.elapsed().as_millis());
                            break;
                        }
                        _ => {
                            std::thread::sleep(std::time::Duration::from_millis(HEALTH_POLL_INTERVAL_MS));
                        }
                    }
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_local_server_config,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // Kill sidecar on app exit
                if let Some(state) = app.try_state::<SidecarState>() {
                    if let Ok(mut child_opt) = state.child.lock() {
                        if let Some(SidecarChild(child)) = child_opt.take() {
                            eprintln!("Shutting down sidecar...");
                            let _ = child.kill();
                        }
                    }
                }
            }
        });
}
