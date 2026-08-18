#include <stddef.h>
#include <unistd.h>

/* Forward declarations */
void *memcpy(void *dst, const void *src, size_t n);
void *memset(void *s, int c, size_t n);
void *memmove(void *dst, const void *src, size_t n);

/*
 * Fortified variants emitted by clang/gcc when _FORTIFY_SOURCE is set.
 * We skip the length check — the kernel controls its own memory.
 */
void *__memcpy_chk(void *dst, const void *src, size_t n, size_t dstlen)
{
    (void)dstlen;
    return memcpy(dst, src, n);
}

void *__memset_chk(void *s, int c, size_t n, size_t slen)
{
    (void)slen;
    return memset(s, c, n);
}

void *__memmove_chk(void *dst, const void *src, size_t n, size_t dstlen)
{
    (void)dstlen;
    return memmove(dst, src, n);
}

/*
 * __stack_chk_fail — called on stack-smashing detection.
 * Logs error message and halts to prevent exploitation.
 */
void __stack_chk_fail(void)
{
    /* Write error message to stderr */
    const char msg[] = "*** stack smashing detected ***: terminated\n";
    write(2, msg, sizeof(msg) - 1);
    
    /* Enter infinite loop to halt execution */
    for (;;) {}
}

/* Provide a dummy stack canary value - in production this should be randomized at boot */
__attribute__((weak)) unsigned long __stack_chk_guard = 0xdeadbeefcafe0000UL;
