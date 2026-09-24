use crate::name::SecretName;
use crate::policy::DenyReason;
use crate::process::CHILD_CHANNEL_FD;
use crate::protocol::{
    ErrorCode, PROTOCOL_VERSION, ProtocolError, Request, RequestFrame, Response, ResponseFrame,
    recv_frame, send_frame,
};
use std::io;
use std::os::fd::{FromRawFd, OwnedFd};

pub struct Client {
    channel: OwnedFd,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("secret access denied with reason {0:?}")]
    Denied(DenyReason),
    #[error("broker returned error {0:?}")]
    Remote(ErrorCode),
    #[error("unsupported response version {0}")]
    UnsupportedVersion(u16),
    #[error("broker closed before responding")]
    Closed,
}

impl Client {
    /// Takes sole ownership of the channel inherited on fd 3.
    ///
    /// # Safety
    ///
    /// Keyleash must have installed an open channel on fd 3. Call this once
    /// in the child, and ensure no other Rust owner controls that descriptor.
    pub unsafe fn from_inherited_fd() -> io::Result<Self> {
        if unsafe { libc::fcntl(CHILD_CHANNEL_FD, libc::F_GETFD) } == -1 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: The caller guarantees fd 3 is open and has no other Rust owner.
        let channel = unsafe { OwnedFd::from_raw_fd(CHILD_CHANNEL_FD) };
        Ok(Self { channel })
    }

    pub fn get(&mut self, name: SecretName) -> Result<String, ClientError> {
        let request = RequestFrame {
            version: PROTOCOL_VERSION,
            request: Request::Get { name },
        };
        send_frame(&self.channel, &request)?;
        let response: ResponseFrame = recv_frame(&self.channel)?.ok_or(ClientError::Closed)?;
        if response.version != PROTOCOL_VERSION {
            return Err(ClientError::UnsupportedVersion(response.version));
        }
        match response.response {
            Response::Value { value } => Ok(value),
            Response::Denied { reason } => Err(ClientError::Denied(reason)),
            Response::Error { code } => Err(ClientError::Remote(code)),
        }
    }
}
