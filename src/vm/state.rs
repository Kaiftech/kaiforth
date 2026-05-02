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
        // Allocate 1024 cells (8KB) + 1 Guard Page (4KB)
        // Total 12KB
        let page_size = 4096;
        let stack_size = 1024 * 8; // 8KB
        let total_size = stack_size + page_size;

        let mut mmap = MmapOptions::new()
            .len(total_size)
            .map_anon()
            .map_err(|e| ForthError::new(ForthErrorKind::ExecutionStateCorrupted(e.to_string()), ForthPhase::Initialization, "Stack Allocation Failure"))?;

        // Protect the guard page (last page)
        // Note: memmap2 doesn't have a direct "protect" method on MmapMut easily for a subrange in a cross-platform way without unsafe or platform-specific calls.
        // However, we can use the 'mmap' crate's safety or just use the whole region as data for now, 
        // but the JIT will check depth. 
        // To truly satisfy the user's "Hardware protection" request on Windows:
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_NOACCESS};
            let mut old_protect = 0;
            unsafe {
                VirtualProtect(
                    mmap.as_ptr().add(stack_size) as *const _,
                    page_size,
                    PAGE_NOACCESS,
                    &mut old_protect
                );
            }
        }
        #[cfg(not(windows))]
        {
            unsafe {
                libc::mprotect(
                    mmap.as_ptr().add(stack_size) as *mut _,
                    page_size,
                    libc::PROT_NONE
                );
            }
        }

        Ok(Self {
            d_stack: mmap, d_depth: 0, f_stack: Vec::new(), r_stack: Vec::new(),
            c_stack: Vec::new(), loop_stack: Vec::new(), exception_stack: Vec::new(),
            ip: 0, in_tr: false,
        })
    }

    pub fn d_stack_ptr(&self) -> *mut i64 {
        self.d_stack.as_ptr() as *mut i64
    }
}
