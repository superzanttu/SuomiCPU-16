int main(void) {
    long result, target;
    __asm__ volatile(
        "addi sp, sp, -16\nsd ra, 0(sp)\n"
        "jal .Ldirect_%=\n"
        "la %1, .Lindirect_%=\njalr %1\n"
        "ld ra, 0(sp)\naddi sp, sp, 16\nj .Ldone_%=\n"
        ".Ldirect_%=: addi %0, zero, 19\njr ra\n"
        ".Lindirect_%=: addi %0, %0, 23\njr ra\n"
        ".Ldone_%=:"
        : "=&r"(result), "=&r"(target) : : "memory");
    if (result != 42) return -1;
    *(volatile long *)0x80000000UL = result;
    *(volatile unsigned char *)0x10000000UL = 'J';
    *(volatile unsigned char *)0x10000000UL = result;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return result;
}
