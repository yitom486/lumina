//! Spawn external CLI tools without flashing a console on Windows.

use std::ffi::OsStr;
use std::process::Command;

pub fn command<P: AsRef<OsStr>>(program: P) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
