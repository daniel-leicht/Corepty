// Local shell sessions backed by a real PTY (ConPTY on Windows, openpty
// elsewhere) via `portable-pty`. Supports CMD, Windows PowerShell, PowerShell 7,
// Bash (Git Bash / WSL), and arbitrary custom commands.

use std::io::{Read, Write};
use std::path::Path;

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc::unbounded_channel;
use uuid::Uuid;

use super::{
    emit_data, emit_exit, emit_status, SessionInfo, SessionInput, SessionKind, SessionManager,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalOptions {
    /// Optional caller-supplied session id (lets the UI wire up event routing
    /// before output starts). Generated if absent.
    #[serde(default)]
    pub id: Option<String>,
    /// One of: "cmd" | "powershell" | "pwsh" | "bash" | "custom".
    pub shell: String,
    /// Program to run when `shell == "custom"`.
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
    /// Optional display title override.
    #[serde(default)]
    pub title: Option<String>,
}

fn default_cols() -> u16 {
    80
}
fn default_rows() -> u16 {
    24
}

pub fn spawn(
    app: AppHandle,
    manager: &SessionManager,
    opts: LocalOptions,
) -> Result<SessionInfo, String> {
    let (program, args, default_title) = resolve_shell(&opts)?;

    let id = opts
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if manager.exists(&id) {
        return Err(format!("session id already in use: {id}"));
    }

    let mut cmd = CommandBuilder::new(&program);
    for a in &args {
        cmd.arg(a);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    // Make it easy for shells/programs to detect us.
    cmd.env("TERM_PROGRAM", "CorePTY");
    if let Some(dir) = opts.cwd.clone().or_else(home_dir) {
        if !dir.is_empty() && Path::new(&dir).is_dir() {
            cmd.cwd(dir);
        }
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: opts.rows.max(1),
            cols: opts.cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("failed to open pty: {e}"))?;

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("failed to launch {program}: {e}"))?;
    // The slave handle is no longer needed once the child owns it.
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("failed to read from pty: {e}"))?;
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("failed to write to pty: {e}"))?;
    let master = pair.master;
    let mut killer = child.clone_killer();

    let title = opts.title.clone().unwrap_or(default_title);
    let info = SessionInfo {
        id: id.clone(),
        kind: SessionKind::Local,
        title,
    };
    let (tx, mut rx) = unbounded_channel::<SessionInput>();
    manager.register(info.clone(), tx);

    // Reader: PTY output -> UI.
    {
        let app = app.clone();
        let id = id.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 16 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => emit_data(&app, &id, &buf[..n]),
                    Err(_) => break,
                }
            }
        });
    }

    // Waiter: child exit -> emit exit + deregister.
    {
        let app = app.clone();
        let id = id.clone();
        std::thread::spawn(move || {
            let code = child.wait().ok().map(|s| s.exit_code() as i32);
            app.state::<SessionManager>().remove(&id);
            emit_exit(&app, &id, code, None);
        });
    }

    // Control: UI input/resize/close -> PTY. Owns the master (for resize).
    {
        std::thread::spawn(move || {
            let master = master;
            while let Some(msg) = rx.blocking_recv() {
                match msg {
                    SessionInput::Data(bytes) => {
                        if writer.write_all(&bytes).is_err() {
                            break;
                        }
                        let _ = writer.flush();
                    }
                    SessionInput::Resize { cols, rows } => {
                        let _ = master.resize(PtySize {
                            rows: rows.max(1),
                            cols: cols.max(1),
                            pixel_width: 0,
                            pixel_height: 0,
                        });
                    }
                    SessionInput::Close => {
                        let _ = killer.kill();
                        break;
                    }
                }
            }
            let _ = killer.kill();
            drop(writer);
            drop(master);
        });
    }

    emit_status(&app, &id, "connected", None);
    Ok(info)
}

