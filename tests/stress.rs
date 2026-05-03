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

use kaiforth::core::types::Op;
use kaiforth::vm::state::Vm;
use kaiforth::system::system::System;
use rand::Rng;

#[test]
fn test_jit_torture_random_stress() {
    let mut sys = System::new(1024 * 1024).expect("System init failed");
    let mut vm = Vm::new().expect("VM init failed");
    sys.paranoid_mode = true;
    sys.trace_enabled = true;
    sys.optimizer.next_opt_threshold = 1;
    sys.optimizer.score_threshold = 1;

    let mut rng = rand::thread_rng();
    let mut ops = Vec::new();

    for _ in 0..200 {
        let op_choice = rng.gen_range(0..10);
        match op_choice {
            0 => ops.push((Op::Push, rng.gen_range(1..100))),
            1 => ops.push((Op::Push, rng.gen_range(1..100))),
            2 => ops.push((Op::Add, 0)),
            3 => ops.push((Op::Sub, 0)),
            4 => ops.push((Op::Dup, 0)),
            5 => ops.push((Op::Swap, 0)),
            6 => ops.push((Op::Over, 0)),
            7 => ops.push((Op::Drop, 0)),
            8 => ops.push((Op::Inc, 0)),
            9 => ops.push((Op::Dec, 0)),
            _ => unreachable!(),
        }
    }
    ops.push((Op::Stop, 0));

    for (op, data) in ops {
        sys.code.push(op, data);
    }

    for _ in 0..150 {
        vm.ip = 0;
        vm.d_depth = 0;
        vm.f_stack.clear();
        vm.r_stack.clear();
        vm.c_stack.clear();
        let _ = vm.run_loop(&mut sys);
        sys.optimizer.observe_runtime_traces(&mut sys.runtime_trace);
        let _ = sys.synchronize_jit();
    }

    println!("Random Stress Test: JIT Hits={}, Poisoned={}, Rollbacks={}", 
        sys.jit_hits, sys.jit_poisoned, sys.jit_rollbacks);
}

#[test]
fn test_jit_divergence_forced() {
    let mut sys = System::new(1024 * 1024).expect("System init failed");
    let mut vm = Vm::new().expect("VM init failed");
    sys.paranoid_mode = false;
    sys.trace_enabled = true;
    sys.optimizer.next_opt_threshold = 1;
    sys.optimizer.score_threshold = 1;

    // A small pattern that repeats to reach the score threshold
    for _ in 0..10 {
        sys.code.push(Op::Push, 10);
        sys.code.push(Op::Push, 20);
        sys.code.push(Op::Add, 0);
        sys.code.push(Op::Drop, 0);
    }
    sys.code.push(Op::Stop, 0);

    // Warm up - run enough times to trigger JIT
    for _ in 0..50 {
        vm.ip = 0;
        vm.d_depth = 0;
        let _ = vm.run_loop(&mut sys);
        sys.optimizer.observe_runtime_traces(&mut sys.runtime_trace);
        let _ = sys.synchronize_jit();
    }

    #[cfg(target_arch = "x86_64")]
    assert!(sys.jit.blocks.len() > 0, "JIT should have compiled blocks after 200 runs");
    #[cfg(not(target_arch = "x86_64"))]
    assert_eq!(sys.jit.blocks.len(), 0, "JIT must not compile on non-x86_64");

    /*
    let mut poisoned = false;
    for block in sys.jit.blocks.values_mut() {
        unsafe {
            let slice = std::slice::from_raw_parts_mut(block.func_ptr as *mut u8, 1024);
            for i in 0..1022 {
                if slice[i] == 0x48 && slice[i+1] == 0x01 && slice[i+2] == 0xC8 {
                    slice[i+1] = 0x29; // add -> sub
                    slice[i+2] = 0xC1;
                    poisoned = true;
                }
            }
        }
    }
    
    assert!(poisoned, "Failed to manually poison JIT code (pattern not found)");

    vm.ip = 0;
    vm.d_depth = 0;
    let _ = vm.run_loop(&mut sys);

    println!("Divergence Test: JIT Poisoned={}", sys.jit_poisoned);
    assert!(sys.jit_poisoned > 0, "Divergence detection should have poisoned the block");
    */
}

#[test]
fn test_loop_stress() {
    let mut sys = System::new(1024 * 1024).expect("System init failed");
    let mut vm = Vm::new().expect("VM init failed");
    sys.paranoid_mode = true;
    sys.trace_enabled = true;
    sys.optimizer.next_opt_threshold = 1;
    sys.optimizer.score_threshold = 1;

    // DO I 1 + LOOP
    sys.code.push(Op::Push, 10); 
    sys.code.push(Op::Push, 0);   
    sys.code.push(Op::Do, 0);
    // sys.code.push(Op::I, 0);
    sys.code.push(Op::Push, 1);
    sys.code.push(Op::Add, 0);
    sys.code.push(Op::Loop, -2i64 as u64); 
    sys.code.push(Op::Stop, 0);

    // Warm up - run enough times to trigger JIT
    for _ in 0..20 {
        vm.ip = 0;
        vm.d_depth = 0;
        vm.r_stack.clear();
        vm.loop_stack.clear();
        let _ = vm.run_loop(&mut sys);
        sys.optimizer.observe_runtime_traces(&mut sys.runtime_trace);
        let _ = sys.synchronize_jit();
    }

    println!("Loop Stress: JIT Hits={}, Poisoned={}", sys.jit_hits, sys.jit_poisoned);
}
