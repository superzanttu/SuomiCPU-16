#include <stdint.h>

static const char name[4] __attribute__((nonstring)) = "NODE";
static unsigned char packet[4] __attribute__((__nonstring__)) = {0, 255, 7, 0};
static char scratch[4] __attribute__((nonstring, aligned(32)));

int main(void)
{
    volatile uint64_t *report = (volatile uint64_t *)UINT32_C(0x80000000);
    volatile unsigned char *uart = (volatile unsigned char *)UINT32_C(0x10000000);
    int total = 0;
    for (int i = 0; i < 4; i++) {
        scratch[i] = name[i];
        total += packet[i];
        uart[0] = scratch[i];
    }
    uart[0] = '\n';
    report[0] = total;
    report[1] = sizeof(name);
    report[2] = (uint64_t)scratch;
    return total;
}
