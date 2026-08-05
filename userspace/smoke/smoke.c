// userspace/smoke/smoke.c
// Minimal userspace syscall smoke test for early QEMU boots.

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

static int check(const char *name, int ok) {
    if (!ok) fprintf(stderr, "SMOKE FAIL: %s (errno=%d)\n", name, errno);
    return ok;
}

static int syscall_errno_checks(void) {
    char byte = 0;
    int pass = 1;

    errno = 0;
    pass &= check("bad write returns errno", write(1, (const void *)0x1, 1) < 0 && errno == EFAULT);

    errno = 0;
    pass &= check("bad open returns errno", open((const char *)0x1, O_RDONLY) < 0 && errno == EFAULT);

    errno = 0;
    pass &= check("bad close returns errno", close(-1) < 0 && errno == EBADF);

    errno = 0;
    pass &= check("bad wait4 returns errno", waitpid(999999, NULL, 0) < 0 && errno == ECHILD);

    errno = 0;
    pass &= check("bad execve returns errno", syscall(SYS_execve, (const char *)0x1, NULL, NULL) < 0 && errno == EFAULT);

    errno = 0;
    pass &= check("read from /dev/null is EOF-safe", read(open("/dev/null", O_RDONLY), &byte, 1) >= 0);

    return pass;
}

int main(void) {
    int pass = 1;

    pass &= check("write stdout", write(STDOUT_FILENO, "SMOKE: write\n", 13) == 13);

    int fd = open("/dev/null", O_RDONLY);
    pass &= check("open /dev/null", fd >= 0);
    if (fd >= 0) pass &= check("close /dev/null", close(fd) == 0);

    pid_t pid = fork();
    pass &= check("fork", pid >= 0);
    if (pid == 0) {
        char *argv[] = { "/bin/true", NULL };
        char *envp[] = { "PATH=/bin:/usr/bin", NULL };
        execve("/bin/true", argv, envp);
        _exit(errno == ENOENT ? 0 : 127);
    }

    if (pid > 0) {
        int status = 0;
        pass &= check("wait4", waitpid(pid, &status, 0) == pid);
        pass &= check("child exit", WIFEXITED(status));
    }

    pid = fork();
    pass &= check("fork exit child", pid >= 0);
    if (pid == 0) _exit(42);
    if (pid > 0) {
        int status = 0;
        pass &= check("wait4 exit status", waitpid(pid, &status, 0) == pid && WIFEXITED(status) && WEXITSTATUS(status) == 42);
    }

    pass &= syscall_errno_checks();

    if (pass) printf("SMOKE OK: userspace_smoke\n");
    fflush(stdout);
    return pass ? 0 : 1;
}
