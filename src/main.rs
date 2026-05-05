/*
 * Copyright (c) 2026 kaif(kaiftech)
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use kaiforth::core::error::ForthResult;
use kaiforth::vm::state::Vm;
use kaiforth::system::system::System;
use kaiforth::compiler::parser::Parser;
use kaiforth::core::types::ExecutionStatus;

fn main() -> ForthResult<()> {
    let args: Vec<String> = std::env::args().collect();
    
    let mut sys = System::new(1024 * 1024)?;
    sys.register_core(); 
    
    let mut vm = Vm::new()?;
    let mut parser = Parser::try_new()?;
    
    use kaiforth::compiler::parser::InputSource;
    use std::io::{self, Write, Read};

    if args.len() > 1 {
        // Run from file
        let path = &args[1];
        let mut file = std::fs::File::open(path).map_err(|_| kaiforth::core::error::ForthError::new(kaiforth::core::error::ForthErrorKind::ExecutionStateCorrupted, kaiforth::core::error::ForthPhase::Parsing))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).map_err(|_| kaiforth::core::error::ForthError::new(kaiforth::core::error::ForthErrorKind::ExecutionStateCorrupted, kaiforth::core::error::ForthPhase::Parsing))?;
        
        parser.input_stack.push(InputSource { text: contents, ptr: 0 });
        vm.interpret_loop(&mut sys, &mut parser)?;
        vm.run_loop(&mut sys)?;
    } else {
        // Interactive REPL
        println!("Kaiforth VM - Production Hardened Core");
        println!("Interactive Mode. Type 'bye' or press Ctrl+D to exit.");
        
        loop {
            if sys.compiling {
                print!("  compiling> ");
            } else {
                print!("ok> ");
            }
            io::stdout().flush().unwrap();
            
            let mut line = String::new();
            let bytes_read = io::stdin().read_line(&mut line).map_err(|_| kaiforth::core::error::ForthError::new(kaiforth::core::error::ForthErrorKind::ExecutionStateCorrupted, kaiforth::core::error::ForthPhase::Parsing))?;
            
            // EOF or manual Ctrl+D
            if bytes_read == 0 || line.contains('\x04') {
                println!();
                break;
            }
            
            parser.input_stack.push(InputSource { text: line, ptr: 0 });
            
            match vm.interpret_loop(&mut sys, &mut parser) {
                Ok(ExecutionStatus::Stop) => break,
                Ok(_) => {
                    if let Err(e) = vm.run_loop(&mut sys) {
                        eprintln!("\nExecution Error: {:?}", e.kind);
                    }
                }
                Err(e) => {
                    eprintln!("\nError: {:?}", e.kind);
                }
            }
        }
    }
    
    Ok(())
}

