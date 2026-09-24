use crate::name::SecretName;
use crate::policy::{Decision, decide};
use crate::process::SpawnedChild;
use crate::protocol::{
    ErrorCode, PROTOCOL_VERSION, ProtocolError, Request, RequestFrame, Response, ResponseFrame,
    recv_frame, send_frame,
};
use std::collections::BTreeSet;
use std::io;
use std::os::fd::AsFd;
use std::process::ExitStatus;

pub struct BrokerSession {
    child: SpawnedChild,
    allowed: BTreeSet<SecretName>,
    failed: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("broker session is closed")]
    Closed,
}

impl BrokerSession {
    pub fn new(child: SpawnedChild, allowed: BTreeSet<SecretName>) -> Self {
        Self {
            child,
            allowed,
            failed: false,
        }
    }

    pub fn serve_one<F>(&mut self, resolve: F) -> Result<(), SessionError>
    where
        F: FnOnce(&SecretName) -> Option<String>,
    {
        if self.failed {
            return Err(SessionError::Closed);
        }
        let request: RequestFrame = match recv_frame(self.child.as_fd()) {
            Ok(Some(request)) => request,
            Ok(None) => {
                self.failed = true;
                return Err(SessionError::Closed);
            }
            Err(error) => {
                self.failed = true;
                return Err(error.into());
            }
        };
        let response = if request.version != PROTOCOL_VERSION {
            Response::Error {
                code: ErrorCode::UnsupportedVersion,
            }
        } else {
            match request.request {
                Request::Get { name } => match decide(&name, &self.allowed) {
                    Decision::Deny(reason) => Response::Denied { reason },
                    Decision::Allow => match resolve(&name) {
                        Some(value) => Response::Value { value },
                        None => Response::Error {
                            code: ErrorCode::SecretNotFound,
                        },
                    },
                },
            }
        };
        let sent = send_frame(
            self.child.as_fd(),
            &ResponseFrame {
                version: PROTOCOL_VERSION,
                response,
            },
        );
        if let Err(error) = sent {
            self.failed = true;
            return Err(error.into());
        }
        Ok(())
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait()
    }
}
