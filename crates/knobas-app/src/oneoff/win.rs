//! A run's kill on Windows (ADR-0060, section 23): a job object. The program
//! is put in a job that kills every process in it when the job is closed or
//! ended, so a stop, the timeout or the quit ends `node` under `eslint.cmd`
//! and what `wsl.exe` started, which `taskkill /T` does not once a parent has
//! gone (ADR-0046 names that gap).
//!
//! The program is assigned to the job right after it starts; a child it
//! starts in that first instant is not in the job, and `taskkill /T` still
//! follows for that case. Owed a witness on Björn's PC.

use std::os::windows::io::AsRawHandle as _;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};

/// A job holding one run's processes.
pub struct Job(HANDLE);

// SAFETY: a job handle is a kernel handle, usable from any thread.
unsafe impl Send for Job {}
// SAFETY: as above; the calls made on it are thread-safe.
unsafe impl Sync for Job {}

impl Job {
    /// A job that kills its processes when closed, holding `child`.
    #[must_use]
    pub fn for_child(child: &std::process::Child) -> Option<Self> {
        // SAFETY: plain Win32 calls on handles this owns or the child's own.
        unsafe {
            let job = CreateJobObjectW(None, None).ok()?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let set = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&info).cast(),
                u32::try_from(std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
                    .unwrap_or(0),
            );
            let assigned = AssignProcessToJobObject(job, HANDLE(child.as_raw_handle()));
            if set.is_err() || assigned.is_err() {
                let _ = CloseHandle(job);
                return None;
            }
            Some(Self(job))
        }
    }

    /// Ends every process in the job.
    pub fn kill(&self) {
        // SAFETY: the handle is this job's own and still open.
        unsafe {
            let _ = TerminateJobObject(self.0, 1);
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: closed once, here; with KILL_ON_JOB_CLOSE this also ends
        // whatever is still in it.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