/// Resolves `(program, args, default_title)` for the requested shell. Delegates
/// the program/args lookup to `resolve_program` so the mapping lives in one place.
fn resolve_shell(opts: &LocalOptions) -> Result<(String, Vec<String>, String), String> {
    if opts.shell == "custom" {
        let program = opts
            .command
            .clone()
            .filter(|c| !c.is_empty())
            .ok_or("custom shell requires a command")?;
        let title = opts.title.clone().unwrap_or_else(|| file_stem(&program));
        return Ok((program, opts.args.clone().unwrap_or_default(), title));
    }
    let (program, args) =
        resolve_program(&opts.shell).ok_or_else(|| format!("unknown shell '{}'", opts.shell))?;
    Ok((program, args, shell_title(&opts.shell).to_string()))
}

/// Human-friendly default tab title for a known shell name.
fn shell_title(shell: &str) -> &'static str {
    match shell {
        "cmd" => "Command Prompt",
        "powershell" => "PowerShell",
        "pwsh" => "PowerShell 7",
        "bash" => "Bash",
        _ => "Shell",
    }
}

/// Resolve `(program, args)` for a known shell name — shared with the elevated
/// broker, which spawns the same shells under an elevated ConPTY.
pub(crate) fn resolve_program(shell: &str) -> Option<(String, Vec<String>)> {
    Some(match shell {
        "cmd" => (resolve_cmd(), vec![]),
        "powershell" => (resolve_powershell(), vec!["-NoLogo".to_string()]),
        "pwsh" => (resolve_pwsh(), vec!["-NoLogo".to_string()]),
        "bash" => (resolve_bash(), vec!["-l".to_string(), "-i".to_string()]),
        _ => return None,
    })
}

fn file_stem(program: &str) -> String {
    Path::new(program)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("shell")
        .to_string()
}

fn home_dir() -> Option<String> {
    std::env::var("USERPROFILE")
        .ok()
        .or_else(|| std::env::var("HOME").ok())
}

#[cfg(windows)]
fn win_root() -> String {
    std::env::var("SystemRoot")
        .or_else(|_| std::env::var("windir"))
        .unwrap_or_else(|_| r"C:\Windows".to_string())
}

#[cfg(windows)]
fn resolve_cmd() -> String {
    let p = format!(r"{}\System32\cmd.exe", win_root());
    if Path::new(&p).exists() {
        p
    } else {
        "cmd.exe".to_string()
    }
}

#[cfg(windows)]
fn resolve_powershell() -> String {
    let p = format!(
        r"{}\System32\WindowsPowerShell\v1.0\powershell.exe",
        win_root()
    );
    if Path::new(&p).exists() {
        p
    } else {
        "powershell.exe".to_string()
    }
}

#[cfg(not(windows))]
fn resolve_cmd() -> String {
    "cmd.exe".to_string()
}

#[cfg(not(windows))]
fn resolve_powershell() -> String {
    "pwsh".to_string()
}

#[cfg(windows)]
fn resolve_pwsh() -> String {
    for c in [
        r"C:\Program Files\PowerShell\7\pwsh.exe",
        r"C:\Program Files\PowerShell\7-preview\pwsh.exe",
    ] {
        if Path::new(c).exists() {
            return c.to_string();
        }
    }
    "pwsh.exe".to_string()
}

#[cfg(windows)]
fn resolve_bash() -> String {
    let mut candidates: Vec<String> = Vec::new();
    if let Ok(pf) = std::env::var("ProgramFiles") {
        candidates.push(format!(r"{pf}\Git\bin\bash.exe"));
        candidates.push(format!(r"{pf}\Git\usr\bin\bash.exe"));
    }
    candidates.push(r"C:\Program Files\Git\bin\bash.exe".to_string());
    candidates.push(r"C:\Windows\System32\bash.exe".to_string()); // WSL
    for c in &candidates {
        if Path::new(c).exists() {
            return c.clone();
        }
    }
    "bash.exe".to_string()
}

#[cfg(not(windows))]
fn resolve_pwsh() -> String {
    "pwsh".to_string()
}

#[cfg(not(windows))]
fn resolve_bash() -> String {
    for c in ["/bin/bash", "/usr/bin/bash", "/usr/local/bin/bash"] {
        if Path::new(c).exists() {
            return c.to_string();
        }
    }
    "bash".to_string()
}
