//! The elevated broker process.
//!
//! `corepty.exe` re-launches itself with `--broker …` via `ShellExecute "runas"`
//! (which raises the UAC prompt). The elevated instance runs *this* code instead
//! of the Tauri app: it connects back to the non-elevated app over a named pipe,
//! spawns a ConPTY running the requested shell **elevated**, and relays terminal
//! I/O + resize using the [`crate::session::proto`] framing. When the shell
//! exits (or the app closes the pipe) the broker tears down and exits.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

use crate::session::local::resolve_program;
use crate::session::proto::{self, TAG_CLOSE, TAG_DATA, TAG_EXIT, TAG_RESIZE};

/// Entry point for a `--broker` invocation. Blocks until the elevated shell ends.
pub fn run(args: &[String]) {
    let Some(pipe_name) = arg(args, "--pipe") else {
        return;
    };
    let shell = arg(args, "--shell").unwrap_or_else(|| "powershell".to_string());
    let cols: u16 = arg(args, "--cols").and_then(|s| s.parse().ok()).unwrap_or(80);
    let rows: u16 = arg(args, "--rows").and_then(|s| s.parse().ok()).unwrap_or(24);
    log(&format!("start: shell={shell} cols={cols} rows={rows} pipe={pipe_name}"));
    match bridge(&pipe_name, &shell, cols.max(1), rows.max(1)) {
        Ok(()) => log("finished"),
        Err(e) => log(&format!("ERROR: {e}")),
    }
}

/// Append a diagnostic line to `%TEMP%\corepty-broker.log`.
fn log(msg: &str) {
    use std::io::Write as _;
    let path = std::env::temp_dir().join("corepty-broker.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{msg}");
    }
}

fn arg(args: &[String], key: &str) -> Option<String> {
    args.iter().position(|a| a == key).and_then(|i| args.get(i + 1).cloned())
}

fn bridge(pipe_name: &str, shell: &str, cols: u16, rows: u16) -> Result<(), String> {
    let pipe = Arc::new(open_pipe(pipe_name).ok_or("could not connect to the app pipe")?);
    log("pipe connected");

    let (program, pargs) =
        resolve_program(shell).ok_or_else(|| format!("unknown shell '{shell}'"))?;
    log(&format!("program: {program}"));

    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| format!("openpty: {e}"))?;

    let mut cmd = CommandBuilder::new(&program);
    for a in &pargs {
        cmd.arg(a);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "CorePTY");
    if let Ok(home) = std::env::var("USERPROFILE") {
        if !home.is_empty() {
            cmd.cwd(home);
        }
    }

    let mut child = pair.slave.spawn_command(cmd).map_err(|e| format!("spawn: {e}"))?;
    log("shell spawned; streaming");
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let mut writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let master = pair.master;
    let mut killer = child.clone_killer();

    // PTY output -> pipe.
    let out = {
        let pipe = pipe.clone();
        thread::spawn(move || {
            let mut buf = [0u8; 16 * 1024];
            let mut total = 0usize;
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        total += n;
                        let mut w: &File = &pipe;
                        if proto::write_frame(&mut w, TAG_DATA, &buf[..n]).is_err() {
                            log("pipe write failed");
                            break;
                        }
                    }
                }
            }
            log(&format!("pty reader ended after {total} bytes"));
        })
    };

    // App input / resize -> PTY. Ends when the app closes the pipe.
    let inp = {
        let pipe = pipe.clone();
        thread::spawn(move || {
            let mut r: &File = &pipe;
            while let Ok(frame) = proto::read_frame(&mut r) {
                match frame.tag {
                    TAG_DATA => {
                        let _ = writer.write_all(&frame.payload);
                        let _ = writer.flush();
                    }
                    TAG_RESIZE if frame.payload.len() >= 4 => {
                        let c = u16::from_le_bytes([frame.payload[0], frame.payload[1]]);
                        let r = u16::from_le_bytes([frame.payload[2], frame.payload[3]]);
                        let _ = master.resize(PtySize {
                            rows: r.max(1),
                            cols: c.max(1),
                            pixel_width: 0,
                            pixel_height: 0,
                        });
                    }
                    TAG_CLOSE => break,
                    _ => {}
                }
            }
            // The app asked to close (or the pipe broke): stop the elevated shell.
            let _ = killer.kill();
        })
    };

    // Wait for the shell to exit, tell the app, then let it drain.
    let code = child.wait().ok().map(|s| s.exit_code() as i32).unwrap_or(-1);
    log(&format!("shell exited: code={code}"));
    {
        let mut w: &File = &pipe;
        let _ = proto::write_frame(&mut w, TAG_EXIT, &code.to_le_bytes());
    }
    thread::sleep(Duration::from_millis(120));
    let _ = (out, inp);
    Ok(())
}

/// Open the app's named pipe as a client, retrying while it spins up.
fn open_pipe(name: &str) -> Option<File> {
    for _ in 0..100 {
        if let Ok(f) = OpenOptions::new().read(true).write(true).open(name) {
            return Some(f);
        }
        thread::sleep(Duration::from_millis(50));
    }
    None
}
