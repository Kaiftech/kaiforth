use memmap2::{MmapMut, MmapOptions};
use crate::core::error::{ForthResult, ForthError, ForthErrorKind, ForthPhase};

pub struct Frame { pub ret_ip: usize, pub word_idx: usize, pub ret_in_tr: bool }

#[derive(Clone, Copy)]
pub struct CatchFrame {
    pub d_depth: usize,
    pub r_depth: usize,
    pub c_depth: usize,
    pub handler_ip: usize,
    pub in_tr: bool,
}

pub struct Vm {
    pub d_stack: MmapMut,
    pub d_depth: usize,
    pub f_stack: Vec<f64>,
    pub r_stack: Vec<i64>,
    pub c_stack: Vec<Frame>,
    pub loop_stack: Vec<i64>,
    pub exception_stack: Vec<CatchFrame>,
    pub ip: usize,
    pub in_tr: bool,
}

impl Vm {
    pub fn new() -> ForthResult<Self> {
        let page_size = 4096;
        let stack_size = 1024 * 8; // 8KB (2 pages)
        // Total 4 pages: [Guard][Data][Data][Guard]
        let total_size = stack_size + (page_size * 2);

        let mut mmap = MmapOptions::new()
            .len(total_size)
            .map_anon()
            .map_err(|e| ForthError::new(ForthErrorKind::ExecutionStateCorrupted(e.to_string()), ForthPhase::Initialization, "Stack Allocation Failure"))?;

        // Protect the guard pages (first and last)
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_NOACCESS};
            let mut old_protect = 0;
            unsafe {
                // Leading guard
                VirtualProtect(mmap.as_ptr() as *const _, page_size, PAGE_NOACCESS, &mut old_protect);
                // Trailing guard
                VirtualProtect(mmap.as_ptr().add(stack_size + page_size) as *const _, page_size, PAGE_NOACCESS, &mut old_protect);
            }
        }
        #[cfg(not(windows))]
        {
            unsafe {
                libc::mprotect(mmap.as_ptr() as *mut _, page_size, libc::PROT_NONE);
                libc::mprotect(mmap.as_ptr().add(stack_size + page_size) as *mut _, page_size, libc::PROT_NONE);
            }
        }

        Ok(Self {
            d_stack: mmap, d_depth: 0, f_stack: Vec::new(), r_stack: Vec::new(),
            c_stack: Vec::new(), loop_stack: Vec::new(), exception_stack: Vec::new(),
            ip: 0, in_tr: false,
        })
    }

    pub fn d_stack_ptr(&self) -> *mut i64 {
        // Offset by page_size to reach the data region
        unsafe { (self.d_stack.as_ptr() as *mut i64).add(4096 / 8) }
    }
}
