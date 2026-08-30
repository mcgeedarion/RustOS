/*
 * syscall_tests.c - Comprehensive Syscall Test Suite for RustOS
 * 
 * This suite validates POSIX compliance and correctness of core syscalls.
 * Compile with: gcc -static -o syscall_tests syscall_tests.c
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <sys/mman.h>
#include <sys/types.h>
#include <errno.h>
#include <signal.h>
#include <time.h>
#include <dirent.h>
#include <pthread.h>

#define TEST_PASS 0
#define TEST_FAIL 1
#define SKIP      2

static int tests_run = 0;
static int tests_passed = 0;
static int tests_failed = 0;
static int tests_skipped = 0;

#define ASSERT(cond, msg) do { \
    tests_run++; \
    if (cond) { \
        tests_passed++; \
        printf("  [PASS] %s\n", msg); \
    } else { \
        tests_failed++; \
        printf("  [FAIL] %s (errno=%d)\n", msg, errno); \
    } \
} while(0)

#define ASSERT_EQ(expected, actual, msg) do { \
    tests_run++; \
    if ((expected) == (actual)) { \
        tests_passed++; \
        printf("  [PASS] %s\n", msg); \
    } else { \
        tests_failed++; \
        printf("  [FAIL] %s: expected %ld, got %ld (errno=%d)\n", msg, (long)(expected), (long)(actual), errno); \
    } \
} while(0)

/* ============================================================================
 * File I/O Tests
 * ============================================================================ */

void test_open_close(void) {
    printf("\n=== Testing open/close ===\n");
    
    int fd = open("/tmp/test_open.txt", O_CREAT | O_WRONLY, 0644);
    ASSERT(fd >= 0, "open() creates file");
    
    if (fd >= 0) {
        ASSERT(close(fd) == 0, "close() succeeds");
    }
    
    fd = open("/tmp/test_open.txt", O_RDONLY);
    ASSERT(fd >= 0, "open() existing file");
    if (fd >= 0) {
        ASSERT(close(fd) == 0, "close() existing file");
    }
    
    ASSERT(open("/nonexistent/path/file", O_RDONLY) < 0, "open() nonexistent returns error");
}

void test_read_write(void) {
    printf("\n=== Testing read/write ===\n");
    
    const char *test_data = "Hello, RustOS!";
    char buffer[64];
    
    int fd = open("/tmp/test_rw.txt", O_CREAT | O_RDWR, 0644);
    ASSERT(fd >= 0, "open() for read/write test");
    
    if (fd >= 0) {
        ssize_t written = write(fd, test_data, strlen(test_data));
        ASSERT_EQ(strlen(test_data), written, "write() returns bytes written");
        
        lseek(fd, 0, SEEK_SET);
        
        ssize_t read_bytes = read(fd, buffer, sizeof(buffer));
        ASSERT_EQ(strlen(test_data), read_bytes, "read() returns bytes read");
        
        buffer[read_bytes] = '\0';
        ASSERT(strcmp(test_data, buffer) == 0, "read() data matches written data");
        
        close(fd);
    }
    
    unlink("/tmp/test_rw.txt");
}

void test_lseek(void) {
    printf("\n=== Testing lseek ===\n");
    
    const char *data = "0123456789";
    int fd = open("/tmp/test_seek.txt", O_CREAT | O_RDWR, 0644);
    ASSERT(fd >= 0, "open() for seek test");
    
    if (fd >= 0) {
        write(fd, data, strlen(data));
        
        off_t pos = lseek(fd, 5, SEEK_SET);
        ASSERT_EQ(5, pos, "lseek() SEEK_SET");
        
        pos = lseek(fd, 2, SEEK_CUR);
        ASSERT_EQ(7, pos, "lseek() SEEK_CUR");
        
        pos = lseek(fd, -3, SEEK_END);
        ASSERT_EQ(7, pos, "lseek() SEEK_END");
        
        pos = lseek(fd, 0, SEEK_END);
        ASSERT_EQ(10, pos, "lseek() returns file size");
        
        close(fd);
    }
    
    unlink("/tmp/test_seek.txt");
}

/* ============================================================================
 * Process Tests
 * ============================================================================ */

void test_fork_wait(void) {
    printf("\n=== Testing fork/wait ===\n");
    
    pid_t pid = fork();
    ASSERT(pid >= 0, "fork() succeeds");
    
    if (pid == 0) {
        /* Child process */
        exit(42);
    } else if (pid > 0) {
        /* Parent process */
        int status;
        pid_t waited = wait(&status);
        ASSERT_EQ(pid, waited, "wait() returns child PID");
        ASSERT(WIFEXITED(status), "child exited normally");
        ASSERT_EQ(42, WEXITSTATUS(status), "child exit code correct");
    }
}

void test_execve(void) {
    printf("\n=== Testing execve ===\n");
    
    pid_t pid = fork();
    ASSERT(pid >= 0, "fork() for execve test");
    
    if (pid == 0) {
        char *argv[] = {"/bin/true", NULL};
        char *envp[] = {"TEST_VAR=execve_test", NULL};
        execve("/bin/true", argv, envp);
        exit(1); /* Should not reach here */
    } else if (pid > 0) {
        int status;
        wait(&status);
        ASSERT(WIFEXITED(status) && WEXITSTATUS(status) == 0, "execve() succeeded");
    }
}

void test_getpid_getppid(void) {
    printf("\n=== Testing getpid/getppid ===\n");
    
    pid_t pid = getpid();
    ASSERT(pid > 0, "getpid() returns positive PID");
    
    pid_t ppid = getppid();
    ASSERT(ppid > 0, "getppid() returns positive PPID");
    
    ASSERT(pid != ppid || pid == 1, "PID != PPID (unless init)");
}

