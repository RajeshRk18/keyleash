#[cfg(not(target_os = "linux"))]
compile_error!("keyleash currently supports Linux only");

pub mod name;
pub mod process;
