use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd, RawFd};
use std::process::{Child, Command, ExitStatus};

pub const CHILD_CHANNEL_FD: RawFd = 3;

pub struct SpawnedChild {
    child: Child,
    channel: OwnedFd,
}

impl SpawnedChild {
    pub fn spawn(_command: Command) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "process channel launcher not implemented",
        ))
    }

    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn send(&self, _packet: &[u8]) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "packet send not implemented",
        ))
    }

    pub fn recv(&self, _buffer: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "packet receive not implemented",
        ))
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait()
    }
}

impl AsFd for SpawnedChild {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.channel.as_fd()
    }
}
