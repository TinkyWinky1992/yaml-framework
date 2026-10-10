#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Desktop shell: runs the AI-IDE server inside the app process and points the React UI at it.

mod settings;

use ai_ide_server::serve;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex};

/// Folders copied from the bundled sample into the first workspace.
const SAMPLE_DIRS: [&str; 3] = ["specs", "src", "tests"];
const OPEN_FOLDER_ID: &str = "open_folder";

/// The embedded server. It keeps one port for the whole session, so the UI only needs its address once.
struct EmbeddedServer {
    port: u16,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

fn spawn_server(listener: TcpListener, root: PathBuf) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = serve(root, listener, std::future::pending::<()>()).await {
            eprintln!("AI-IDE server stopped: {e}");
        }
    })
}

/// Rebinds the same port after the previous server released it. Retries briefly because
/// Windows can take a moment to free a socket.
async fn bind_with_retry(port: u16) -> Result<TcpListener, String> {
    let addr = format!("127.0.0.1:{port}");
    let mut last_error = String::new();
    for _ in 0..30 {
        match TcpListener::bind(&addr).await {
            Ok(listener) => return Ok(listener),
            Err(e) => {
                last_error = e.to_string();
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
    Err(format!("Could not restart the IDE server on port {port}: {last_error}"))
}

/// Sends the provider the user picked last time. The server starts on its default provider.
async fn apply_saved_provider(app: &AppHandle, port: u16) {
    let Some(id) = settings::load(app).active_provider else { return };
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/api/providers");
    for _ in 0..20 {
        match client.post(&url).json(&serde_json::json!({ "id": id })).send().await {
            Ok(res) => {
                if !res.status().is_success() {
                    eprintln!("Saved AI provider '{id}' is not available ({})", res.status());
                }
                return;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(150)).await,
        }
    }
}

/// The folder used when none was opened yet. The bundled sample is copied there once.
fn default_workspace(app: &AppHandle) -> Result<PathBuf, String> {
    let target = app.path().app_data_dir().map_err(|e| e.to_string())?.join("sample-project");
    if !target.exists() {
        let source = sample_source(app)?;
        for dir in SAMPLE_DIRS {
            copy_dir(&source.join(dir), &target.join(dir))?;
        }
    }
    Ok(target)
}

fn sample_source(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(resources) = app.path().resource_dir() {
        let bundled = resources.join("sample");
        if bundled.join("specs").is_dir() {
            return Ok(bundled);
        }
    }
    // `pnpm tauri dev` runs from the repository, where the sample sits next to src-tauri.
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    if repo.join("specs").is_dir() {
        return Ok(repo);
    }
    Err("The bundled sample project could not be found".into())
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    if !from.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let dest = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Stops the running server and starts one on the same port for `root`.
async fn restart_server(app: &AppHandle, root: PathBuf) -> Result<(), String> {
    let server = app.state::<EmbeddedServer>();
    let mut task = server.task.lock().await;
    if let Some(handle) = task.take() {
        handle.abort();
        let _ = handle.await;
    }
    let listener = bind_with_retry(server.port).await?;
    *task = Some(spawn_server(listener, root));
    drop(task);
    apply_saved_provider(app, server.port).await;
    Ok(())
}

/// Shows the folder picker. On a choice, switches the server to that folder and returns it.
async fn pick_and_open_folder(app: AppHandle) -> Result<Option<PathBuf>, String> {
    let (tx, rx) = oneshot::channel();
    app.dialog().file().pick_folder(move |picked| {
        let _ = tx.send(picked);
    });
    let Some(picked) = rx.await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let root = picked.into_path().map_err(|e| e.to_string())?;
    restart_server(&app, root.clone()).await?;
    settings::remember_folder(&app, &root)?;
    Ok(Some(root))
}

#[tauri::command]
fn get_api_base(server: State<'_, EmbeddedServer>) -> String {
    format!("http://127.0.0.1:{}", server.port)
}

/// Returns the chosen folder, or `None` if the user cancelled. The UI reloads after a choice.
#[tauri::command]
async fn open_folder(app: AppHandle) -> Result<Option<String>, String> {
    Ok(pick_and_open_folder(app).await?.map(|p| p.display().to_string()))
}

#[tauri::command]
fn recent_folders(app: AppHandle) -> Vec<String> {
    settings::load(&app).recent_folders
}

#[tauri::command]
fn remember_provider(app: AppHandle, id: String) -> Result<(), String> {
    settings::remember_provider(&app, &id)
}

#[tauri::command]
fn set_api_key(name: String, value: String) -> Result<(), String> {
    settings::store_key(&name, value.trim())
}

#[tauri::command]
fn api_key_status() -> Vec<(String, bool)> {
    settings::KEY_NAMES
        .iter()
        .map(|name| (name.to_string(), settings::key_present(name)))
        .collect()
}

fn build_menu(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id(OPEN_FOLDER_ID, "Open Folder…")
        .accelerator("CmdOrCtrl+O")
        .build(app)?;
    let file = SubmenuBuilder::new(app, "File").item(&open).separator().quit().build()?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;
    let menu = MenuBuilder::new(app).item(&file).item(&edit).build()?;
    app.set_menu(menu)?;
    Ok(())
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    settings::load_keys_into_env();

    let saved = settings::load(&handle);
    let root = match saved.recent_folders.iter().map(PathBuf::from).find(|p| p.is_dir()) {
        Some(root) => root,
        None => default_workspace(&handle)?,
    };

    // Bind before the window opens, so the UI never sees a closed port. Port 0 lets the OS pick a free one.
    let listener = tauri::async_runtime::block_on(TcpListener::bind("127.0.0.1:0"))?;
    let port = listener.local_addr()?.port();
    let task = spawn_server(listener, root);
    app.manage(EmbeddedServer {
        port,
        task: Mutex::new(Some(task)),
    });

    let apply_handle = handle.clone();
    tauri::async_runtime::spawn(async move { apply_saved_provider(&apply_handle, port).await });

    build_menu(&handle)?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(setup)
        .on_menu_event(|app, event| {
            if event.id() == OPEN_FOLDER_ID {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    match pick_and_open_folder(app.clone()).await {
                        Ok(Some(_)) => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.eval("window.location.reload()");
                            }
                        }
                        Ok(None) => {}
                        Err(e) => eprintln!("Open Folder failed: {e}"),
                    }
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_api_base,
            open_folder,
            recent_folders,
            remember_provider,
            set_api_key,
            api_key_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running AI Spec IDE");
}
