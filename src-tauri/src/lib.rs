use std::sync::Mutex;
use tauri::Emitter;
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, FilePath, MessageDialogButtons};

#[derive(serde::Serialize, Clone)]
struct StartupFile {
    path: String,
    content: String,
}

struct PendingStartupFile(Mutex<Option<StartupFile>>);

fn markdown_ext(path: &str) -> bool {
    path.to_lowercase().ends_with(".md") || path.to_lowercase().ends_with(".markdown")
}

fn open_from_args(app: &tauri::AppHandle, args: &[String]) {
    for arg in args {
        if !markdown_ext(arg) {
            continue;
        }
        open_file_path(app, arg);
        break;
    }
}

fn open_file_path(app: &tauri::AppHandle, path: &str) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let payload = StartupFile {
        path: path.to_string(),
        content,
    };
    if let Some(state) = app.try_state::<PendingStartupFile>() {
        *state.0.lock().unwrap() = Some(payload.clone());
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("cm:open-file", payload);
    }
}

fn focus_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn take_startup_file(
    state: tauri::State<PendingStartupFile>,
) -> Option<StartupFile> {
    state.0.lock().unwrap().take()
}

#[tauri::command]
async fn confirm_discard(app: tauri::AppHandle, message: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .message(message)
            .title("chillMarkdown")
            .buttons(MessageDialogButtons::OkCancelCustom("Continue".into(), "Cancel".into()))
            .blocking_show()
    })
    .await
    .unwrap_or(false)
}

#[derive(serde::Serialize)]
struct FileInfo {
    size: u64,
    created_millis: Option<u64>,
    modified_millis: Option<u64>,
}

fn epoch_millis(t: std::io::Result<std::time::SystemTime>) -> Option<u64> {
    t.ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

#[tauri::command]
fn file_info(path: String) -> Option<FileInfo> {
    let meta = std::fs::metadata(&path).ok()?;
    Some(FileInfo {
        size: meta.len(),
        created_millis: epoch_millis(meta.created()),
        modified_millis: epoch_millis(meta.modified()),
    })
}

#[tauri::command]
fn documents_dir(app: tauri::AppHandle) -> Result<String, String> {
    app.path()
        .document_dir()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn show_open_dialog(app: tauri::AppHandle) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("Markdown", &["md", "markdown", "txt"])
            .blocking_pick_file()
            .and_then(|f: FilePath| f.into_path().ok())
            .map(|p| p.to_string_lossy().to_string())
    })
    .await
    .unwrap_or(None)
}

#[tauri::command]
async fn show_save_dialog(
    app: tauri::AppHandle,
    default_dir: Option<String>,
    file_name: Option<String>,
) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut builder = app.dialog().file();
        builder = match file_name {
            Some(n) => builder.set_file_name(n),
            None => builder,
        };
        builder = match default_dir {
            Some(d) => builder.set_directory(d),
            None => builder,
        };
        builder
            .blocking_save_file()
            .and_then(|f: FilePath| f.into_path().ok())
            .map(|p| p.to_string_lossy().to_string())
    })
    .await
    .unwrap_or(None)
}

#[tauri::command]
fn read_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_file(path: String, contents: String) -> Result<usize, String> {
    std::fs::write(&path, contents.as_bytes())
        .map(|_| contents.len())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn save_file_bytes(path: String, bytes: Vec<u8>) -> Result<usize, String> {
    std::fs::write(&path, bytes)
        .map(|_| 0)
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            focus_main_window(app);
            open_from_args(app, &argv[1..]);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(PendingStartupFile(Mutex::new(None)))
        .setup(|app| {
            let args: Vec<String> = std::env::args().skip(1).collect();
            open_from_args(app.handle(), &args);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            documents_dir,
            show_open_dialog,
            show_save_dialog,
            confirm_discard,
            file_info,
            read_file,
            save_file,
            save_file_bytes,
            take_startup_file
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| match event {
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            tauri::RunEvent::Opened { urls } => {
                for url in urls {
                    if let Ok(path) = url.to_file_path()
                        && markdown_ext(&path.to_string_lossy())
                    {
                        open_file_path(_app, &path.to_string_lossy());
                    }
                }
            }
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => {
                focus_main_window(_app);
            }
            _ => {}
        });
}