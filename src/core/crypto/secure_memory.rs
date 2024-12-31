// src/core/crypto/secure_memory.rs

use std::alloc::{alloc, dealloc, Layout};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecureMemoryError {
    #[error("Failed to allocate secure memory")]
    AllocationFailed,
    #[error("Failed to lock memory pages")]
    LockFailed,
    #[error("Invalid memory alignment")]
    InvalidAlignment,
}

pub struct SecureMemory {
    ptr: *mut u8,
    layout: Layout,
    locked: AtomicBool,
}

impl SecureMemory {
    pub fn new(size: usize) -> Result<Self, SecureMemoryError> {
        // Ensure proper alignment for the platform
        let align = std::mem::align_of::<usize>();
        let layout = Layout::from_size_align(size, align)
            .map_err(|_| SecureMemoryError::InvalidAlignment)?;

        // Allocate memory
        let ptr = unsafe { alloc(layout) };
        if ptr.is_null() {
            return Err(SecureMemoryError::AllocationFailed);
        }

        // Lock memory pages to prevent swapping
        #[cfg(target_family = "unix")]
        unsafe {
            use libc::{mlock, ENOMEM};
            if mlock(ptr as *const _, size) == -1 {
                dealloc(ptr, layout);
                return Err(SecureMemoryError::LockFailed);
            }
        }

        #[cfg(target_family = "windows")]
        unsafe {
            use winapi::um::memoryapi::{VirtualLock};
            if VirtualLock(ptr as *mut _, size) == 0 {
                dealloc(ptr, layout);
                return Err(SecureMemoryError::LockFailed);
            }
        }

        Ok(Self {
            ptr,
            layout,
            locked: AtomicBool::new(true),
        })
    }

    pub fn write(&mut self, data: &[u8]) -> Result<(), SecureMemoryError> {
        if !self.locked.load(Ordering::SeqCst) {
            return Err(SecureMemoryError::LockFailed);
        }

        if data.len() > self.layout.size() {
            return Err(SecureMemoryError::InvalidAlignment);
        }

        unsafe {
            ptr::copy_nonoverlapping(data.as_ptr(), self.ptr, data.len());
        }

        Ok(())
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<(), SecureMemoryError> {
        if !self.locked.load(Ordering::SeqCst) {
            return Err(SecureMemoryError::LockFailed);
        }

        if buf.len() > self.layout.size() {
            return Err(SecureMemoryError::InvalidAlignment);
        }

        unsafe {
            ptr::copy_nonoverlapping(self.ptr, buf.as_mut_ptr(), buf.len());
        }

        Ok(())
    }

    pub fn clear(&mut self) {
        if self.locked.load(Ordering::SeqCst) {
            unsafe {
                ptr::write_bytes(self.ptr, 0, self.layout.size());
            }
        }
    }
}

impl Drop for SecureMemory {
    fn drop(&mut self) {
        self.clear();

        #[cfg(target_family = "unix")]
        unsafe {
            use libc::munlock;
            munlock(self.ptr as *const _, self.layout.size());
        }

        #[cfg(target_family = "windows")]
        unsafe {
            use winapi::um::memoryapi::VirtualUnlock;
            VirtualUnlock(self.ptr as *mut _, self.layout.size());
        }

        unsafe {
            dealloc(self.ptr, self.layout);
        }

        self.locked.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secure_memory_allocation() {
        let mem = SecureMemory::new(1024);
        assert!(mem.is_ok());
    }

    #[test]
    fn test_secure_memory_write_read() {
        let mut mem = SecureMemory::new(1024).unwrap();
        let data = b"test data";
        
        assert!(mem.write(data).is_ok());
        
        let mut buf = vec![0u8; data.len()];
        assert!(mem.read(&mut buf).is_ok());
        assert_eq!(&buf, data);
    }

    #[test]
    fn test_secure_memory_clear() {
        let mut mem = SecureMemory::new(1024).unwrap();
        let data = b"test data";
        
        mem.write(data).unwrap();
        mem.clear();
        
        let mut buf = vec![0u8; data.len()];
        mem.read(&mut buf).unwrap();
        assert_eq!(&buf, &vec![0u8; data.len()]);
    }
}