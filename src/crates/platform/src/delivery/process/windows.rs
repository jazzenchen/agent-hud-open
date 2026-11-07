//! Child processes on Windows: pipes read only as far as `PeekNamedPipe`
//! reports, and a job object that ends a server's descendants with it.
use super::*;
use std::{
    io::Read,
    process::{Child, ChildStderr, ChildStdout},
};

/// Windows pipes stay blocking: `drain` reads only what is already there.
pub(super) fn prepare(
    _child: &mut Child,
    _stdout: &ChildStdout,
    _stderr: &ChildStderr,
) -> Result<()> {
    Ok(())
}

/// A temporary app-server may be launched by an npm cmd/Node shim. Closing its
/// job terminates the shim and descendants; killing only Child would leave the
/// server behind. Assign immediately after spawn, before any protocol request.
pub(super) struct Job(std::os::windows::io::OwnedHandle);

impl Job {
    pub(super) fn new() -> Result<Self> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                handle.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self(handle))
    }

    pub(super) fn assign(&self, child: &Child) -> Result<()> {
        use std::os::windows::io::AsRawHandle;
        if unsafe {
            windows_sys::Win32::System::JobObjects::AssignProcessToJobObject(
                self.0.as_raw_handle(),
                child.as_raw_handle(),
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
}

pub(super) fn drain(
    pipe: &mut (impl Read + std::os::windows::io::AsRawHandle),
    bytes: &mut Vec<u8>,
) -> Result<()> {
    use windows_sys::Win32::{Foundation::ERROR_BROKEN_PIPE, System::Pipes::PeekNamedPipe};
    let mut chunk = [0u8; 8192];
    // Anonymous child pipes support PeekNamedPipe. Never block on EOF: a
    // detached background client can retain the inherited write handle.
    for _ in 0..64 {
        let mut available = 0;
        let ready = unsafe {
            PeekNamedPipe(
                pipe.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if ready == 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                break;
            }
            return Err(error.into());
        }
        if available == 0 {
            break;
        }
        let limit = (available as usize).min(chunk.len());
        let size = pipe.read(&mut chunk[..limit])?;
        if size == 0 {
            break;
        }
        if bytes.len() < 256 * 1024 {
            bytes.extend_from_slice(&chunk[..size.min(256 * 1024 - bytes.len())]);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_app_server_job_terminates_descendants() {
        use std::io::BufRead;
        use std::os::windows::{
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
            process::CommandExt,
        };
        use windows_sys::Win32::{
            Foundation::WAIT_OBJECT_0,
            System::Threading::{
                OpenProcess, WaitForSingleObject, CREATE_NO_WINDOW, PROCESS_SYNCHRONIZE,
            },
        };
        let job = Job::new().unwrap();
        let executable = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut child = Command::new(executable).args(["-NoProfile", "-NonInteractive", "-Command",
            "$child=Start-Process $env:ComSpec -ArgumentList '/c ping -n 30 127.0.0.1 >nul' -WindowStyle Hidden -PassThru; [Console]::Out.WriteLine($child.Id); Start-Sleep -Seconds 30"])
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).creation_flags(CREATE_NO_WINDOW).spawn().unwrap();
        job.assign(&child).unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let _ = std::io::BufReader::new(stdout).read_line(&mut line);
            let _ = sender.send(line);
        });
        let pid: u32 = receiver
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        assert!(!handle.is_null());
        let descendant = unsafe { OwnedHandle::from_raw_handle(handle) };
        drop(job);
        assert_eq!(
            unsafe { WaitForSingleObject(descendant.as_raw_handle(), 10_000) },
            WAIT_OBJECT_0
        );
        assert_eq!(
            unsafe { WaitForSingleObject(child.as_raw_handle(), 10_000) },
            WAIT_OBJECT_0
        );
        let _ = child.wait();
    }

    #[test]
    fn captures_pipes_and_stops_at_deadline() {
        let executable = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let plan = |script: &str, timeout| CommandPlan {
            executable: executable.clone(),
            arguments: vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                script.into(),
            ],
            directory: None,
            environment: BTreeMap::new(),
            timeout,
        };
        let output = NativeProcessRunner
            .run(&plan(
                "[Console]::Out.Write('out'); [Console]::Error.Write('err')",
                Duration::from_secs(10),
            ))
            .unwrap();
        assert_eq!(output.stdout, b"out");
        assert_eq!(output.stderr, b"err");
        assert_eq!(output.code, Some(0));
        let started = Instant::now();
        let output = NativeProcessRunner
            .run(&plan("Start-Sleep -Seconds 30", Duration::from_millis(500)))
            .unwrap();
        assert!(output.timed_out);
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
