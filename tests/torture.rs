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

use kaiforth::core::types::{Op, ExecutionStatus};
use kaiforth::system::system::System;
use kaiforth::vm::state::Vm;
use rand::Rng;

#[test]
fn test_jit_torture_random_bytecode() {
    let mut sys = System::new(1024 * 1024).expect("System Init Fail");
    sys.paranoid_mode = true; // FORCE DIFFERENTIAL TESTING
    sys.trace_enabled = true; // ENABLE OPTIMIZER FEEDBACK
    let mut vm = Vm::new().expect("VM Init Fail");

    let mut rng = rand::thread_rng();

    // Torture for 200 iterations of random blocks
    for _iteration in 0..200 {
        sys.code.clear();
        let segment_len = rng.gen_range(10..40);
        
        // Ensure some initial data for Store/Fetch ops
        for _ in 0..10 {
            sys.code.push(Op::Push, rng.gen_range(0..100) as u64);
        }

        for _ in 0..segment_len {
            let op_type = rng.gen_range(0..12);
            match op_type {
                0 => sys.code.push(Op::Push, rng.gen_range(0..1000)),
                1 => sys.code.push(Op::Add, 0),
                2 => sys.code.push(Op::Sub, 0),
                3 => sys.code.push(Op::Mul, 0),
                4 => sys.code.push(Op::Dup, 0),
                5 => sys.code.push(Op::Drop, 0),
                6 => sys.code.push(Op::Swap, 0),
                7 => {
                    // SAFE STORE GENERATOR
                    sys.code.push(Op::Push, rng.gen_range(0..1000)); // val
                    sys.code.push(Op::Push, rng.gen_range(0..512) * 8); // aligned addr
                    sys.code.push(Op::Store, 0);
                },
                8 => {
                    // SAFE FETCH GENERATOR
                    sys.code.push(Op::Push, rng.gen_range(0..512) * 8);
                    sys.code.push(Op::Fetch, 0);
                },
                9 => sys.code.push(Op::Over, 0),
                10 => sys.code.push(Op::Rot, 0),
                11 => sys.code.push(Op::Noop, 0),
                _ => unreachable!()
            }
        }
        sys.code.push(Op::Stop, 0);

        // Run until completion or error
        for _run in 0..110 {
            vm.ip = 0;
            vm.d_depth = 0;
            vm.r_stack.clear();
            vm.loop_stack.clear();
            
            if sys.optimizer.should_run_optimizer() || _run == 0 {
                sys.optimizer.observe_runtime_traces(&mut sys.runtime_trace);
                let _ = sys.synchronize_jit();
            }

            // EQUIVALENCE ASSERTION is handled by sys.paranoid_mode internally.
            // If it fails, sys.jit_poisoned increments and it rollbacks.
            match vm.step_ex(&mut sys) {
                Ok(ExecutionStatus::Stop) => break,
                Ok(_) => continue,
                Err(_) => break, // Expected in random torture
            }
        }
    }

    println!("Torture Suite Finished.");
    println!("JIT Hits: {}", sys.jit_hits);
    println!("JIT Poisoned: {}", sys.jit_poisoned);
    println!("JIT Rollbacks: {}", sys.jit_rollbacks);
}

#[test]
fn test_loop_stack_torture() {
    let mut sys = System::new(1024 * 1024).expect("System Init Fail");
    sys.paranoid_mode = true;
    sys.trace_enabled = true; // REQUIRED FOR JIT
    let mut vm = Vm::new().expect("VM Init Fail");

    // Test recursive JIT entry with loop stacks
    sys.code.push(Op::Push, 10); // limit
    sys.code.push(Op::Push, 0);  // index
    sys.code.push(Op::Do, 0);
    sys.code.push(Op::I, 0);
    sys.code.push(Op::Push, 1);
    sys.code.push(Op::Add, 0);
    sys.code.push(Op::Loop, -3i64 as u64);
    sys.code.push(Op::Stop, 0);

    for _ in 0..150 {
        vm.ip = 0;
        vm.d_depth = 0;
        vm.r_stack.clear();
        vm.loop_stack.clear();
        let _ = vm.run_loop(&mut sys);
    }
}
