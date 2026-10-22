//! Optional delivery of the server's session pane, which clients draw in place of
//! their native sidebar. It rides on named endpoint controls so the generation-1
//! core codecs stay untouched: clients opt in through the hello, servers advertise
//! the capability, and either side ignores the other's unknown controls.

use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};

use super::{ClientMessage, ClientPaneInputEvent, FrameData, ServerMessage};

pub(crate) const CAPABILITY: &str = "session_pane";
/// Server to client: the pane's state and cells, or `pane: null` while it is down.
pub(crate) const MESSAGE_KIND: &str = "endpoint.session-pane.v1";
/// Client to server: input aimed at the pane.
pub(crate) const INPUT_KIND: &str = "endpoint.session-pane.input.v1";
/// Client to server: the size of the client's slot for the pane.
pub(crate) const SLOT_KIND: &str = "endpoint.session-pane.slot.v1";

const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

/// JSON envelope. New fields must be optional; `frame` is base64 of the bincode
/// `FrameData` layout, which the v1 core already freezes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct UpdateWire {
    boot_id: String,
    revision: u64,
    #[serde(default)]
    pane: Option<ViewWire>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ViewWire {
    terminal_id: String,
    #[serde(default)]
    focused: bool,
    width: u16,
    collapsed_width: u16,
    #[serde(default)]
    mouse_reporting: bool,
    #[serde(default)]
    sgr_pixel_mouse: bool,
    #[serde(default)]
    pixel_width: u32,
    #[serde(default)]
    pixel_height: u32,
    frame: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPaneUpdate {
    pub boot_id: String,
    pub revision: u64,
    pub pane: Option<SessionPaneView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPaneView {
    pub terminal_id: String,
    pub focused: bool,
    /// Expanded width the server wants every client to give the pane.
    pub width: u16,
    pub collapsed_width: u16,
    pub mouse_reporting: bool,
    pub sgr_pixel_mouse: bool,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub frame: FrameData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPaneInput {
    pub terminal_id: String,
    pub events: Vec<ClientPaneInputEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPaneSlot {
    pub cols: u16,
    pub rows: u16,
}

pub(crate) fn update_message(update: &SessionPaneUpdate) -> Result<ServerMessage, String> {
    let pane = update
        .pane
        .as_ref()
        .map(|view| {
            let frame = bincode::serde::encode_to_vec(&view.frame, bincode::config::standard())
                .map_err(|error| error.to_string())?;
            Ok::<_, String>(ViewWire {
                terminal_id: view.terminal_id.clone(),
                focused: view.focused,
                width: view.width,
                collapsed_width: view.collapsed_width,
                mouse_reporting: view.mouse_reporting,
                sgr_pixel_mouse: view.sgr_pixel_mouse,
                pixel_width: view.pixel_width,
                pixel_height: view.pixel_height,
                frame: STANDARD_NO_PAD.encode(frame),
            })
        })
        .transpose()?;
    let data = serde_json::to_string(&UpdateWire {
        boot_id: update.boot_id.clone(),
        revision: update.revision,
        pane,
    })
    .map_err(|error| error.to_string())?;
    Ok(ServerMessage::EndpointControl {
        kind: MESSAGE_KIND.to_string(),
        data,
    })
}

pub(crate) fn decode_update(data: &str) -> Result<SessionPaneUpdate, String> {
    let wire: UpdateWire = serde_json::from_str(data).map_err(|error| error.to_string())?;
    let pane = wire
        .pane
        .map(|view| {
            let bytes = STANDARD_NO_PAD
                .decode(view.frame.as_bytes())
                .map_err(|error| error.to_string())?;
            if bytes.len() > MAX_FRAME_BYTES {
                return Err("session pane frame is too large".to_string());
            }
            let (frame, consumed): (FrameData, usize) = bincode::serde::decode_from_slice(
                &bytes,
                bincode::config::standard().with_limit::<MAX_FRAME_BYTES>(),
            )
            .map_err(|error| error.to_string())?;
            if consumed != bytes.len()
                || frame.cells.len() != usize::from(frame.width) * usize::from(frame.height)
            {
                return Err("session pane frame does not match its size".to_string());
            }
            Ok(SessionPaneView {
                terminal_id: view.terminal_id,
                focused: view.focused,
                width: view.width,
                collapsed_width: view.collapsed_width,
                mouse_reporting: view.mouse_reporting,
                sgr_pixel_mouse: view.sgr_pixel_mouse,
                pixel_width: view.pixel_width,
                pixel_height: view.pixel_height,
                frame,
            })
        })
        .transpose()?;
    Ok(SessionPaneUpdate {
        boot_id: wire.boot_id,
        revision: wire.revision,
        pane,
    })
}

pub(crate) fn input_message(input: &SessionPaneInput) -> Result<ClientMessage, String> {
    Ok(ClientMessage::EndpointControl {
        kind: INPUT_KIND.to_string(),
        data: serde_json::to_string(input).map_err(|error| error.to_string())?,
    })
}

pub(crate) fn decode_input(data: &str) -> Result<SessionPaneInput, String> {
    serde_json::from_str(data).map_err(|error| error.to_string())
}

pub(crate) fn slot_message(slot: SessionPaneSlot) -> Result<ClientMessage, String> {
    Ok(ClientMessage::EndpointControl {
        kind: SLOT_KIND.to_string(),
        data: serde_json::to_string(&slot).map_err(|error| error.to_string())?,
    })
}

pub(crate) fn decode_slot(data: &str) -> Result<SessionPaneSlot, String> {
    serde_json::from_str(data).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::CursorState;
    use ratatui::{buffer::Buffer, layout::Rect};

    fn update() -> SessionPaneUpdate {
        let mut frame = FrameData::from_ratatui_buffer(
            &Buffer::empty(Rect::new(0, 0, 3, 2)),
            Some(CursorState {
                x: 1,
                y: 0,
                visible: true,
                shape: Default::default(),
            }),
        );
        frame.cells[0].symbol = "a".into();
        SessionPaneUpdate {
            boot_id: "boot".into(),
            revision: 7,
            pane: Some(SessionPaneView {
                terminal_id: "t1".into(),
                focused: true,
                width: 26,
                collapsed_width: 8,
                mouse_reporting: true,
                sgr_pixel_mouse: false,
                pixel_width: 0,
                pixel_height: 0,
                frame,
            }),
        }
    }

    fn control_data(message: ServerMessage) -> String {
        let ServerMessage::EndpointControl { kind, data } = message else {
            panic!("expected a named control");
        };
        assert_eq!(kind, MESSAGE_KIND);
        data
    }

    #[test]
    fn update_round_trips_and_tolerates_future_fields() {
        let update = update();
        let data = control_data(update_message(&update).unwrap());
        assert_eq!(decode_update(&data).unwrap(), update);

        let mut value: serde_json::Value = serde_json::from_str(&data).unwrap();
        value["future"] = serde_json::json!(1);
        value["pane"]["future"] = serde_json::json!("x");
        assert_eq!(decode_update(&value.to_string()).unwrap(), update);

        let down = SessionPaneUpdate {
            pane: None,
            ..update
        };
        assert_eq!(
            decode_update(&control_data(update_message(&down).unwrap())).unwrap(),
            down
        );
    }

    #[test]
    fn update_rejects_frames_that_do_not_match_their_size() {
        let mut update = update();
        if let Some(view) = update.pane.as_mut() {
            view.frame.cells.pop();
        }
        assert!(decode_update(&control_data(update_message(&update).unwrap())).is_err());
        assert!(decode_update(r#"{"boot_id":"b","revision":1,"pane":{"terminal_id":"t","width":1,"collapsed_width":1,"frame":"!!"}}"#).is_err());
    }

    #[test]
    fn session_pane_v1_payload_is_frozen() {
        use sha2::{Digest, Sha256};
        let data = control_data(update_message(&update()).unwrap());
        assert_eq!(
            format!("{:x}", Sha256::digest(data.as_bytes())),
            "d203ddd398a7de0c302fa5d967a52f3b4c042655aa807f1b9d4950b97177af90"
        );
    }

    #[test]
    fn input_and_slot_round_trip() {
        let input = SessionPaneInput {
            terminal_id: "t1".into(),
            events: vec![
                ClientPaneInputEvent::TextCommit("j".into()),
                ClientPaneInputEvent::Paste("x".into()),
            ],
        };
        let ClientMessage::EndpointControl { kind, data } = input_message(&input).unwrap() else {
            panic!("expected a named control");
        };
        assert_eq!(kind, INPUT_KIND);
        assert_eq!(decode_input(&data).unwrap(), input);

        let slot = SessionPaneSlot { cols: 26, rows: 40 };
        let ClientMessage::EndpointControl { kind, data } = slot_message(slot).unwrap() else {
            panic!("expected a named control");
        };
        assert_eq!(kind, SLOT_KIND);
        assert_eq!(decode_slot(&data).unwrap(), slot);
    }
}
