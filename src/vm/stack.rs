use crate::vm::state::Vm;
use crate::core::error::{ForthResult, ForthError, ForthErrorKind, ForthPhase};

impl Vm {
    #[inline(always)]
    pub fn d_push(&mut self, val: i64) -> ForthResult<()> {
        if self.d_depth >= 1022 {
            return Err(ForthError::new(ForthErrorKind::StackOverflow, ForthPhase::Execution));
        }
        let ptr = self.d_stack_ptr();
        unsafe {
            // Safety: d_depth < 1024, and stack region is 1024 elements.
            // Leading guard is at 0..512. Data is at 512..1536.
            // ptr is 512. ptr.add(1023) is 1535 (last valid element).
            *ptr.add(self.d_depth) = val;
        }
        self.d_depth += 1;
        Ok(())
    }

    #[inline(always)]
    pub fn d_pop(&mut self) -> ForthResult<i64> {
        if self.d_depth == 0 {
            return Err(ForthError::new(ForthErrorKind::StackUnderflow, ForthPhase::Execution));
        }
        self.d_depth -= 1;
        let ptr = self.d_stack_ptr();
        unsafe {
            Ok(*ptr.add(self.d_depth))
        }
    }

    #[inline(always)]
    pub fn r_push(&mut self, val: i64) -> ForthResult<()> {
        if self.r_stack.len() >= 1024 {
            return Err(ForthError::new(ForthErrorKind::ReturnStackOverflow, ForthPhase::Execution));
        }
        self.r_stack.push(val);
        Ok(())
    }

    #[inline(always)]
    pub fn r_pop(&mut self) -> ForthResult<i64> {
        self.r_stack.pop().ok_or_else(|| ForthError::new(ForthErrorKind::ReturnStackUnderflow, ForthPhase::Execution))
    }

    #[inline(always)]
    pub fn r_fetch(&mut self) -> ForthResult<i64> {
        self.r_stack.last().copied().ok_or_else(|| ForthError::new(ForthErrorKind::ReturnStackUnderflow, ForthPhase::Execution))
    }

    pub fn f_push(&mut self, val: f64) -> ForthResult<()> {
        if self.f_stack.len() >= 1024 {
            return Err(ForthError::new(ForthErrorKind::StackOverflow, ForthPhase::Execution));
        }
        self.f_stack.push(val); 
        Ok(())
    }

    pub fn f_pop(&mut self) -> ForthResult<f64> {
        self.f_stack.pop().ok_or_else(|| ForthError::new(ForthErrorKind::StackUnderflow, ForthPhase::Execution))
    }
}

