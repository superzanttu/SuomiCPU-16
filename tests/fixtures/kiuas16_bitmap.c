static unsigned long allocated;
static unsigned long events;
static unsigned int payload[4];
static unsigned int ready;

static _Bool reserve(unsigned int slot)
{
    if (slot >= 4) return 0;
    unsigned long mask = 1UL << slot;
    return (__atomic_fetch_or(&allocated, mask, 2) & mask) == 0;
}

static _Bool release(unsigned int slot)
{
    if (slot >= 4) return 0;
    unsigned long mask = 1UL << slot;
    return (__atomic_fetch_and(&allocated, ~mask, 3) & mask) != 0;
}

int main(void)
{
    if (!reserve(0) || !reserve(3) || reserve(3) || reserve(4)) return 1;
    payload[0] = 17;
    payload[3] = 42;
    if (__atomic_fetch_or(&ready, 9, 3) != 0) return 2;
    if (__atomic_load_n(&ready, 2) != 9) return 3;
    if (payload[0] != 17 || payload[3] != 42) return 4;
    if (!release(0) || release(0) || !reserve(0)) return 5;
    if (__atomic_fetch_xor(&events, 0x8000000000000001UL, 4) != 0) return 6;
    if (__atomic_fetch_xor(&events, 1, 5) != 0x8000000000000001UL) return 7;
    if (__atomic_fetch_and(&ready, ~1U, 3) != 9) return 8;
    if (__atomic_fetch_or(&ready, 1, 3) != 8) return 9;
    *(volatile unsigned long *)0x80000000UL = allocated;
    *(volatile unsigned long *)0x80000008UL = events;
    *(volatile unsigned int *)0x80000010UL = ready;
    *(volatile unsigned int *)0x80000014UL = payload[0];
    *(volatile unsigned int *)0x80000018UL = payload[3];
    *(volatile unsigned char *)0x10000000UL = 'B';
    *(volatile unsigned char *)0x10000000UL = allocated;
    *(volatile unsigned char *)0x10000000UL = ready;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return payload[0] + payload[3];
}
