#include <stdint.h>

#define PAGE 4096
#define ALIGNED(n) __attribute__((aligned(n)))

static unsigned char a_marker = 0x5a;
static unsigned char descriptor[16] ALIGNED(64) = {[0] = 3, [15] = 7};
static uint64_t page_table[512] ALIGNED(PAGE);
static unsigned char *descriptor_view = descriptor;
static uint64_t *page_view = page_table;

int main(void)
{
    volatile uint64_t *report = (volatile uint64_t *)UINT32_C(0x80000000);
    if (((uintptr_t)descriptor & 63) || ((uintptr_t)page_table & (PAGE - 1))) return 1;
    if (a_marker != 0x5a || descriptor_view[0] != 3 || descriptor_view[15] != 7) return 2;
    if (page_view != page_table || page_table[0] || page_table[511]) return 3;
    page_table[0] = (uintptr_t)descriptor;
    page_table[511] = UINT64_C(0x0123456789abcdef);
    report[0] = (uintptr_t)descriptor;
    report[1] = (uintptr_t)page_table;
    report[2] = page_table[0];
    report[3] = page_table[511];
    *(volatile unsigned char *)UINT32_C(0x10000000) = 'A';
    *(volatile unsigned char *)UINT32_C(0x10000000) = descriptor_view[15];
    *(volatile unsigned char *)UINT32_C(0x10000000) = '\n';
    return 42;
}
