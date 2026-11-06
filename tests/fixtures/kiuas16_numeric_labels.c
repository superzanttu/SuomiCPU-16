static inline long countdown(long value) {
    __asm__ volatile(
        "1: addi %0, %0, -1\n"
        "bne %0, zero, 1b\n"
        "j 1f\n"
        "addi %0, zero, -99\n"
        "1: addi %0, %0, 21\n"
        : "+r"(value));
    return value;
}

int main(void) {
    long result = countdown(3) + countdown(5);
    *(volatile unsigned long *)0x80000000UL = result;
    *(volatile unsigned char *)0x10000000UL = 'N';
    *(volatile unsigned char *)0x10000000UL = result;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return result;
}
