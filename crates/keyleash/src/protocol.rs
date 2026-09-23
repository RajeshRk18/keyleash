use crate::name::SecretName;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;

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
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseFrame {
    pub(crate) version: u16,
    pub(crate) response: Response,
}

#[cfg(test)]
mod tests {
    use super::{PROTOCOL_VERSION, Request, RequestFrame, Response, ResponseFrame};
    use crate::name::SecretName;

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
}
