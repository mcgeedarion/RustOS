/*
 * userspace/libc-shim/shim.c — syscall shim implementation
 *
 * Compile with:
 *   -nostdlib -ffreestanding -fno-stack-protector
 *
 */

#include "shim.h"

/* ─── raw syscall ─────────────────────────────────────────────────────────────── */

#if defined(__x86_64__)
/*
 * Linux x86_64 syscall ABI
 *   rax = syscall number, rdi rsi rdx r10 r8 r9 = args 1-6
 *   return value in rax (negative → -errno)
 */
long shim_syscall(long nr, long a1, long a2, long a3,
                  long a4, long a5, long a6)
{
    long ret;
    __asm__ volatile (
        "syscall"
        : "=a" (ret)
        : "0" (nr), "D" (a1), "S" (a2), "d" (a3),
          "r" (a4), "r" (a5), "r" (a6)
        : "rcx", "r11", "memory"
    );
    return ret;
}

#elif defined(__aarch64__)
/*
 * Linux AArch64 syscall ABI
 *   x8 = syscall number, x0-x5 = args 1-6
 *   return value in x0 (negative → -errno)
 */
long shim_syscall(long nr, long a1, long a2, long a3,
                  long a4, long a5, long a6)
{
    register long x8  __asm__("x8")  = nr;
    register long x0  __asm__("x0")  = a1;
    register long x1  __asm__("x1")  = a2;
    register long x2  __asm__("x2")  = a3;
    register long x3  __asm__("x3")  = a4;
    register long x4  __asm__("x4")  = a5;
    register long x5  __asm__("x5")  = a6;
    __asm__ volatile (
        "svc #0"
        : "+r" (x0)
        : "r" (x8), "r" (x1), "r" (x2), "r" (x3), "r" (x4), "r" (x5)
        : "memory"
    );
    return x0;
}

#else
#  error "shim.c: unsupported architecture"
#endif

/* ─── POSIX-compatible wrappers ────────────────────────────────────────────── */

ssize_t shim_write(int fd, const void *buf, size_t n)
{
    return (ssize_t)shim_syscall(SYS_WRITE, (long)fd, (long)buf, (long)n, 0, 0, 0);
}

ssize_t shim_read(int fd, void *buf, size_t n)
{
    return (ssize_t)shim_syscall(SYS_READ, (long)fd, (long)buf, (long)n, 0, 0, 0);
}

int shim_open(const char *path, int flags, mode_t mode)
{
    return (int)shim_syscall(SYS_OPEN, (long)path, (long)flags, (long)mode, 0, 0, 0);
}

int shim_close(int fd)
{
    return (int)shim_syscall(SYS_CLOSE, (long)fd, 0, 0, 0, 0, 0);
}

void shim_exit(int code)
{
    shim_syscall(SYS_EXIT, (long)code, 0, 0, 0, 0, 0);
    for (;;) {}
}

pid_t shim_fork(void)
{
    return (pid_t)shim_syscall(SYS_FORK, 0, 0, 0, 0, 0, 0);
}

int shim_execve(const char *path, char *const argv[], char *const envp[])
{
    return (int)shim_syscall(SYS_EXECVE, (long)path, (long)argv, (long)envp, 0, 0, 0);
}

int shim_waitpid(pid_t pid, int *status, int options)
{
    return (int)shim_syscall(SYS_WAIT4, (long)pid, (long)status, (long)options, 0, 0, 0);
}

int shim_nanosleep(long sec, long nsec)
{
    long ts[2] = { sec, nsec };
    return (int)shim_syscall(SYS_NANOSLEEP, (long)ts, 0, 0, 0, 0, 0);
}

int shim_sched_yield(void)
{
    return (int)shim_syscall(SYS_SCHED_YIELD, 0, 0, 0, 0, 0, 0);
}

char *shim_getcwd(char *buf, size_t size)
{
    long r = shim_syscall(SYS_GETCWD, (long)buf, (long)size, 0, 0, 0, 0);
    return (r < 0) ? (char *)0 : buf;
}

int shim_chdir(const char *path)
{
    return (int)shim_syscall(SYS_CHDIR, (long)path, 0, 0, 0, 0, 0);
}

/* ─── string primitives ──────────────────────────────────────────────────── */

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
    unsigned char *d = (unsigned char *)dst;
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
