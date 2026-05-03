fn main() {
    println!("target_arch: {}", std::env::consts::ARCH);
    println!("cfg x86_64: {}", cfg!(target_arch = "x86_64"));
}