/* ============================================================================
 * Memory Tests
 * ============================================================================ */

void test_mmap_munmap(void) {
    printf("\n=== Testing mmap/munmap ===\n");
    
    size_t page_size = sysconf(_SC_PAGESIZE);
    
    void *addr = mmap(NULL, page_size, PROT_READ | PROT_WRITE, 
                      MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    ASSERT(addr != MAP_FAILED, "mmap() succeeds");
    
    if (addr != MAP_FAILED) {
        memset(addr, 0xAA, page_size);
        
        unsigned char *buf = (unsigned char *)addr;
        ASSERT(buf[0] == 0xAA && buf[page_size-1] == 0xAA, "mmap() memory writable");
        
        int ret = munmap(addr, page_size);
        ASSERT_EQ(0, ret, "munmap() succeeds");
    }
}

void test_brk(void) {
    printf("\n=== Testing brk ===\n");
    
    void *old_brk = sbrk(0);
    ASSERT(old_brk != (void*)-1, "sbrk(0) succeeds");
    
    void *new_brk = sbrk(page_size);
    ASSERT(new_brk != (void*)-1, "sbrk() increases heap");
    
    if (new_brk != (void*)-1) {
        void *current = sbrk(0);
        ASSERT((char*)current == (char*)old_brk + page_size, "brk increased correctly");
        
        sbrk(-page_size);
        current = sbrk(0);
        ASSERT(current == old_brk, "brk decreased correctly");
    }
}

/* ============================================================================
 * Directory Tests
 * ============================================================================ */

void test_mkdir_rmdir(void) {
    printf("\n=== Testing mkdir/rmdir ===\n");
    
    int ret = mkdir("/tmp/test_dir", 0755);
    ASSERT_EQ(0, ret, "mkdir() succeeds");
    
    struct stat st;
    ret = stat("/tmp/test_dir", &st);
    ASSERT_EQ(0, ret, "stat() on directory");
    ASSERT(S_ISDIR(st.st_mode), "created item is directory");
    
    ret = rmdir("/tmp/test_dir");
    ASSERT_EQ(0, ret, "rmdir() succeeds");
    
    ret = rmdir("/nonexistent_dir");
    ASSERT(ret < 0, "rmdir() nonexistent fails");
}

void test_opendir_readdir(void) {
    printf("\n=== Testing opendir/readdir ===\n");
    
    DIR *dir = opendir("/tmp");
    ASSERT(dir != NULL, "opendir() succeeds");
    
    if (dir != NULL) {
        struct dirent *entry;
        int entries_found = 0;
        
        while ((entry = readdir(dir)) != NULL) {
            entries_found++;
            ASSERT(entry->d_name != NULL, "readdir() returns valid name");
        }
        
        ASSERT(entries_found > 0, "readdir() finds entries");
        
        closedir(dir);
    }
}

/* ============================================================================
 * Signal Tests
 * ============================================================================ */

static volatile sig_atomic_t signal_received = 0;

void signal_handler(int sig) {
    signal_received = 1;
}

void test_signal_kill(void) {
    printf("\n=== Testing signal/kill ===\n");
    
    signal_received = 0;
    
    struct sigaction sa;
    memset(&sa, 0, sizeof(sa));
    sa.sa_handler = signal_handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = 0;
    
    int ret = sigaction(SIGUSR1, &sa, NULL);
    ASSERT_EQ(0, ret, "sigaction() succeeds");
    
    pid_t pid = getpid();
    ret = kill(pid, SIGUSR1);
    ASSERT_EQ(0, ret, "kill() to self succeeds");
    
    ASSERT(signal_received == 1, "signal handler called");
    
    signal(SIGUSR1, SIG_DFL);
}

/* ============================================================================
 * Time Tests
 * ============================================================================ */

void test_time_clock(void) {
    printf("\n=== Testing time/clock ===\n");
    
    time_t t = time(NULL);
    ASSERT(t > 0, "time() returns positive value");
    
    clock_t c = clock();
    ASSERT(c >= 0, "clock() succeeds");
    
    struct timespec ts;
    int ret = clock_gettime(CLOCK_MONOTONIC, &ts);
    ASSERT_EQ(0, ret, "clock_gettime() succeeds");
    ASSERT(ts.tv_sec > 0, "clock_gettime() returns valid time");
}

/* ============================================================================
 * Main Test Runner
 * ============================================================================ */

int main(int argc, char *argv[]) {
    printf("========================================\n");
    printf("RustOS Syscall Test Suite\n");
    printf("========================================\n");
    
    /* File I/O */
    test_open_close();
    test_read_write();
    test_lseek();
    
    /* Process */
    test_fork_wait();
    test_execve();
    test_getpid_getppid();
    
    /* Memory */
    test_mmap_munmap();
    test_brk();
    
    /* Directory */
    test_mkdir_rmdir();
    test_opendir_readdir();
    
    /* Signals */
    test_signal_kill();
    
    /* Time */
    test_time_clock();
    
    /* Summary */
    printf("\n========================================\n");
    printf("Test Summary:\n");
    printf("  Total:   %d\n", tests_run);
    printf("  Passed:  %d\n", tests_passed);
    printf("  Failed:  %d\n", tests_failed);
    printf("  Skipped: %d\n", tests_skipped);
    printf("========================================\n");
    
    return tests_failed > 0 ? 1 : 0;
}
