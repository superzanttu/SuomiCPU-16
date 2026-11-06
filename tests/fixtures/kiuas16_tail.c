int main(void) {
    long result;
    __asm__ volatile(
        "addi sp, sp, -16\nsd ra, 0(sp)\n"
        "call .Lfirst_%=\n"
        "ld ra, 0(sp)\naddi sp, sp, 16\nj .Ldone_%=\n"
        ".Lfirst_%=: addi %0, zero, 19\n"
        "sd t1, 8(sp)\ntail .Llast_%=\n"
        "addi %0, zero, -99\n"
        ".Llast_%=: ld t1, 8(sp)\naddi %0, %0, 23\nret\n"
        ".Ldone_%=:"
        : "=&r"(result) : : "memory");
    if (result != 42) return -1;
    *(volatile long *)0x80000000UL = result;
    *(volatile unsigned char *)0x10000000UL = 'Q';
    *(volatile unsigned char *)0x10000000UL = result;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return result;
}
