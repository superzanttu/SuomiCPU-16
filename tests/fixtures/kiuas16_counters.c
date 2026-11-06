int main(void) {
    unsigned long cycles, ticks, retired, scratch;
    __asm__ volatile(
        "rdcycle %0\nrdcycle %1\nsub %0, %1, %0\n"
        "rdtime %1\nrdtime %2\nsub %1, %2, %1\n"
        "rdinstret %2\nrdinstret %3\nsub %2, %3, %2"
        : "=&r"(cycles), "=&r"(ticks), "=&r"(retired), "=&r"(scratch));
    if (cycles != 1 || ticks != 1 || retired != 1) return -1;
    *(volatile unsigned long *)0x80000000UL = cycles;
    *(volatile unsigned long *)0x80000008UL = ticks;
    *(volatile unsigned long *)0x80000010UL = retired;
    *(volatile unsigned char *)0x10000000UL = 'T';
    *(volatile unsigned char *)0x10000000UL = 42;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return 42;
}
