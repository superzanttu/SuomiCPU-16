#include <stdint.h>

#define NORETURN __attribute__((noreturn))

static void panic(unsigned char code) NORETURN;
static volatile uint64_t phase;

static void panic(unsigned char code)
{
    volatile unsigned char *uart = (volatile unsigned char *)UINT32_C(0x10000000);
    volatile uint64_t *report = (volatile uint64_t *)UINT32_C(0x80000000);
    report[0] = phase;
    report[1] = code;
    *uart = 'P';
    *uart = code;
    *uart = '\n';
    for (;;) asm volatile("wfi" : : : "memory");
}

static void descend(int depth)
{
    if (depth) descend(depth - 1);
    else panic(42);
    phase = 99;
}

int main(void)
{
    phase = 7;
    descend(3);
    *(volatile unsigned char *)UINT32_C(0x10000000) = 'X';
    return 1;
}
