#include <stdint.h>

static void write_scratch(uint64_t value)
{
    asm volatile("csrw mscratch, %0" : : "r"(value) : "memory");
}

static uint64_t read_scratch(void)
{
    uint64_t value;
    __asm__ volatile("csrr %0, mscratch" : "=r"(value) : : "memory");
    return value;
}

static uint64_t exchange_scratch(uint64_t value)
{
    __asm__ __volatile__("csrrw %0, mscratch, %0" : "+r"(value) : : "memory");
    return value;
}

int main(void)
{
    write_scratch(UINT64_C(0xfedcba9876543210));
    uint64_t first = read_scratch();
    uint64_t old = exchange_scratch(17);
    asm volatile("csrsi mscratch, %0" : : "i"(8) : "memory");
    uint64_t last = read_scratch();
    if (first != UINT64_C(0xfedcba9876543210) || old != first || last != 25) return 1;
    *(volatile uint64_t *)UINT32_C(0x80000000) = first;
    *(volatile uint64_t *)UINT32_C(0x80000008) = old;
    *(volatile uint64_t *)UINT32_C(0x80000010) = last;
    asm volatile("fence" : : : "memory");
    *(volatile unsigned char *)UINT32_C(0x10000000) = 'C';
    *(volatile unsigned char *)UINT32_C(0x10000000) = last;
    *(volatile unsigned char *)UINT32_C(0x10000000) = '\n';
    return last;
}
