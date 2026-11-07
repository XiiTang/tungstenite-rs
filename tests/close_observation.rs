//! The observed Close is the peer's input; only the automatic reply is normalized.
use std::io::{self, Cursor, Read, Write};
use tungstenite::{
    protocol::{frame::coding::CloseCode, CloseFrame, Role},
    Error, Message, WebSocket,
};

struct Wire {
    incoming: Cursor<Vec<u8>>,
    outgoing: Vec<u8>,
}
impl Read for Wire {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.incoming.read(buf)
    }
}
impl Write for Wire {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.outgoing.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn close_bytes(role: Role, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x88, payload.len() as u8];
    match role {
        Role::Client => bytes.extend_from_slice(payload),
        Role::Server => {
            bytes[1] |= 0x80;
            let mask = [1, 2, 3, 4];
            bytes.extend_from_slice(&mask);
            bytes.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        }
    }
    bytes
}
fn socket(role: Role, payload: &[u8]) -> WebSocket<Wire> {
    WebSocket::from_raw_socket(
        Wire { incoming: Cursor::new(close_bytes(role, payload)), outgoing: Vec::new() },
        role,
        None,
    )
}
fn frame(payload: &[u8]) -> Option<CloseFrame> {
    (!payload.is_empty()).then(|| CloseFrame {
        code: u16::from_be_bytes([payload[0], payload[1]]).into(),
        reason: std::str::from_utf8(&payload[2..]).unwrap().to_owned().into(),
    })
}
#[test]
fn original_close_and_automatic_reply_in_both_roles() {
    for role in [Role::Client, Role::Server] {
        for code in [None, Some(1000u16), Some(1002), Some(1005), Some(1006), Some(1015), Some(999)]
        {
            let mut payload = Vec::new();
            if let Some(code) = code {
                payload.extend_from_slice(&code.to_be_bytes());
                payload.extend_from_slice(b"original");
            }
            let mut socket = socket(role, &payload);
            assert_eq!(socket.read().unwrap(), Message::Close(frame(&payload)));
            let flush = socket.flush();
            assert!(flush.is_ok() || matches!(flush, Err(Error::ConnectionClosed)));
            let opposite = match role {
                Role::Client => Role::Server,
                Role::Server => Role::Client,
            };
            let mut reply = WebSocket::from_raw_socket(
                Wire {
                    incoming: Cursor::new(socket.get_ref().outgoing.clone()),
                    outgoing: Vec::new(),
                },
                opposite,
                None,
            );
            let expected = match frame(&payload) {
                Some(frame) if !frame.code.is_allowed() => Some(CloseFrame {
                    code: CloseCode::Protocol,
                    reason: "Protocol violation".into(),
                }),
                frame => frame,
            };
            assert_eq!(reply.read().unwrap(), Message::Close(expected));
            let _ = socket.read();
            assert!(!socket.can_read());
        }
    }
}
#[test]
fn our_close_acknowledgement_keeps_original_and_adds_no_reply() {
    for role in [Role::Client, Role::Server] {
        for code in [1000u16, 1002, 1005] {
            let payload = code.to_be_bytes();
            let mut socket = socket(role, &payload);
            socket.close(None).unwrap();
            let before = socket.get_ref().outgoing.clone();
            assert_eq!(socket.read().unwrap(), Message::Close(frame(&payload)));
            let _ = socket.flush();
            assert_eq!(socket.get_ref().outgoing, before);
        }
    }
}
#[test]
fn malformed_close_still_fails_before_observation() {
    for role in [Role::Client, Role::Server] {
        for payload in [&[3][..], &[3, 232, 255][..]] {
            let mut socket = socket(role, payload);
            assert!(matches!(socket.read(), Err(Error::Protocol(_) | Error::Utf8(_))));
        }
    }
}
