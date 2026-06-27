/*
 * userspace/libc-shim/shim.c — syscall shim implementation
 *
 * Compile with:
 *   -nostdlib -ffreestanding -fno-stack-protector
 *
 */

#include "shim.h"

/* ─── raw syscall ──────────────────────────────────────────────────── */

#if defined(__x86_64__)
/*
 * Linux x86_64 syscall ABI
 *   rax = syscall number
 *   rdi rsi rdx r10 r8  r9 = args 1-6
 *   return value in rax (negative → -errno)
 */
long shim_syscall(long nr,
                  long a1, long a2, long a3,
                  long a4, long a5, long a6)
{
    long ret;
    __asm__ volatile (
        "syscall"
        : "=a" (ret)
        : "0"  (nr),
          "D"  (a1), "S" (a2), "d" (a3),
          "r"  (a4), "r" (a5), "r" (a6)   /* r10, r8, r9 via clobber/"r" */
        : "rcx", "r11", "memory"
    );
    return ret;
}

#else
#  error "shim.c: unsupported architecture"
#endif

/* ─── POSIX-compatible wrappers ─────────────────────────────────────── */

ssize_t shim_write(int fd, const void *buf, size_t n)
{
    return (ssize_t)shim_syscall(SYS_WRITE, (long)fd,
                                  (long)buf, (long)n, 0, 0, 0);
}

ssize_t shim_read(int fd, void *buf, size_t n)
{
    return (ssize_t)shim_syscall(SYS_READ, (long)fd,
                                  (long)buf, (long)n, 0, 0, 0);
}

int shim_open(const char *path, int flags, mode_t mode)
{
    return (int)shim_syscall(SYS_OPEN, (long)path,
                              (long)flags, (long)mode, 0, 0, 0);
}

int shim_close(int fd)
{
    return (int)shim_syscall(SYS_CLOSE, (long)fd, 0, 0, 0, 0, 0);
}

void shim_exit(int code)
{
    shim_syscall(SYS_EXIT, (long)code, 0, 0, 0, 0, 0);
    /* Unreachable, but satisfies [[noreturn]] / the compiler. */
    for (;;) __asm__ volatile ("hlt");
}

pid_t shim_fork(void)
{
    return (pid_t)shim_syscall(SYS_FORK, 0, 0, 0, 0, 0, 0);
}

int shim_execve(const char *path, char *const argv[], char *const envp[])
{
    return (int)shim_syscall(SYS_EXECVE,
                              (long)path, (long)argv, (long)envp, 0, 0, 0);
}

/*
 * waitpid — implemented via wait4(pid, status, options, NULL).
 * The kernel's SYS_wait4 handler must fill *status with the W* macros.
 */
int shim_waitpid(pid_t pid, int *status, int options)
{
    return (int)shim_syscall(SYS_WAIT4,
                              (long)pid, (long)status, (long)options,
                              0 /* rusage */, 0, 0);
}

/*
 * nanosleep({ .tv_sec=sec, .tv_nsec=nsec }, NULL).
 *
 * We lay out the timespec manually to avoid a struct dependency on
 * <time.h> in the shim's own header.
 */
int shim_nanosleep(long sec, long nsec)
{
    long ts[2] = { sec, nsec };   /* struct timespec layout on 64-bit */
    return (int)shim_syscall(SYS_NANOSLEEP, (long)ts, 0, 0, 0, 0, 0);
}

int shim_sched_yield(void)
{
    return (int)shim_syscall(SYS_SCHED_YIELD, 0, 0, 0, 0, 0, 0);
}

/*
 * getcwd — the kernel writes the path into buf and returns its length.
 * On error (buf too small, not mounted, etc.) it returns a negative errno.
 */
char *shim_getcwd(char *buf, size_t size)
{
    long r = shim_syscall(SYS_GETCWD, (long)buf, (long)size, 0, 0, 0, 0);
    return (r < 0) ? (char *)0 : buf;
}

int shim_chdir(const char *path)
{
    return (int)shim_syscall(SYS_CHDIR, (long)path, 0, 0, 0, 0, 0);
}

/* ─── string primitives ──────────────────────────────────────────────── */

size_t shim_strlen(const char *s)
{
    const char *p = s;
    while (*p) p++;
    return (size_t)(p - s);
}

int shim_strcmp(const char *a, const char *b)
{
    while (*a && *a == *b) { a++; b++; }
    return (unsigned char)*a - (unsigned char)*b;
}

void *shim_memcpy(void *dst, const void *src, size_t n)
{
    unsigned char       *d = (unsigned char *)dst;
    const unsigned char *s = (const unsigned char *)src;
    while (n--) *d++ = *s++;
    return dst;
}

void *shim_memset(void *dst, int c, size_t n)
{
    unsigned char *d = (unsigned char *)dst;
    while (n--) *d++ = (unsigned char)c;
    return dst;
}

/*
 * _start — bare entry point used when linking with -nostdlib.
 *
 * The kernel places argc/argv/envp on the stack at rsp per the
 * System V AMD64 ABI.  We call main() then exit() with its return value.
 *
 * When musl IS the libc, musl provides its own _start; this block is
 * excluded by guarding on __rustos_shim__.
 */
#ifdef __rustos_shim__
__attribute__((naked))
void _start(void)
{
    __asm__ volatile (
        /* align to 16 bytes before the call */
        "xor   %rbp, %rbp       \n"
        "mov   (%rsp),  %rdi    \n"   /* argc */
        "lea   8(%rsp), %rsi    \n"   /* argv */
        "lea   8(%rsi,%rdi,8),%rdx\n" /* envp = argv + argc + 1 */
        "and   $-16, %rsp       \n"
        "call  main             \n"
        /* main returned — call exit(rax) */
        "mov   %rax, %rdi       \n"
        "mov   $60,  %rax       \n"   /* SYS_exit */
        "syscall                \n"
        "hlt                   \n"
    );
}
#endif /* __rustos_shim__ */
