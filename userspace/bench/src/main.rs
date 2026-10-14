//! RustOS no-libc syscall microbenchmark.
//!
//! Performs fixed warm-up and measured getpid iterations, times the measured
//! batch using CLOCK_MONOTONIC, and emits one machine-readable JSON record.
//! Latency thresholds stay advisory until repeated runs establish variance.

#![no_std]
#![no_main]
#![allow(clippy::missing_safety_doc)]

use core::arch::{asm, naked_asm};

const WARMUP_ITERATIONS: u64 = 1_000;
const SAMPLE_COUNT: usize = 256;
const SAMPLE_BATCH_SIZE: usize = 32;
const MEASURED_ITERATIONS: u64 = SAMPLE_COUNT as u64 * SAMPLE_BATCH_SIZE as u64;
const CLOCK_MONOTONIC: usize = 1;

#[cfg(target_arch = "x86_64")]
const SYS_WRITE: usize = 1;
#[cfg(target_arch = "x86_64")]
const SYS_GETPID: usize = 39;
#[cfg(target_arch = "x86_64")]
const SYS_CLOCK_GETTIME: usize = 228;

#[cfg(target_arch = "aarch64")]
const SYS_WRITE: usize = 64;
#[cfg(target_arch = "aarch64")]
const SYS_GETPID: usize = 172;
#[cfg(target_arch = "aarch64")]
const SYS_CLOCK_GETTIME: usize = 113;

#[cfg(target_arch = "riscv64")]
const SYS_WRITE: usize = 64;
#[cfg(target_arch = "riscv64")]
const SYS_GETPID: usize = 172;
#[cfg(target_arch = "riscv64")]
const SYS_CLOCK_GETTIME: usize = 113;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

#[inline(always)]
unsafe fn sys_write(fd: usize, buf: *const u8, count: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        asm!(
            "syscall",
            in("rax") SYS_WRITE,
            in("rdi") fd,
            in("rsi") buf,
            in("rdx") count,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: isize;
        asm!(
            "svc #0",
            in("x8") SYS_WRITE,
            in("x0") fd,
            in("x1") buf,
            in("x2") count,
            lateout("x0") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: isize;
        asm!(
            "ecall",
            in("a7") SYS_WRITE,
            in("a0") fd,
            in("a1") buf,
            in("a2") count,
            lateout("a0") ret,
            options(nostack),
        );
        ret
    }
}

#[inline(always)]
unsafe fn sys_getpid() -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        asm!(
            "syscall",
            in("rax") SYS_GETPID,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: isize;
        asm!("svc #0", in("x8") SYS_GETPID, lateout("x0") ret, options(nostack));
        ret
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: isize;
        asm!("ecall", in("a7") SYS_GETPID, lateout("a0") ret, options(nostack));
        ret
    }
}

#[inline(always)]
unsafe fn sys_clock_gettime(clock_id: usize, out: *mut Timespec) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        asm!(
            "syscall",
            in("rax") SYS_CLOCK_GETTIME,
            in("rdi") clock_id,
            in("rsi") out,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: isize;
        asm!(
            "svc #0",
            in("x8") SYS_CLOCK_GETTIME,
            in("x0") clock_id,
            in("x1") out,
            lateout("x0") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: isize;
        asm!(
            "ecall",
            in("a7") SYS_CLOCK_GETTIME,
            in("a0") clock_id,
            in("a1") out,
            lateout("a0") ret,
            options(nostack),
        );
        ret
    }
}

fn write_all(fd: usize, data: &[u8]) {
    let mut offset = 0;
    while offset < data.len() {
        let written = unsafe { sys_write(fd, data[offset..].as_ptr(), data.len() - offset) };
        if written <= 0 {
            unsafe { sys_exit_group(1) };
        }
        offset += written as usize;
    }
}

fn write_u64(fd: usize, value: u64) {
    let mut buffer = [0u8; 20];
    let mut index = buffer.len();
    let mut n = value;
    if n == 0 {
        index -= 1;
        buffer[index] = b'0';
    } else {
        while n != 0 {
            index -= 1;
            buffer[index] = b'0' + (n % 10) as u8;
            n /= 10;
        }
    }
    write_all(fd, &buffer[index..]);
}


