use keyleash::client::{Client, ClientError};
use keyleash::name::SecretName;
use keyleash::policy::DenyReason;
use keyleash::process::SpawnedChild;
use keyleash::protocol::{ErrorCode, MAX_FRAME_SIZE, ProtocolError, recv_frame};
use keyleash::session::BrokerSession;
use rustix::net::{SendFlags, send};
use std::collections::BTreeSet;
use std::env;
use std::error::Error;
use std::os::fd::BorrowedFd;
use std::process::Command;

const PROBE_ENV: &str = "KEYLEASH_BROKER_PROBE";

#[test]
fn broker_probe_child() -> Result<(), Box<dyn Error>> {
    let Ok(mode) = env::var(PROBE_ENV) else {
        return Ok(());
    };

    // SAFETY: SpawnedChild installed fd 3 for this probe. This is its only owner.
    let mut client = unsafe { Client::from_inherited_fd()? };
    let requested = SecretName::try_from("DATABASE_URL")?;
    match mode.as_str() {
        "allowed" => assert_eq!(client.get(requested)?, "postgres://example"),
        "denied" => assert!(matches!(
            client.get(requested),
            Err(ClientError::Denied(DenyReason::NotAllowed))
        )),
        "missing" => assert!(matches!(
            client.get(requested),
            Err(ClientError::Remote(ErrorCode::SecretNotFound))
        )),
        "bad_response" => assert!(matches!(
            client.get(requested),
            Err(ClientError::UnsupportedVersion(2))
        )),
        other => panic!("unexpected probe mode {other}"),
    }
    Ok(())
}

#[test]
fn broker_raw_probe_child() -> Result<(), Box<dyn Error>> {
    let Ok(mode) = env::var(PROBE_ENV) else {
        return Ok(());
    };
    // SAFETY: SpawnedChild installs fd 3 for this exact child process.
    let channel = unsafe { BorrowedFd::borrow_raw(3) };
    let packet = match mode.as_str() {
        "unsupported" => {
            br#"{"version":2,"request":{"type":"get","name":"DATABASE_URL"}}"#.to_vec()
        }
        "malformed" | "malformed_then_valid" => b"not-json".to_vec(),
        "oversized" => vec![b'x'; MAX_FRAME_SIZE + 1],
        other => panic!("unexpected raw probe mode {other}"),
    };
    assert_eq!(send(channel, &packet, SendFlags::NOSIGNAL)?, packet.len());
    if mode == "malformed_then_valid" {
        let valid = br#"{"version":1,"request":{"type":"get","name":"DATABASE_URL"}}"#;
        assert_eq!(send(channel, valid, SendFlags::NOSIGNAL)?, valid.len());
    }
    if mode == "unsupported" {
        let response: serde_json::Value = recv_frame(channel)?.unwrap();
        assert_eq!(response["version"], 1);
        assert_eq!(response["response"]["code"], "unsupported_version");
    }
    Ok(())
}

fn probe_command(test_name: &str, mode: &str) -> Result<Command, Box<dyn Error>> {
    let mut command = Command::new(env::current_exe()?);
    command
        .args(["--exact", test_name, "--nocapture"])
        .env(PROBE_ENV, mode);
    Ok(command)
}

#[test]
fn allowed_request_resolves_and_reaches_the_child() -> Result<(), Box<dyn Error>> {
    let child = SpawnedChild::spawn(probe_command("broker_probe_child", "allowed")?)?;
    let allowed = BTreeSet::from([SecretName::try_from("DATABASE_URL")?]);
    let mut session = BrokerSession::new(child, allowed);
    let mut resolved = false;
    session.serve_one(|name| {
        assert_eq!(name.as_ref(), "DATABASE_URL");
        resolved = true;
        Some("postgres://example".to_owned())
    })?;
    assert!(resolved);
    assert!(session.wait()?.success());
    Ok(())
}

#[test]
fn denied_request_never_calls_the_resolver() -> Result<(), Box<dyn Error>> {
    let child = SpawnedChild::spawn(probe_command("broker_probe_child", "denied")?)?;
    let mut session = BrokerSession::new(child, BTreeSet::new());
    session.serve_one(|_| panic!("resolver must not run after denial"))?;
    assert!(session.wait()?.success());
    Ok(())
}

#[test]
fn missing_allowed_secret_returns_a_typed_error() -> Result<(), Box<dyn Error>> {
    let child = SpawnedChild::spawn(probe_command("broker_probe_child", "missing")?)?;
    let allowed = BTreeSet::from([SecretName::try_from("DATABASE_URL")?]);
    let mut session = BrokerSession::new(child, allowed);
    session.serve_one(|_| None)?;
    assert!(session.wait()?.success());
    Ok(())
}

#[test]
fn unsupported_version_gets_an_error_response() -> Result<(), Box<dyn Error>> {
    let child = SpawnedChild::spawn(probe_command("broker_raw_probe_child", "unsupported")?)?;
    let mut session = BrokerSession::new(child, BTreeSet::new());
    session.serve_one(|_| panic!("resolver must not run for unsupported version"))?;
    assert!(session.wait()?.success());
    Ok(())
}

#[test]
fn malformed_json_ends_the_session() -> Result<(), Box<dyn Error>> {
    let child = SpawnedChild::spawn(probe_command("broker_raw_probe_child", "malformed")?)?;
    let mut session = BrokerSession::new(child, BTreeSet::new());
    assert!(matches!(
        session.serve_one(|_| panic!("resolver must not run for malformed JSON")),
        Err(keyleash::session::SessionError::Protocol(
            ProtocolError::Json(_)
        ))
    ));
    assert!(session.wait()?.success());
    Ok(())
}

#[test]
fn malformed_json_prevents_later_requests() -> Result<(), Box<dyn Error>> {
    let child = SpawnedChild::spawn(probe_command(
        "broker_raw_probe_child",
        "malformed_then_valid",
    )?)?;
    let mut session = BrokerSession::new(child, BTreeSet::new());
    assert!(matches!(
        session.serve_one(|_| panic!("resolver must not run")),
        Err(keyleash::session::SessionError::Protocol(
            ProtocolError::Json(_)
        ))
    ));
    assert!(matches!(
        session.serve_one(|_| panic!("failed session must not resolve")),
        Err(keyleash::session::SessionError::Closed)
    ));
    assert!(session.wait()?.success());
    Ok(())
}

#[test]
fn oversized_packet_ends_the_session_before_decoding() -> Result<(), Box<dyn Error>> {
    let child = SpawnedChild::spawn(probe_command("broker_raw_probe_child", "oversized")?)?;
    let mut session = BrokerSession::new(child, BTreeSet::new());
    assert!(matches!(
        session.serve_one(|_| panic!("resolver must not run for oversized packet")),
        Err(keyleash::session::SessionError::Protocol(ProtocolError::FrameTooLarge { len }))
            if len == MAX_FRAME_SIZE + 1
    ));
    assert!(session.wait()?.success());
    Ok(())
}

#[test]
fn client_rejects_an_unsupported_response_version() -> Result<(), Box<dyn Error>> {
    let mut child = SpawnedChild::spawn(probe_command("broker_probe_child", "bad_response")?)?;
    let mut request = [0_u8; 256];
    let length = child.recv(&mut request)?;
    let parsed: serde_json::Value = serde_json::from_slice(&request[..length])?;
    assert_eq!(parsed["request"]["name"], "DATABASE_URL");
    child.send(br#"{"version":2,"response":{"type":"value","value":"irrelevant"}}"#)?;
    assert!(child.wait()?.success());
    Ok(())
}
