use keyleash::process::SpawnedChild;
use rustix::net::{RecvFlags, SendFlags, recv, send};
use std::env;
use std::io;
use std::os::fd::BorrowedFd;
use std::path::PathBuf;
use std::process::Command;

const PROBE_ENV: &str = "KEYLEASH_FD3_PROBE";

#[test]
fn fd3_probe_child() -> io::Result<()> {
    if env::var_os(PROBE_ENV).is_none() {
        return Ok(());
    }

    let target = std::fs::read_link("/proc/self/fd/3")?;
    assert!(target.to_string_lossy().starts_with("socket:["));

    if env::var(PROBE_ENV).as_deref() == Ok("packet") {
        // SAFETY: SpawnedChild installs a live socket endpoint at fd 3 for
        // this exact probe process. BorrowedFd does not take ownership.
        let channel = unsafe { BorrowedFd::borrow_raw(3) };
        let mut packet = [0_u8; 16];
        let (initialized_len, received_len) = recv(channel, &mut packet, RecvFlags::empty())?;
        assert_eq!(initialized_len, received_len);
        assert_eq!(&packet[..initialized_len], b"ping");
        assert_eq!(send(channel, b"pong", SendFlags::NOSIGNAL)?, 4);
    }

    Ok(())
}

#[test]
fn launcher_installs_child_channel_on_fd3() -> io::Result<()> {
    let mut command = Command::new(env::current_exe()?);
    command
        .args(["--exact", "fd3_probe_child", "--nocapture"])
        .env(PROBE_ENV, "1");

    let mut spawned = SpawnedChild::spawn(command)?;
    let fd3 = PathBuf::from(format!("/proc/{}/fd/3", spawned.id()));
    assert!(
        std::fs::read_link(fd3)?
            .to_string_lossy()
            .starts_with("socket:[")
    );
    assert!(spawned.wait()?.success());
    Ok(())
}

#[test]
fn launcher_exchanges_packet_and_observes_eof() -> io::Result<()> {
    let mut command = Command::new(env::current_exe()?);
    command
        .args(["--exact", "fd3_probe_child", "--nocapture"])
        .env(PROBE_ENV, "packet");

    let mut spawned = SpawnedChild::spawn(command)?;
    spawned.send(b"ping")?;

    let mut packet = [0_u8; 16];
    let received = spawned.recv(&mut packet)?;
    assert_eq!(&packet[..received], b"pong");

    assert!(spawned.wait()?.success());
    assert_eq!(spawned.recv(&mut packet)?, 0); // EOF
    Ok(())
}
