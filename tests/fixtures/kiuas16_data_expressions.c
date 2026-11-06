int main(void) {
    unsigned long address, value, backward, pointer, length;
    __asm__(
        "j .Lread_%=\n.balign 8\n"
        ".Lbase_%=: .word .Lpayload_%= - .Lbase_%=\n"
        ".word .Lbase_%= - .Lpayload_%=\n"
        ".quad .Lpayload_%= + 8\n"
        ".Lpayload_%=: .quad 19, 23\n"
        ".Lend_%=: .half .Lend_%= - .Lbase_%=\n"
        ".balign 4\n.Lread_%=: la %0, .Lbase_%=\n"
        "lw %1, 0(%0)\nlw %2, 4(%0)\nld %3, 8(%0)\n"
        "add %1, %1, %0\nld %1, 0(%1)\nld %4, 0(%3)\n"
        "add %1, %1, %4\nlhu %4, 32(%0)"
        : "=&r"(address), "=&r"(value), "=&r"(backward),
          "=&r"(pointer), "=&r"(length));
    if (value != 42 || backward != (unsigned long)-16L ||
        pointer != address + 24 || length != 32) return -1;
    *(volatile unsigned long *)0x80000000UL = value;
    *(volatile unsigned long *)0x80000008UL = backward;
    *(volatile unsigned long *)0x80000010UL = pointer - address;
    *(volatile unsigned long *)0x80000018UL = length;
    *(volatile unsigned char *)0x10000000UL = 'R';
    *(volatile unsigned char *)0x10000000UL = value;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return value;
}
