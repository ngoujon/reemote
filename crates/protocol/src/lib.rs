//! Wire protocol shared between the reemote host (controlled machine) and
//! client (controller). Messages are length-prefixed bincode frames sent
//! over an already-established TLS stream.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Protocol version. Bumped on any breaking change to `Message`.
pub const PROTOCOL_VERSION: u32 = 1;

/// Hard cap on a single frame's encoded size, to bound memory use when
/// reading an untrusted length prefix.
pub const MAX_FRAME_BYTES: u32 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Physical key identity, independent of keyboard layout or modifier state.
/// Printable characters are sent separately via `InputEvent::Text` (which
/// carries the already-composed, layout-aware character); this enum only
/// covers keys needed for navigation, editing, and modifier-based shortcuts
/// (e.g. Ctrl+C), where a `Text` event alone wouldn't fire on most OSes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SpecialKey {
    Enter,
    Escape,
    Backspace,
    Tab,
    Space,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Shift,
    Control,
    Alt,
    Meta,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputEvent {
    MouseMove { x: f32, y: f32 },
    MouseButton { button: MouseButton, down: bool },
    MouseScroll { dx: f32, dy: f32 },
    SpecialKey { key: SpecialKey, down: bool },
    /// Already layout-composed printable text (handles shift/AltGr/dead
    /// keys correctly since the client's OS does the composition).
    Text { chars: String },
}

/// One monitor as reported by the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub id: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

/// A rectangular, JPEG-encoded region of the selected display. The client
/// composites these onto its local framebuffer at (x, y). A full frame is
/// sent as one chunk covering the whole display; subsequent frames only
/// send the changed region(s) to save bandwidth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameChunk {
    pub display_id: u32,
    pub full_width: u32,
    pub full_height: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub jpeg: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    /// First message sent by the client after the TLS handshake completes.
    ClientHello { protocol_version: u32 },
    /// Host's reply to `ClientHello`.
    ServerHello {
        protocol_version: u32,
        host_name: String,
    },
    /// Client authenticates with the host's configured password.
    AuthRequest { password: String },
    AuthResult { ok: bool, reason: Option<String> },

    /// Host announces its displays; client picks one to view.
    Displays { displays: Vec<DisplayInfo> },
    SelectDisplay { display_id: u32 },

    /// Host -> client screen updates.
    Frame(FrameChunk),

    /// Client -> host control input. Only accepted after a successful
    /// `AuthResult { ok: true, .. }` and while control is not locked out.
    Input(InputEvent),

    /// Either side may request the other end to enable/disable clipboard
    /// sync or send clipboard contents.
    ClipboardSync { text: String },

    /// Graceful disconnect with a human-readable reason.
    Disconnect { reason: String },

    /// Periodic keepalive; either side may send it.
    Ping,
    Pong,
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("frame too large: {0} bytes (max {MAX_FRAME_BYTES})")]
    FrameTooLarge(u32),
    #[error("bincode error: {0}")]
    Bincode(#[from] bincode::Error),
}

/// Writes a single length-prefixed, bincode-encoded message.
pub async fn write_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    msg: &Message,
) -> Result<(), ProtocolError> {
    let payload = bincode::serialize(msg)?;
    let len = payload.len() as u32;
    if len > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge(len));
    }
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

/// Reads a single length-prefixed, bincode-encoded message.
pub async fn read_message<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<Message, ProtocolError> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf);
    if len > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge(len));
    }
    let mut payload = vec![0u8; len as usize];
    reader.read_exact(&mut payload).await?;
    let msg = bincode::deserialize(&payload)?;
    Ok(msg)
}
