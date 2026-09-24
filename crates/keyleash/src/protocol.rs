use crate::name::SecretName;
use crate::policy::DenyReason;
use rustix::net::{RecvFlags, SendFlags, recv, send};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{io, os::fd::AsFd};

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_FRAME_SIZE: usize = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("socket I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("invalid JSON frame: {0}")]
    Json(#[from] serde_json::Error),
    #[error("frame is too large at {len} bytes")]
    FrameTooLarge { len: usize },
}

pub fn send_frame<T: Serialize>(fd: impl AsFd, frame: &T) -> Result<(), ProtocolError> {
    let encoded = serde_json::to_vec(frame)?;
    if encoded.len() > MAX_FRAME_SIZE {
        return Err(ProtocolError::FrameTooLarge { len: encoded.len() });
    }
    let sent = send(fd, &encoded, SendFlags::NOSIGNAL).map_err(io::Error::from)?;
    if sent != encoded.len() {
        return Err(io::Error::new(io::ErrorKind::WriteZero, "short packet send").into());
    }
    Ok(())
}

pub fn recv_frame<T: DeserializeOwned>(fd: impl AsFd) -> Result<Option<T>, ProtocolError> {
    let mut buffer = vec![0_u8; MAX_FRAME_SIZE];
    let (initialized_len, received_len) =
        recv(fd, &mut buffer, RecvFlags::TRUNC).map_err(io::Error::from)?;
    if received_len > initialized_len {
        return Err(ProtocolError::FrameTooLarge { len: received_len });
    }
    if initialized_len == 0 {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&buffer[..initialized_len])?))
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum Request {
    Get { name: SecretName },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestFrame {
    pub(crate) version: u16,
    pub(crate) request: Request,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum Response {
    Value { value: String },
    Denied { reason: DenyReason },
    Error { code: ErrorCode },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseFrame {
    pub(crate) version: u16,
    pub(crate) response: Response,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum ErrorCode {
    UnsupportedVersion,
    SecretNotFound,
}

#[cfg(test)]
mod tests {
    use super::{
        ErrorCode, MAX_FRAME_SIZE, PROTOCOL_VERSION, ProtocolError, Request, RequestFrame,
        Response, ResponseFrame, recv_frame, send_frame,
    };
    use crate::name::SecretName;
    use crate::policy::DenyReason;
    use rustix::net::{AddressFamily, SendFlags, SocketFlags, SocketType, send, socketpair};

    #[test]
    fn get_request_has_a_versioned_json_envelope() {
        let frame = RequestFrame {
            version: PROTOCOL_VERSION,
            request: Request::Get {
                name: SecretName::try_from("DATABASE_URL").unwrap(),
            },
        };

        assert_eq!(
            serde_json::to_string(&frame).unwrap(),
            r#"{"version":1,"request":{"type":"get","name":"DATABASE_URL"}}"#
        );
    }

    #[test]
    fn decodes_a_get_request() {
        let json = r#"{"version":1,"request":{"type":"get","name":"DATABASE_URL"}}"#;
        let frame: RequestFrame = serde_json::from_str(json).unwrap();

        assert_eq!(frame.version, PROTOCOL_VERSION);
        match frame.request {
            Request::Get { name } => assert_eq!(name.as_ref(), "DATABASE_URL"),
        }
    }

    #[test]
    fn rejects_an_unknown_get_field() {
        let json = r#"{"version":1,"request":{"type":"get","name":"DATABASE_URL","extra":true}}"#;
        assert!(serde_json::from_str::<RequestFrame>(json).is_err());
    }

    #[test]
    fn value_response_has_a_versioned_json_envelope() {
        let frame = ResponseFrame {
            version: PROTOCOL_VERSION,
            response: Response::Value {
                value: "postgres://example".to_owned(),
            },
        };

        assert_eq!(
            serde_json::to_string(&frame).unwrap(),
            r#"{"version":1,"response":{"type":"value","value":"postgres://example"}}"#
        );
    }

    #[test]
    fn error_response_names_the_unsupported_version_code() {
        let frame = ResponseFrame {
            version: PROTOCOL_VERSION,
            response: Response::Error {
                code: ErrorCode::UnsupportedVersion,
            },
        };

        assert_eq!(
            serde_json::to_string(&frame).unwrap(),
            r#"{"version":1,"response":{"type":"error","code":"unsupported_version"}}"#
        );
    }

    #[test]
    fn denied_response_round_trips_with_a_typed_reason() {
        let frame = ResponseFrame {
            version: PROTOCOL_VERSION,
            response: Response::Denied {
                reason: DenyReason::NotAllowed,
            },
        };
        let json = serde_json::to_string(&frame).unwrap();
        assert_eq!(
            json,
            r#"{"version":1,"response":{"type":"denied","reason":"not_allowed"}}"#
        );
        let decoded: ResponseFrame = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.version, PROTOCOL_VERSION);
        match decoded.response {
            Response::Denied { reason } => assert_eq!(reason, DenyReason::NotAllowed),
            _ => panic!("expected denied response"),
        }
    }

    #[test]
    fn missing_secret_has_a_distinct_error_code() {
        let frame = ResponseFrame {
            version: PROTOCOL_VERSION,
            response: Response::Error {
                code: ErrorCode::SecretNotFound,
            },
        };
        assert_eq!(
            serde_json::to_string(&frame).unwrap(),
            r#"{"version":1,"response":{"type":"error","code":"secret_not_found"}}"#
        );
    }

    #[test]
    fn request_frame_round_trips_over_a_packet_socket() {
        let (sender, receiver) = socketpair(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::CLOEXEC,
            None,
        )
        .unwrap();
        let frame = RequestFrame {
            version: PROTOCOL_VERSION,
            request: Request::Get {
                name: SecretName::try_from("DATABASE_URL").unwrap(),
            },
        };
        send_frame(&sender, &frame).unwrap();
        let decoded: RequestFrame = recv_frame(&receiver).unwrap().unwrap();
        assert_eq!(decoded.version, PROTOCOL_VERSION);
        match decoded.request {
            Request::Get { name } => assert_eq!(name.as_ref(), "DATABASE_URL"),
        }
    }

    #[test]
    fn rejects_an_oversized_incoming_packet() {
        let (sender, receiver) = socketpair(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::CLOEXEC,
            None,
        )
        .unwrap();
        let packet = vec![b'x'; MAX_FRAME_SIZE + 1];
        assert_eq!(
            send(&sender, &packet, SendFlags::NOSIGNAL).unwrap(),
            packet.len()
        );
        let result = recv_frame::<RequestFrame>(&receiver);
        assert!(matches!(
            result,
            Err(ProtocolError::FrameTooLarge { len }) if len == MAX_FRAME_SIZE + 1
        ));
    }

    #[test]
    fn rejects_an_oversized_outgoing_frame() {
        let (sender, _receiver) = socketpair(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::CLOEXEC,
            None,
        )
        .unwrap();
        let frame = ResponseFrame {
            version: PROTOCOL_VERSION,
            response: Response::Value {
                value: "x".repeat(MAX_FRAME_SIZE),
            },
        };
        assert!(matches!(
            send_frame(&sender, &frame),
            Err(ProtocolError::FrameTooLarge { len }) if len > MAX_FRAME_SIZE
        ));
    }
}
