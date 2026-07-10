// Tauri command surface exposed to the frontend.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use tauri::{AppHandle, State};

use crate::session::{local, ssh, telnet, SessionInfo, SessionInput, SessionManager};

#[tauri::command]
pub fn session_create_local(
    app: AppHandle,
    manager: State<SessionManager>,
    options: local::LocalOptions,
) -> Result<SessionInfo, String> {
    local::spawn(app, &manager, options)
}

#[tauri::command]
pub fn session_create_ssh(
    app: AppHandle,
    manager: State<SessionManager>,
    options: ssh::SshOptions,
) -> Result<SessionInfo, String> {
    ssh::connect(app, &manager, options)
}

#[tauri::command]
pub fn session_create_telnet(
    app: AppHandle,
    manager: State<SessionManager>,
    options: telnet::TelnetOptions,
) -> Result<SessionInfo, String> {
    telnet::connect(app, &manager, options)
}

/// Write UTF-8 text (keystrokes) to a session.
#[tauri::command]
pub fn session_write(
    manager: State<SessionManager>,
    id: String,
    data: String,
) -> Result<(), String> {
    manager.send(&id, SessionInput::Data(data.into_bytes()))
}

/// Write raw (base64-encoded) bytes to a session — used for binary-safe paste.
#[tauri::command]
pub fn session_write_bytes(
    manager: State<SessionManager>,
    id: String,
    data: String,
) -> Result<(), String> {
    let bytes = STANDARD
        .decode(data)
        .map_err(|e| format!("invalid base64: {e}"))?;
    manager.send(&id, SessionInput::Data(bytes))
}

#[tauri::command]
pub fn session_resize(
    manager: State<SessionManager>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    manager.send(&id, SessionInput::Resize { cols, rows })
}

#[tauri::command]
pub fn session_close(manager: State<SessionManager>, id: String) -> Result<(), String> {
    let result = manager.send(&id, SessionInput::Close);
    manager.remove(&id);
    // Closing an already-dead session is not an error from the UI's view.
    let _ = result;
    Ok(())
}

#[tauri::command]
pub fn session_list(manager: State<SessionManager>) -> Vec<SessionInfo> {
    manager.list()
}
