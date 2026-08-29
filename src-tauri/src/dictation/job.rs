//! Ties a child process's lifetime to this one at the kernel level.
//!
//! Killing the whisper server when Asyar quits is easy; the hard case is
//! Asyar *not* quitting, but crashing or being force-killed. Nothing in the
//! process gets to run then, so no `Drop`, no shutdown hook, and no signal
//! handler can help. The server would keep running with the whole model
//! resident, which for `medium.en` is nearly 1.8 GB.
//!
//! A Windows job object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` moves the
//! job to the kernel: when the last handle to the job closes, every process
//! in it is terminated. Process teardown closes handles whatever the reason,
//! so this holds for a crash exactly as it does for a clean exit.
//!
//! There is no portable equivalent, so this is Windows-only. Elsewhere
//! `Job::new` returns `None` and the caller falls back to scanning for
//! orphans at startup.

#[cfg(windows)]
mod imp {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// A job object whose members die with this process.
    pub struct Job(HANDLE);

    // SAFETY: a job object handle is an ordinary kernel handle with no thread
    // affinity. Every call made on it below is documented as thread-safe, and
    // the handle is closed exactly once, in `Drop`.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Job {
        /// Creates an unnamed job that kills its members when it closes.
        ///
        /// Unnamed on purpose: a named job could be opened, and joined, by
        /// anything else on the machine.
        pub fn new() -> Option<Self> {
            // SAFETY: null attributes and a null name are the documented way
            // to ask for a default, unnamed job.
            let handle = unsafe { CreateJobObjectW(None, None) }
                .inspect_err(|e| crate::say!("[dictation] could not create a job object: {e}"))
                .ok()?;

            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            // SAFETY: the class and the struct match, and the length is that
            // struct's own size.
            let set = unsafe {
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    std::ptr::addr_of!(limits).cast(),
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if let Err(e) = set {
                crate::say!("[dictation] could not set the job object limit: {e}");
                // Without the limit this job guarantees nothing, and holding
                // it would only make the caller think otherwise.
                unsafe { CloseHandle(handle).ok() };
                return None;
            }

            Some(Self(handle))
        }

        /// Puts `child` in the job. Returns whether it worked.
        ///
        /// There is a window between the child being created and this call
        /// during which a process it spawned itself could escape the job.
        /// `whisper-server` spawns nothing, so the window is empty in
        /// practice; closing it properly would need `CREATE_SUSPENDED` and
        /// the child's thread handle, which `std::process::Command` does not
        /// expose.
        pub fn adopt(&self, child: &Child) -> bool {
            let process = HANDLE(child.as_raw_handle());
            // SAFETY: `child` is alive for this call, so its handle is valid,
            // and `self.0` is valid until `Drop`.
            match unsafe { AssignProcessToJobObject(self.0, process) } {
                Ok(()) => true,
                Err(e) => {
                    crate::say!("[dictation] could not add the whisper server to the job: {e}");
                    false
                }
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // Closing the last handle is what kills the members, so this is
            // the operative line, not just cleanup.
            // SAFETY: created above and closed only here.
            unsafe { CloseHandle(self.0).ok() };
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::process::Child;

    /// Stands in for the Windows job object. Always absent, which tells the
    /// caller to fall back to scanning for orphans.
    pub struct Job;

    impl Job {
        pub fn new() -> Option<Self> {
            None
        }

        pub fn adopt(&self, _child: &Child) -> bool {
            false
        }
    }
}

pub use imp::Job;

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    /// Spawns a child that would otherwise outlive us, then proves that
    /// letting go of the job is enough to kill it.
    ///
    /// This is the whole feature. Without it the only claim would be that
    /// some Win32 calls returned `Ok`, which is not the same as the process
    /// actually dying.
    #[test]
    fn closing_the_job_kills_what_it_holds() {
        let job = Job::new().expect("a job object should be creatable");

        // Long enough that it could not have exited on its own.
        let mut child = Command::new("cmd")
            .args(["/c", "ping -n 60 127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn");

        assert!(job.adopt(&child), "the child should join the job");
        assert!(
            child.try_wait().unwrap().is_none(),
            "the child should still be running before the job closes"
        );

        drop(job);

        let deadline = Instant::now() + Duration::from_secs(5);
        let exited = loop {
            match child.try_wait() {
                Ok(Some(_)) => break true,
                _ if Instant::now() >= deadline => break false,
                _ => std::thread::sleep(Duration::from_millis(20)),
            }
        };

        if !exited {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the child outlived the job it was assigned to");
        }
    }

    #[test]
    fn two_jobs_can_exist_at_once() {
        // One job per `WhisperServer`, and tests construct several. A named
        // job would collide here; an unnamed one cannot.
        let a = Job::new().expect("first");
        let b = Job::new().expect("second");
        drop((a, b));
    }
}
