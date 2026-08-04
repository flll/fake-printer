//! Single-instance guard keyed on the IPP port.
//!
//! Without it a second process still publishes its mDNS records before failing
//! to bind the port, so clients see two conflicting advertisements for the same
//! service name and the printer disappears from their list, while the first,
//! healthy process keeps running invisibly.

/// Held for the lifetime of the process; releases the lock on drop.
pub struct InstanceLock(#[allow(dead_code)] imp::Handle);

/// Returns `None` when another fake-printer already owns `port`.
pub fn acquire(port: u16) -> Option<InstanceLock> {
    imp::acquire(&format!("Local\\fake-printer-ipp-{port}")).map(InstanceLock)
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    pub struct Handle(HANDLE);

    // The handle is only closed on drop; no cross-thread mutation.
    unsafe impl Send for Handle {}

    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    pub fn acquire(name: &str) -> Option<Handle> {
        let wide: Vec<u16> = OsStr::new(name).encode_wide().chain(std::iter::once(0)).collect();
        let handle = unsafe { CreateMutexW(std::ptr::null(), 1, wide.as_ptr()) };
        if handle.is_null() {
            // Cannot determine ownership; start anyway rather than refuse to run.
            return Some(Handle(std::ptr::null_mut()));
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe { CloseHandle(handle) };
            return None;
        }
        Some(Handle(handle))
    }
}

#[cfg(not(windows))]
mod imp {
    pub struct Handle;

    pub fn acquire(_name: &str) -> Option<Handle> {
        Some(Handle)
    }
}
