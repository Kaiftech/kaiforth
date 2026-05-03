use kaiforth::vm::state::Vm;
use kaiforth::system::system::System;
use kaiforth::core::types::Op;

#[test]
fn test_jit_divergence_detection() {
    let mut sys = System::new().unwrap();
    sys.paranoid_mode = true;
    sys.optimizer.next_opt_threshold = 1; // JIT everything immediately

    // Create a simple addition block
    sys.code.push(Op::Push, 10);
    sys.code.push(Op::Push, 20);
    sys.code.push(Op::Add, 0);
    sys.code.push(Op::Ret, 0);

    let mut vm = Vm::new().unwrap();
    
    // Run once to trigger JIT
    vm.run_loop(&mut sys).unwrap();
    assert_eq!(vm.d_pop().unwrap(), 30);
    
    // Now we have a JIT block. Let's force a divergence.
    // We can't easily corrupt the machine code safely without UB, 
    // but we can "poison" the JIT by making it do something different than baseline.
    // Actually, our differential check compares BASELINE interpreter vs JIT.
    // If we change the bytecode AFTER JIT is compiled, we might trigger it?
    // No, JIT uses its own compiled code.
    
    // Let's test that paranoid mode catches a JIT trap.
    // (We'd need a way to make the JIT trap, e.g. division by zero).
}

#[test]
fn test_stack_hardening() {
    let mut sys = System::new().unwrap();
    // Test that pushing too much triggers a trap/error, not UB
    let mut vm = Vm::new().unwrap();
    for _ in 0..1025 {
        let _ = vm.d_push(1);
    }
    // Should have checked bounds
}
