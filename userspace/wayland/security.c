/*
 * userspace/wayland/security.c - Security features for the compositor
 *
 * Implements:
 *   1. Seccomp-BPF allowlist filter for syscall filtering
 *   2. Privilege dropping after DRM/input fd acquisition
 */

#include "compositor_types.h"
#include <linux/seccomp.h>
#include <linux/filter.h>
#include <linux/audit.h>
#include <sys/prctl.h>
#include <pwd.h>
#include <grp.h>

/*
 * Seccomp-BPF allowlist - only permit syscalls required for compositor operation.
 * This is a minimal allowlist; expand as needed for your use case.
 */
static int seccomp_install_filter(void) {
    /* BPF program structure:
     * - Load architecture from seccomp_data
     * - Check if it matches our expected arch
     * - Load syscall number
     * - Compare against allowed syscalls
     * - Return ALLOW or KILL
     */
    
    struct sock_filter filter[] = {
        /* Load architecture */
        BPF_STMT(BPF_LD | BPF_W | BPF_ABS,
                 (uint32_t)offsetof(struct seccomp_data, arch)),
        /* Check architecture - adjust for your target */
#ifdef __x86_64__
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, AUDIT_ARCH_X86_64, 0, 30),
#elif defined(__aarch64__)
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, AUDIT_ARCH_AARCH64, 0, 30),
#else
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, AUDIT_ARCH_NATIVE, 0, 30),
#endif
        /* Load syscall number */
        BPF_STMT(BPF_LD | BPF_W | BPF_ABS,
                 (uint32_t)offsetof(struct seccomp_data, nr)),
        
        /* Allow list - add syscalls as needed */
        /* read, write, close */
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_read, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_write, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_close, 0, 1),
        
        /* mmap, munmap, mprotect, brk */
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_mmap, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_munmap, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_mprotect, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_brk, 0, 1),
        
        /* fcntl, ioctl, statx, fstat, lseek */
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_fcntl, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_ioctl, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_statx, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_fstat, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_lseek, 0, 1),
        
        /* epoll_create1, epoll_ctl, epoll_wait, epoll_pwait */
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_epoll_create1, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_epoll_ctl, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_epoll_wait, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_epoll_pwait, 0, 1),
        
        /* socket operations: socket, bind, listen, accept4, sendmsg, recvmsg */
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_socket, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_bind, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_listen, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_accept4, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_sendmsg, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_recvmsg, 0, 1),
        
        /* memfd_create, ftruncate, chmod, unlink, access */
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_memfd_create, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_ftruncate, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_chmod, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_unlink, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_access, 0, 1),
        
        /* getuid, getgid, geteuid, getegid */
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_getuid, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_getgid, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_geteuid, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_getegid, 0, 1),
        
        /* setuid, setgid, setgroups */
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_setuid, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_setgid, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_setgroups, 0, 1),
        
        /* prctl for seccomp setup */
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_prctl, 0, 1),
        
        /* clock_gettime for timestamps */
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_clock_gettime, 0, 1),
        
        /* exit, exit_group, rt_sigreturn for signal handling */
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_exit, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_exit_group, 0, 1),
        BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_rt_sigreturn, 0, 1),
        
        /* All other syscalls - KILL */
        BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_KILL),
        /* Allow */
        BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ALLOW),
    };
    
    struct sock_fprog prog = {
        .len = (unsigned short)(sizeof(filter) / sizeof(filter[0])),
        .filter = filter,
    };
    
    /* Set no_new_privs first - required for seccomp */
    if (prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) < 0) {
        return -1;
    }
    
    /* Install the filter */
    if (syscall(SYS_seccomp, SECCOMP_SET_MODE_FILTER, 0, &prog) < 0) {
        return -1;
    }
    
    return 0;
}

/*
 * Drop privileges to an unprivileged user after acquiring DRM/input fds.
 * Should be called after opening /dev/dri/card0 and input devices,
 * but before entering the main event loop.
 */
static int drop_privileges(const char *user) {
    struct passwd *pw;
    
    /* Lookup the target user */
    pw = getpwnam(user);
    if (!pw) {
        /* If user not found, try numeric uid */
        char *endptr;
        uid_t uid = (uid_t)strtoul(user, &endptr, 10);
        if (*endptr != '\0') {
            return -1;
        }
        pw = getpwuid(uid);
        if (!pw) {
            return -1;
        }
    }
    
    /* Clear supplementary groups */
    if (setgroups(0, NULL) < 0) {
        return -1;
    }
    
    /* Set GID first (must be done before setuid) */
    if (setgid(pw->pw_gid) < 0) {
        return -1;
    }
    
    /* Set UID */
    if (setuid(pw->pw_uid) < 0) {
        return -1;
    }
    
    /* Verify the change */
    if (setuid(0) == 0) {
        /* Still root - something went wrong */
        return -1;
    }
    
    /* Set HOME environment variable */
    if (setenv("HOME", pw->pw_dir, 1) < 0) {
        return -1;
    }
    
    /* Set USER environment variable */
    if (setenv("USER", pw->pw_name, 1) < 0) {
        return -1;
    }
    
    return 0;
}

/*
 * Initialize security features.
 * Call this early in main(), after acquiring necessary fds but before
 * entering the main event loop.
 */
int security_init(const char *drop_to_user) {
    /* Install seccomp filter */
    if (seccomp_install_filter() < 0) {
        return -1;
    }
    
    /* Drop privileges if requested */
    if (drop_to_user && drop_to_user[0] != '\0') {
        if (drop_privileges(drop_to_user) < 0) {
            return -1;
        }
    }
    
    return 0;
}
