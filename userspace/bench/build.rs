use std::env;

fn main() {
    for (name, key) in [
        ("RUSTOS_HOST_OS", "RUSTOS_HOST_OS"),
        ("RUSTOS_HOST_ARCH", "RUSTOS_HOST_ARCH"),
        ("RUSTOS_COMPILER_VERSION", "RUSTOS_COMPILER_VERSION"),
        ("RUSTOS_QEMU_VERSION", "RUSTOS_QEMU_VERSION"),
    ] {
        println!("cargo:rerun-if-env-changed={key}");
        let value = env::var(key).unwrap_or_else(|_| "unknown".to_owned());
        println!("cargo:rustc-env={name}={value}");
    }
}
