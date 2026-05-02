mod core {
    pub mod error;
    pub mod types;
}
mod vm {
    pub mod state;
    pub mod memory;
    pub mod stack;
    pub mod execution;
}
mod compiler {
    pub mod parser;
}
mod optimizer {
    pub mod contract;
    pub mod segmentation;
    pub mod analysis;
}
mod jit {
    pub mod abi;
    pub mod runtime;
}
mod system {
    pub mod system;
}

use crate::core::error::{ForthResult, ForthError, ForthErrorKind, ForthPhase};
use crate::core::types::{Op, ExecutionStatus};
use crate::vm::state::Vm;
use crate::system::system::System;
use crate::compiler::parser::{Parser, Token};

#[inline(always)]
pub fn read_cycle_counter() -> u64 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    { unsafe { core::arch::x86_64::_rdtsc() } }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("Clock fail").as_nanos() as u64 }
}

impl Vm {
    pub fn run_loop(&mut self, sys: &mut System) -> ForthResult<()> {
        loop {
            match self.step_ex(sys) {
                Ok(ExecutionStatus::Done) => continue,
                Ok(ExecutionStatus::Stop) => break,
                Ok(ExecutionStatus::Yielded) => break,
                Ok(ExecutionStatus::Thrown(code)) => {
                    self.unwind_to_handler(code)?;
                }
                Err(e) => {
                    // Critical VM Error: Try to unwind or abort
                    if let Ok(_) = self.unwind_to_handler(-1) {
                        println!("Critical error trapped: {}", e.message);
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }
}

fn main() -> ForthResult<()> {
    println!("Kaiforth VM - Production Hardened Core");
    let mut sys = System::new()?;
    let mut vm = Vm::new()?;
    let mut _parser = Parser::try_new()?;
    
    // Minimal demonstration
    vm.run_loop(&mut sys)?;
    
    Ok(())
}
