static inline long countdown(long value) {
    __asm__ volatile(
        "blez %0, .Ldone_%=\n"
        ".Lloop_%=: addi %0, %0, -1\n"
        "bnez %0, .Lloop_%=\n"
        "addi %0, %0, 21\n"
        ".Ldone_%=:"
        : "+r"(value));
    return value;
}

static inline long ordered(unsigned long left, unsigned long right) {
    long result;
    __asm__ volatile(
        "addi %0, zero, 0\n"
        "bgtu %1, %2, .Lgreater_%=\n"
        "j .Ldone_%=\n"
        ".Lgreater_%=: addi %0, zero, 21\n"
        ".Ldone_%=:"
        : "=&r"(result) : "r"(left), "r"(right));
    return result;
}

int main(void) {
    long result = countdown(5) + ordered(~0UL, 1UL);
    if (countdown(-7) != -7 || ordered(1UL, ~0UL) != 0) return -1;
    *(volatile unsigned long *)0x80000000UL = result;
    *(volatile unsigned char *)0x10000000UL = 'B';
    *(volatile unsigned char *)0x10000000UL = result;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return result;
}
