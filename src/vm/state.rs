use memmap2::{MmapMut, MmapOptions};
use crate::core::error::{ForthResult, ForthError, ForthErrorKind, ForthPhase};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Frame { pub ret_ip: usize, pub word_idx: usize, pub ret_in_tr: bool }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatchFrame {
    pub d_depth: usize,
    pub r_depth: usize,
    pub c_depth: usize,
    pub handler_ip: usize,
    pub in_tr: bool,
}

pub const D_STACK_START_OFFSET: usize = 4096;
pub const D_STACK_SIZE: usize = 1024 * 8;
pub const CANARY_VALUE: u64 = 0xDEADBEEFCAFEBABE;

/// The Virtual Machine execution state.
/// 
/// Contains all stacks and the instruction pointer. The data stack (`d_stack`)
/// is backed by a guarded `mmap` region for maximum security.
pub struct Vm {
    /// Guarded data stack (mmap).
    pub d_stack: MmapMut,
    /// Current depth of the data stack.
    pub d_depth: usize,
    /// Floating point stack.
    pub f_stack: Vec<f64>,
    /// Return stack (for return addresses and temporary storage).
    pub r_stack: Vec<i64>,
    /// Call stack (for tracking word call nesting).
    pub c_stack: Vec<Frame>,
    /// Loop stack (for DO/LOOP parameters).
    pub loop_stack: Vec<i64>,
    /// Exception stack (for CATCH/THROW handlers).
    pub exception_stack: Vec<CatchFrame>,
    /// Execution path trace for JIT optimization.
    pub path_trace: Vec<u64>,
    /// Instruction pointer.
    pub ip: usize,
    /// Whether currently executing inside a JIT-compiled trace.
    pub in_tr: bool,
}

impl Vm {
    pub fn new() -> ForthResult<Self> {
        let page_size = 4096;
        let stack_size = D_STACK_SIZE; 
        let total_size = stack_size + (page_size * 2);

        let mut mmap = MmapOptions::new()
            .len(total_size)
            .map_anon()
            .map_err(|_| ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Initialization))?;

        // Initialize canaries at top and bottom of data stack
        unsafe {
            let base = mmap.as_mut_ptr().add(page_size) as *mut u64;
            *base = CANARY_VALUE;
            let top = mmap.as_mut_ptr().add(page_size + stack_size - 8) as *mut u64;
            *top = CANARY_VALUE;
        }

        #[cfg(windows)]
        {
            unsafe {
                unsafe extern "system" {
                    fn VirtualProtect(lpAddress: *const std::ffi::c_void, dwSize: usize, flNewProtect: u32, lpflOldProtect: *mut u32) -> i32;
                }
                const PAGE_NOACCESS: u32 = 0x01;
                let mut old_protect = 0;
                if VirtualProtect(mmap.as_ptr() as *const _, page_size, PAGE_NOACCESS, &mut old_protect) == 0 {
                    return Err(ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Initialization));
                }
                if VirtualProtect(mmap.as_ptr().add(stack_size + page_size) as *const _, page_size, PAGE_NOACCESS, &mut old_protect) == 0 {
                    return Err(ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Initialization));
                }
            }
        }
        #[cfg(not(windows))]
        {
            unsafe {
                if libc::mprotect(mmap.as_ptr() as *mut _, page_size, libc::PROT_NONE) != 0 {
                    return Err(ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Initialization));
                }
                if libc::mprotect(mmap.as_ptr().add(stack_size + page_size) as *mut _, page_size, libc::PROT_NONE) != 0 {
                    return Err(ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Initialization));
                }
            }
        }

        Ok(Self {
            d_stack: mmap, d_depth: 0, f_stack: Vec::with_capacity(128), r_stack: Vec::with_capacity(128),
            c_stack: Vec::with_capacity(64), loop_stack: Vec::with_capacity(64), exception_stack: Vec::with_capacity(16),
            path_trace: Vec::with_capacity(256), ip: 0, in_tr: false,
        })
    }

    pub fn d_stack_ptr(&self) -> *mut i64 {
        // Skip the first 8 bytes (bottom canary)
        unsafe { self.d_stack.as_ptr().add(D_STACK_START_OFFSET + 8) as *mut i64 }
    }

    pub fn verify_canaries(&self) -> ForthResult<()> {
        unsafe {
            let base = self.d_stack.as_ptr().add(D_STACK_START_OFFSET) as *const u64;
            if *base != CANARY_VALUE { return Err(ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Execution)); }
            let top = self.d_stack.as_ptr().add(D_STACK_START_OFFSET + D_STACK_SIZE - 8) as *const u64;
            if *top != CANARY_VALUE { return Err(ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Execution)); }
        }
        Ok(())
    }

    pub fn try_clone(&self) -> ForthResult<Self> {
        let page_size = 4096;
        let stack_size = D_STACK_SIZE;
        let total_size = stack_size + (page_size * 2);

        let mut new_stack = MmapOptions::new()
            .len(total_size)
            .map_anon()
            .map_err(|_| ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Initialization))?;
        
        new_stack[page_size..page_size + stack_size].copy_from_slice(&self.d_stack[page_size..page_size + stack_size]);

        #[cfg(windows)]
        {
            unsafe {
                unsafe extern "system" {
                    fn VirtualProtect(lpAddress: *const std::ffi::c_void, dwSize: usize, flNewProtect: u32, lpflOldProtect: *mut u32) -> i32;
                }
                const PAGE_NOACCESS: u32 = 0x01;
                let mut old_protect = 0;
                let _ = VirtualProtect(new_stack.as_ptr() as *const _, page_size, PAGE_NOACCESS, &mut old_protect);
                let _ = VirtualProtect(new_stack.as_ptr().add(stack_size + page_size) as *const _, page_size, PAGE_NOACCESS, &mut old_protect);
            }
        }
        #[cfg(not(windows))]
        {
            unsafe {
                let _ = libc::mprotect(new_stack.as_ptr() as *mut _, page_size, libc::PROT_NONE);
                let _ = libc::mprotect(new_stack.as_ptr().add(stack_size + page_size) as *mut _, page_size, libc::PROT_NONE);
            }
        }

        Ok(Self {
            d_stack: new_stack,
            d_depth: self.d_depth,
            f_stack: self.f_stack.clone(),
            r_stack: self.r_stack.clone(),
            c_stack: self.c_stack.clone(),
            loop_stack: self.loop_stack.clone(),
            exception_stack: self.exception_stack.clone(),
            path_trace: self.path_trace.clone(),
            ip: self.ip,
            in_tr: self.in_tr,
        })
    }
}

