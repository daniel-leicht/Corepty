//! Non-elevated side of an elevated ("Run as Administrator") session.
//!
//! We create **two** one-directional named pipes (output + input) — each with an
//! ACL that only lets elevated (admin) clients connect — then re-launch ourselves
//! as the broker via `ShellExecute "runas"` (UAC). The elevated broker connects
//! back and runs the shell; here we just bridge the pipes to the UI.
//!
//! Two pipes avoid the synchronous-handle serialization that a single duplex
//! handle would suffer from concurrent read+write (which deadlocks).

use std::ffi::c_void;
use std::fs::File;
use std::mem::size_of;
use std::os::windows::io::FromRawHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tauri::AppHandle;
use tokio::sync::mpsc::unbounded_channel;
use uuid::Uuid;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, FALSE, HANDLE};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

use super::local::LocalOptions;
use super::proto::{self, TAG_CLOSE, TAG_DATA, TAG_EXIT, TAG_RESIZE};
use super::{emit_data, emit_exit, emit_status, SessionInfo, SessionInput, SessionKind, SessionManager};

pub fn spawn(
    app: AppHandle,
    manager: &SessionManager,
    opts: LocalOptions,
) -> Result<SessionInfo, String> {
    let id = opts.id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
    let shell = opts.shell.clone();
    let cols = opts.cols.max(1);
    let rows = opts.rows.max(1);
    let base = Uuid::new_v4();
    let pipe_out = format!(r"\\.\pipe\corepty-{base}-o"); // broker writes, app reads
    let pipe_in = format!(r"\\.\pipe\corepty-{base}-i"); // app writes, broker reads

    // 1. Create the pipes — restricted so only elevated (admin) clients can open them.
    let h_out = create_pipe(&pipe_out)?;
    let h_in = create_pipe(&pipe_in)?;
    log(&format!("pipes created: {base}"));

    // 2. Re-launch ourselves elevated as the broker (raises the UAC prompt).
    if let Err(e) = launch_broker(&pipe_out, &pipe_in, &shell, cols, rows) {
        let _ = unsafe { CloseHandle(h_out) };
        let _ = unsafe { CloseHandle(h_in) };
        return Err(e);
    }

    // 3. Register the session and bridge the pipes <-> UI on a worker thread.
    let title = opts.title.clone().unwrap_or_else(|| format!("{shell} (Admin)"));
    let info = SessionInfo {
        id: id.clone(),
        kind: SessionKind::Local,
        title,
    };
    let (tx, mut rx) = unbounded_channel::<SessionInput>();
    manager.register(info.clone(), tx);
    emit_status(&app, &id, "connecting", None);
    log(&format!("broker launched (id={id}); awaiting connect"));

    // If the elevated broker never connects, surface an error (pointing at its log).
    let connected = Arc::new(AtomicBool::new(false));
    {
        let (app, id, connected) = (app.clone(), id.clone(), connected.clone());
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(25));
            if !connected.load(Ordering::SeqCst) {
                log("broker did not connect within 25s");
                emit_exit(
                    &app,
                    &id,
                    None,
                    Some(
                        "The elevated broker didn't connect — see %TEMP%\\corepty-broker.log."
                            .to_string(),
                    ),
                );
            }
        });
    }

    let raw_out = h_out.0 as isize; // HANDLEs aren't Send; smuggle them across as ints.
    let raw_in = h_in.0 as isize;
    thread::spawn(move || {
        let ho = HANDLE(raw_out as *mut c_void);
        let hi = HANDLE(raw_in as *mut c_void);
        // Wait for the elevated broker to connect both pipes.
        let _ = unsafe { ConnectNamedPipe(ho, None) };
        let _ = unsafe { ConnectNamedPipe(hi, None) };
        connected.store(true, Ordering::SeqCst);
        let fout = unsafe { File::from_raw_handle(ho.0) }; // read-only here
        let fin = unsafe { File::from_raw_handle(hi.0) }; // write-only here
        emit_status(&app, &id, "connected", None);
        log(&format!("broker connected (id={id}); streaming"));

        // output pipe -> UI
        let reader = {
            let (app, id) = (app.clone(), id.clone());
            thread::spawn(move || {
                let mut r: &File = &fout;
                let mut got = false;
                loop {
                    match proto::read_frame(&mut r) {
                        Ok(frame) if frame.tag == TAG_DATA => {
                            if !got {
                                got = true;
                                log(&format!("received first {} bytes", frame.payload.len()));
                            }
                            emit_data(&app, &id, &frame.payload);
                        }
                        Ok(frame) if frame.tag == TAG_EXIT => {
                            let code = (frame.payload.len() >= 4).then(|| {
                                i32::from_le_bytes([
                                    frame.payload[0],
                                    frame.payload[1],
                                    frame.payload[2],
                                    frame.payload[3],
                                ])
                            });
                            emit_exit(&app, &id, code, None);
                            return;
                        }
                        Ok(_) => {}
                        Err(_) => {
                            emit_exit(&app, &id, None, None);
                            return;
                        }
                    }
                }
            })
        };

        // UI -> input pipe
        while let Some(msg) = rx.blocking_recv() {
            let mut w: &File = &fin;
            match msg {
                SessionInput::Data(bytes) => {
                    let _ = proto::write_frame(&mut w, TAG_DATA, &bytes);
                }
                SessionInput::Resize { cols, rows } => {
                    let mut p = [0u8; 4];
                    p[0..2].copy_from_slice(&cols.to_le_bytes());
                    p[2..4].copy_from_slice(&rows.to_le_bytes());
                    let _ = proto::write_frame(&mut w, TAG_RESIZE, &p);
                }
                SessionInput::Close => {
                    let _ = proto::write_frame(&mut w, TAG_CLOSE, &[]);
                    break;
                }
            }
        }
        drop(fin);
        let _ = reader;
    });

    Ok(info)
}

