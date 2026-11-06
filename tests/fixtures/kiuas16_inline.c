#include <stdint.h>

static inline uint64_t exchange_scratch(uint64_t value)
{
    uint64_t old;
    __asm__ volatile ("csrrw %0, mscratch, %1" : "=r"(old) : "r"(value) : "memory");
    return old;
}

static inline uint64_t read_scratch(void)
{
    uint64_t value;
    __asm__ volatile ("csrr %0, mscratch" : "=r"(value));
    return value;
}

static inline uint64_t descend(uint64_t depth, uint64_t value)
{
    if (!depth) return exchange_scratch(value);
    return descend(depth - 1, value + 1);
}

int main(void)
{
    volatile uint64_t *report = (volatile uint64_t *)UINT32_C(0x80000000);
    volatile unsigned char *uart = (volatile unsigned char *)UINT32_C(0x10000000);
    exchange_scratch(UINT64_C(0x1234567887654321));
    report[0] = descend(3, 39);
    report[1] = read_scratch();
    uart[0] = 'I';
    uart[0] = report[1];
    uart[0] = '\n';
    return report[1];
}
