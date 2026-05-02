pub struct JitContext {
    pub magic: u64, // 0x4B4149464F525448
    pub d_stack_base: *mut i64,
    pub d_depth: *mut usize,
    pub memory_base: *mut u8,
    pub memory_limit: usize,
    pub trap_code: i32,
    pub trap_ip: usize,
}

pub type JitFunc = unsafe extern "C" fn(*mut JitContext);

#[cfg(windows)]
pub type JitFuncAbi = unsafe extern "win64" fn(*mut JitContext);
#[cfg(not(windows))]
pub type JitFuncAbi = unsafe extern "sysv64" fn(*mut JitContext);

pub unsafe fn call_jit(func_ptr: *const u8, ctx: &mut JitContext) {
    let func: JitFuncAbi = std::mem::transmute(func_ptr);
    func(ctx);
}
