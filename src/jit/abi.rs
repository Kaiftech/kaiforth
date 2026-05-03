use crate::core::error::{ForthResult, ForthError, ForthErrorKind, ForthPhase};

#[repr(C)]
pub struct JitContext {
    pub magic: u64,                 // (0)
    pub version: u64,               // (8)
    pub struct_size: u64,           // (16)
    pub d_stack_ptr: *mut i64,      // (24)
    pub d_stack_limit: u64,         // (32)
    pub d_depth: u64,              // (40)
    pub r_stack_ptr: *mut i64,      // (48)
    pub r_depth: u64,              // (56)
    pub memory_ptr: *mut u8,        // (64)
    pub memory_limit: u64,         // (72)
    pub journal_ptr: *mut u64,      // (80)
    pub journal_len: u64,          // (88)
    pub journal_cap: u64,          // (96)
    pub trap_ip: u64,              // (104)
    pub trap_code: u64,            // (112)
    pub writes_occurred: u64,      // (120)
    pub trap_addr: u64,            // (128)
    pub trap_target: u64,          // (136)
    pub loop_stack_ptr: *mut i64,  // (144)
    pub loop_stack_depth: u64,     // (152)
    pub d_canary: u64,             // (160)
    pub r_canary: u64,             // (168)
    pub l_canary: u64,             // (176)
    pub path_trace_ptr: *mut u64,  // (184)
    pub path_trace_len: u64,       // (192)
    pub path_trace_cap: u64,       // (200)
    pub trap_instruction_idx: u64, // (208)
    pub r_stack_limit: u64,        // (216)
    pub loop_stack_limit: u64,     // (224)
    pub exception_stack_ptr: *mut i64, // (232)
    pub exception_stack_depth: u64,    // (240)
    pub r_stack_limit_real: u64,      // (248)
    pub loop_stack_limit_real: u64,   // (256)
}

#[cfg(all(target_arch = "x86_64", windows))]
pub type JitFuncAbi = unsafe extern "win64" fn(*mut JitContext);
#[cfg(all(target_arch = "x86_64", not(windows)))]
pub type JitFuncAbi = unsafe extern "sysv64" fn(*mut JitContext);
#[cfg(not(target_arch = "x86_64"))]
pub type JitFuncAbi = unsafe extern "C" fn(*mut JitContext);

impl JitContext {
    pub fn validate(&self) -> ForthResult<()> {
        if self.magic != 0x4B4149464F525448 || self.struct_size != std::mem::size_of::<JitContext>() as u64 {
            return Err(ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Execution));
        }
        // Pointer sanity: ensure they are not null if capacity > 0
        if self.d_stack_ptr.is_null() || self.r_stack_ptr.is_null() || self.memory_ptr.is_null() {
            return Err(ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Execution));
        }
        Ok(())
    }
}

pub unsafe fn call_jit(func_ptr: *const u8, ctx: &mut JitContext) -> ForthResult<()> {
    // Safety: We must never execute x86_64 machine code on non-x86_64 targets.
    // This guard is the last line of defence before SIGILL.
    if !cfg!(target_arch = "x86_64") {
        return Err(ForthError::new(ForthErrorKind::OptimizationFailed, ForthPhase::Execution));
    }

    if func_ptr.is_null() || (func_ptr as usize) % 16 != 0 {
        return Err(ForthError::new(ForthErrorKind::ExecutionStateCorrupted, ForthPhase::Execution));
    }
    
    // Pre-call validation
    ctx.validate()?;
    
    // ABI Boundary Assertion
    debug_assert_eq!((ctx as *const _ as usize) % 8, 0, "JitContext must be 8-byte aligned");
    
    unsafe {
        let func: JitFuncAbi = std::mem::transmute(func_ptr);
        func(ctx);
    }
    
    // Post-call validation (Trap check & structural integrity)
    ctx.validate()?;
    
    Ok(())
}