fn write_json_string(fd: usize, value: &str) {
    write_all(fd, b"\"");
    for byte in value.as_bytes() {
        match *byte {
            b'"' => write_all(fd, b"\\\""),
            b'\\' => write_all(fd, b"\\\\"),
            b'\\n' => write_all(fd, b"\\n"),
            b'\\r' => write_all(fd, b"\\r"),
            b'\\t' => write_all(fd, b"\\t"),
            0..=0x1f => write_all(fd, b" "),
            _ => write_all(fd, core::slice::from_ref(byte)),
        }
    }
    write_all(fd, b"\"");
}

fn percentile(samples: &[u64; SAMPLE_COUNT], percentile: usize) -> u64 {
    let index = (samples.len() - 1) * percentile / 100;
    samples[index]
}

fn now_ns() -> Option<i128> {
    let mut ts = Timespec::default();
    let result = unsafe { sys_clock_gettime(CLOCK_MONOTONIC, &mut ts) };
    if result < 0 || ts.tv_sec < 0 || !(0..1_000_000_000).contains(&ts.tv_nsec) {
        return None;
    }
    Some(i128::from(ts.tv_sec) * 1_000_000_000 + i128::from(ts.tv_nsec))
}

fn run_benchmark() -> bool {
    let stdout = 1usize;
    let stderr = 2usize;

    let mut checksum = 0u64;
    for _ in 0..WARMUP_ITERATIONS {
        let pid = unsafe { sys_getpid() };
        if pid <= 0 {
            write_all(stderr, b"RUSTOS_BENCH marker=getpid_failed phase=warmup\\n");
            return false;
        }
        checksum = checksum.wrapping_add(pid as u64);
    }

    // Collect a distribution of per-call batch means. Each sample times a
    // fixed group so clock_gettime overhead is amortized rather than timing
    // every syscall individually.
    let mut samples = [0u64; SAMPLE_COUNT];
    let mut completed = 0u64;
    let mut elapsed_ns = 0u64;

    for sample_index in 0..SAMPLE_COUNT {
        let started = match now_ns() {
            Some(value) => value,
            None => {
                write_all(stderr, b"RUSTOS_BENCH marker=clock_failed phase=sample_start\\n");
                return false;
            }
        };

        for _ in 0..SAMPLE_BATCH_SIZE {
            let pid = unsafe { sys_getpid() };
            if pid <= 0 {
                write_all(stderr, b"RUSTOS_BENCH marker=getpid_failed phase=measured\\n");
                return false;
            }
            checksum = checksum.wrapping_add(pid as u64);
            completed += 1;
        }

        let ended = match now_ns() {
            Some(value) if value >= started => value,
            _ => {
                write_all(stderr, b"RUSTOS_BENCH marker=clock_failed phase=sample_end\\n");
                return false;
            }
        };
        let sample_elapsed = (ended - started) as u64;
        elapsed_ns = elapsed_ns.saturating_add(sample_elapsed);
        samples[sample_index] = sample_elapsed / SAMPLE_BATCH_SIZE as u64;
    }

    // An in-place insertion sort avoids allocation and is small enough for
    // this fixed 256-sample distribution.
    for i in 1..SAMPLE_COUNT {
        let value = samples[i];
        let mut j = i;
        while j > 0 && samples[j - 1] > value {
            samples[j] = samples[j - 1];
            j -= 1;
        }
        samples[j] = value;
    }

    let mean_ns = elapsed_ns / completed.max(1);
    write_all(stdout, b"{\\"schema_version\\":2,\\"benchmark\\":\\"getpid\\",\\"benchmark_version\\":2,\\"architecture\\":\\"");
    #[cfg(target_arch = "x86_64")]
    write_all(stdout, b"x86_64");
    #[cfg(target_arch = "aarch64")]
    write_all(stdout, b"aarch64");
    #[cfg(target_arch = "riscv64")]
    write_all(stdout, b"riscv64");

    write_all(stdout, b"\\",\\"environment\\":{\\"guest\\":\\"RustOS\\",\\"execution_context\\":\\"QEMU guest\\",\\"build_profile\\":\\"release\\",\\"clock\\":\\"CLOCK_MONOTONIC\\",\\"syscall_abi\\":\\"raw_linux_compatible\\",\\"host_os\\":");
    write_json_string(stdout, option_env!("RUSTOS_HOST_OS").unwrap_or("unknown"));
    write_all(stdout, b",\\"host_arch\\":");
    write_json_string(stdout, option_env!("RUSTOS_HOST_ARCH").unwrap_or("unknown"));
    write_all(stdout, b",\\"compiler_version\\":");
    write_json_string(stdout, option_env!("RUSTOS_COMPILER_VERSION").unwrap_or("unknown"));
    write_all(stdout, b",\\"qemu_version\\":");
    write_json_string(stdout, option_env!("RUSTOS_QEMU_VERSION").unwrap_or("unknown"));
    write_all(stdout, b"},\\"warmup_iterations\\":");
    write_u64(stdout, WARMUP_ITERATIONS);
    write_all(stdout, b",\\"measured_iterations\\":");
    write_u64(stdout, MEASURED_ITERATIONS);
    write_all(stdout, b",\\"successful_calls\\":");
    write_u64(stdout, completed);
    write_all(stdout, b",\\"operation_counts\\":{\\"getpid\\":");
    write_u64(stdout, completed);
    write_all(stdout, b",\\"clock_gettime\\":");
    write_u64(stdout, (SAMPLE_COUNT * 2) as u64);
    write_all(stdout, b"},\\"elapsed_ns\\":");
    write_u64(stdout, elapsed_ns);
    write_all(stdout, b",\\"mean_ns_per_call\\":");
    write_u64(stdout, mean_ns);
    write_all(stdout, b",\\"sample_count\\":");
    write_u64(stdout, SAMPLE_COUNT as u64);
    write_all(stdout, b",\\"sample_batch_size\\":");
    write_u64(stdout, SAMPLE_BATCH_SIZE as u64);
    write_all(stdout, b",\\"latency_ns_per_call\\":{\\"min\\":");
    write_u64(stdout, samples[0]);
    write_all(stdout, b",\\"p50\\":");
    write_u64(stdout, percentile(&samples, 50));
    write_all(stdout, b",\\"p90\\":");
    write_u64(stdout, percentile(&samples, 90));
    write_all(stdout, b",\\"p99\\":");
    write_u64(stdout, percentile(&samples, 99));
    write_all(stdout, b",\\"max\\":");
    write_u64(stdout, samples[SAMPLE_COUNT - 1]);
    write_all(stdout, b"},\\"pid_checksum\\":");
    write_u64(stdout, checksum);
    write_all(stdout, b"}\\nRUSTOS_BENCH marker=complete\\n");
    true
}

#[inline(always)]
unsafe fn sys_exit_group(status: usize) -> ! {
    #[cfg(target_arch = "x86_64")]
    asm!("syscall", in("rax") 231usize, in("rdi") status, options(noreturn));
    #[cfg(target_arch = "aarch64")]
    asm!("svc #0", in("x8") 94usize, in("x0") status, options(noreturn));
    #[cfg(target_arch = "riscv64")]
    asm!("ecall", in("a7") 94usize, in("a0") status, options(noreturn));
}

fn bench_main() -> ! {
    let ok = run_benchmark();
    unsafe { sys_exit_group(if ok { 0 } else { 1 }) }
}

#[cfg(target_arch = "x86_64")]
#[no_mangle]
#[unsafe(naked)]
unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        "and rsp, -16",
        "xor rbp, rbp",
        "call {entry}",
        "mov rdi, 1",
        "mov rax, 231",
        "syscall",
        entry = sym bench_main,
    );
}

#[cfg(target_arch = "aarch64")]
#[no_mangle]
#[unsafe(naked)]
unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        "mov x29, xzr",
        "and sp, sp, #-16",
        "bl {entry}",
        "mov x0, #1",
        "mov x8, #94",
        "svc #0",
        entry = sym bench_main,
    );
}

#[cfg(target_arch = "riscv64")]
#[no_mangle]
#[unsafe(naked)]
unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        "mv s0, zero",
        "andi sp, sp, -16",
        "call {entry}",
        "li a0, 1",
        "li a7, 94",
        "ecall",
        entry = sym bench_main,
    );
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    unsafe { sys_exit_group(101) }
}
