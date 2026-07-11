//! Tiny framed protocol spoken over the named pipe that connects the
//! (non-elevated) app to the elevated broker. Both ends are the same binary, so
//! they always agree on the wire format.
//!
//! Frame = `[tag: u8][len: u32 LE][payload: len bytes]`.

use std::io::{self, Read, Write};

/// Raw terminal bytes — app→broker is keystrokes, broker→app is output.
pub const TAG_DATA: u8 = 1;
/// app→broker resize. Payload = `[cols: u16 LE][rows: u16 LE]`.
pub const TAG_RESIZE: u8 = 2;
/// broker→app shell exit. Payload = `[code: i32 LE]`.
pub const TAG_EXIT: u8 = 3;
/// app→broker: stop the elevated shell now (the tab was closed).
pub const TAG_CLOSE: u8 = 4;

pub struct Frame {
    pub tag: u8,
    pub payload: Vec<u8>,
}

pub fn write_frame(w: &mut impl Write, tag: u8, payload: &[u8]) -> io::Result<()> {
    let mut header = [0u8; 5];
    header[0] = tag;
    header[1..5].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    w.write_all(&header)?;
    if !payload.is_empty() {
        w.write_all(payload)?;
    }
    w.flush()
}

pub fn read_frame(r: &mut impl Read) -> io::Result<Frame> {
    let mut header = [0u8; 5];
    r.read_exact(&mut header)?;
    let len = u32::from_le_bytes([header[1], header[2], header[3], header[4]]) as usize;
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    Ok(Frame {
        tag: header[0],
        payload,
    })
}
