use std::{
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

pub(crate) fn capture(command: &mut Command, timeout: Duration) -> Result<Output, &'static str> {
    crate::launch_environment::apply(command);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "could not start executable")?;
    let result = (|| {
        let mut stdout = child.stdout.take().ok_or("output is unavailable")?;
        let mut stderr = child.stderr.take().ok_or("error output is unavailable")?;
        nonblocking(&stdout)?;
        nonblocking(&stderr)?;
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let deadline = Instant::now() + timeout;
        loop {
            let status = child
                .try_wait()
                .map_err(|_| "could not wait for executable")?;
            read_version_output(&mut stdout, &mut output)?;
            read_version_output(&mut stderr, &mut errors)?;
            if output.len() + errors.len() > 4096 {
                return Err("version output exceeds the size limit");
            }
            if let Some(status) = status {
                return Ok(Output {
                    status,
                    stdout: output,
                    stderr: errors,
                });
            }
            if Instant::now() >= deadline {
                return Err("version check timed out");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    if result.is_err() {
        if let Err(error) = child.kill()
            && error.kind() != std::io::ErrorKind::InvalidInput
        {
            crate::application_log::failure("process_probe_cleanup", error.to_string());
        }
        if let Err(error) = child.wait() {
            crate::application_log::failure("process_probe_cleanup", error.to_string());
        }
    }
    result
}

fn nonblocking(pipe: &impl std::os::fd::AsRawFd) -> Result<(), &'static str> {
    let fd = pipe.as_raw_fd();
    // SAFETY: fd is borrowed from a live child pipe; these commands read and set integer flags.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err("could not configure version output");
    }
    Ok(())
}

fn read_version_output(
    pipe: &mut impl std::io::Read,
    output: &mut Vec<u8>,
) -> Result<(), &'static str> {
    let mut buffer = [0_u8; 1024];
    loop {
        match pipe.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(size) => {
                if output.len() + size > 4096 {
                    return Err("version output exceeds the size limit");
                }
                output.extend_from_slice(&buffer[..size]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err("could not read version output"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_probe_times_out_even_if_a_descendant_keeps_its_pipe_open() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 0.5 & exit 0"]);
        let start = Instant::now();
        let output = capture(&mut command, Duration::from_millis(50)).unwrap();
        assert!(output.status.success());
        assert!(start.elapsed() < Duration::from_millis(400));
        let mut slow = Command::new("/bin/sleep");
        slow.arg("1");
        assert!(capture(&mut slow, Duration::from_millis(30)).is_err());
    }
}
