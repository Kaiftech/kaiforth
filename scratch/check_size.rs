use kaiforth::jit::abi::JitContext;

fn main() {
    println!("JitContext size: {}", std::mem::size_of::<JitContext>());
}
