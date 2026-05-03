use kaiforth::core::error::ForthResult;
use kaiforth::vm::state::Vm;
use kaiforth::system::system::System;
use kaiforth::compiler::parser::Parser;
use kaiforth::read_cycle_counter;

fn main() -> ForthResult<()> {
    println!("Kaiforth VM - Production Hardened Core");
    let mut sys = System::new(1024 * 1024)?;
    sys.register_core(); // Populate dictionary
    
    let mut vm = Vm::new()?;
    let mut parser = Parser::try_new()?;
    
    // Add stdin as source
    use kaiforth::compiler::parser::InputSource;
    use std::io::{self, Read};
    
    let mut buffer = String::new();
    println!("Type Forth code (Ctrl+D to finish):");
    io::stdin().read_to_string(&mut buffer).map_err(|_| kaiforth::core::error::ForthError::new(kaiforth::core::error::ForthErrorKind::ExecutionStateCorrupted, kaiforth::core::error::ForthPhase::Parsing))?;
    
    parser.input_stack.push(InputSource { text: buffer, ptr: 0 });
    
    let start = read_cycle_counter()?;
    vm.interpret_loop(&mut sys, &mut parser)?;
    vm.run_loop(&mut sys)?; // Run any remaining compiled code
    let end = read_cycle_counter()?;
    
    println!("Execution finished in {} cycles.", end.wrapping_sub(start));
    Ok(())
}
