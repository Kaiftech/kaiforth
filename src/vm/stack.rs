use crate::vm::state::Vm;
use crate::core::error::{ForthResult, ForthError, ForthErrorKind, ForthPhase};

impl Vm {
    #[inline(always)]
    pub fn d_push(&mut self, val: i64) -> ForthResult<()> {
        if self.d_depth >= 1024 {
            return Err(ForthError::new(ForthErrorKind::Abort("Data Stack Overflow".into()), ForthPhase::Execution, "Stack Fault"));
        }
        let ptr = self.d_stack_ptr();
        unsafe {
            *ptr.add(self.d_depth) = val;
        }
        self.d_depth += 1;
        Ok(())
    }

    #[inline(always)]
    pub fn d_pop(&mut self) -> ForthResult<i64> {
        if self.d_depth == 0 {
            return Err(ForthError::new(ForthErrorKind::StackUnderflow { exp: 1, found: 0 }, ForthPhase::Execution, "Stack Fault"));
        }
        self.d_depth -= 1;
        let ptr = self.d_stack_ptr();
        unsafe {
            Ok(*ptr.add(self.d_depth))
        }
    }

    pub fn d_pop2(&mut self) -> ForthResult<(i64, i64)> {
        let b = self.d_pop()?;
        let a = self.d_pop()?;
        Ok((a, b))
    }

    pub fn f_push(&mut self, val: f64) -> ForthResult<()> {
        self.f_stack.push(val); Ok(())
    }

    pub fn f_pop(&mut self) -> ForthResult<f64> {
        self.f_stack.pop().ok_or_else(|| ForthError::new(ForthErrorKind::StackUnderflow { exp: 1, found: 0 }, ForthPhase::Execution, "Float Stack Fault"))
    }
}