/// Create a named pipe with a DACL that grants access only to Administrators and
/// SYSTEM, so a non-elevated process can't hijack the elevated shell.
fn create_pipe(name: &str) -> Result<HANDLE, String> {
    let sddl = wide("D:P(A;;GA;;;BA)(A;;GA;;;SY)");
    let mut psd = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut psd,
            None,
        )
        .map_err(|e| format!("security descriptor: {e}"))?;
    }
    let sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: psd.0,
        bInheritHandle: FALSE,
    };
    let wname = wide(name);
    let handle = unsafe {
        CreateNamedPipeW(
            PCWSTR(wname.as_ptr()),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            1,
            64 * 1024,
            64 * 1024,
            0,
            Some(&sa),
        )
    };
    if handle.is_invalid() {
        return Err(format!(
            "CreateNamedPipe failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(handle)
}

/// Launch `corepty.exe --broker …` elevated. Blocks on the UAC prompt; returns an
/// error if the user declines.
fn launch_broker(
    pipe_out: &str,
    pipe_in: &str,
    shell: &str,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_w = wide(&exe.to_string_lossy());
    let verb_w = wide("runas");
    let params = format!(
        r#"--broker --pipe-out "{pipe_out}" --pipe-in "{pipe_in}" --shell "{shell}" --cols {cols} --rows {rows}"#
    );
    let params_w = wide(&params);

    // ShellExecute can hand off to Shell extensions, so COM must be initialized.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    let hinst = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb_w.as_ptr()),
            PCWSTR(exe_w.as_ptr()),
            PCWSTR(params_w.as_ptr()),
            PCWSTR::null(),
            SW_HIDE,
        )
    };
    let code = hinst.0 as isize;
    if code <= 32 {
        return Err(if code == 5 {
            "Elevation was cancelled".to_string()
        } else {
            format!("could not elevate (ShellExecute error {code})")
        });
    }
    Ok(())
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Append a diagnostic line to `%TEMP%\corepty-elevated.log`.
fn log(msg: &str) {
    use std::io::Write as _;
    let path = std::env::temp_dir().join("corepty-elevated.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{msg}");
    }
}
