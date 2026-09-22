use libc::{F_GETFD, F_SETFD, FD_CLOEXEC, close, dup3, fcntl};
use rustix::net::{
    AddressFamily, RecvFlags, SendFlags, SocketFlags, SocketType, recv, send, socketpair,
};
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus};

pub const CHILD_CHANNEL_FD: RawFd = 3;

pub struct SpawnedChild {
    child: Child,
    channel: OwnedFd,
}

impl SpawnedChild {
    pub fn spawn(mut command: Command) -> io::Result<Self> {
        let (parent_fd, child_fd) = socketpair(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::CLOEXEC,
            None,
        )?;

        let parent_raw_fd = parent_fd.as_raw_fd();
        let child_raw_fd = child_fd.as_raw_fd();

        // SAFETY:
        // This callback runs after fork and before exec while capturing only raw integers whose OwnedFd remains alive until spawn returns
        // Closure performs only fd syscalls and it does not perform any allocation, logging, or locking
        let command =
            unsafe { command.pre_exec(move || install_child_channel(parent_raw_fd, child_raw_fd)) };

        let child = command.spawn()?;

        drop(child_fd);

        Ok(Self {
            child,
            channel: parent_fd,
        })
    }

    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn send(&self, packet: &[u8]) -> io::Result<()> {
        // why NOSIGNAL? If peer is closed, kernel may kill sender process without NOSIGNAL.
        // Now, it will send EPIPE error
        let size = send(self.as_fd(), packet, SendFlags::NOSIGNAL)?;
        if size != packet.len() {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "WriteZero error"));
        }
        Ok(())
    }

    pub fn recv(&self, buffer: &mut [u8]) -> io::Result<usize> {
        let (initialized_len, received_len) = recv(&self.channel, buffer, RecvFlags::empty())?;
        debug_assert_eq!(initialized_len, received_len); // channel sends one packet with no truncation so both must be equal

        Ok(initialized_len)
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

// Gives the child only its endpoint on fd3 across exec
fn install_child_channel(
    parent_raw: RawFd, /* parent socket endpoint handle*/
    child_raw: RawFd,  /* child socket endpoint handle */
) -> io::Result<()> {
    let result = unsafe { close(parent_raw) };

    if result == -1 {
        return Err(io::Error::last_os_error());
    }

    if child_raw == CHILD_CHANNEL_FD {
        let flags = unsafe { fcntl(CHILD_CHANNEL_FD, F_GETFD) };
        if flags == -1 {
            return Err(io::Error::last_os_error());
        }

        // clear FD_CLOEXEC
        let result = unsafe { fcntl(CHILD_CHANNEL_FD, F_SETFD, flags & !FD_CLOEXEC) };
        if result == -1 {
            return Err(io::Error::last_os_error());
        }
    } else {
        let result = unsafe { dup3(child_raw, CHILD_CHANNEL_FD, 0) };
        if result == -1 {
            return Err(io::Error::last_os_error());
        }

        let result = unsafe { close(child_raw) };
        if result == -1 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
