int main(void) {
    unsigned long address, sum;
    unsigned char *data;
    int i;
    __asm__(
        "j .Lread_%=\n"
        ".Ldata_%=: .space 4, 42\n.skip 3, -1\n.zero 2, 128\n"
        ".space 2\n.skip 1\n.zero 0, 255\n"
        ".balign 4\n.Lread_%=: la %0, .Ldata_%="
        : "=r"(address));
    data = (unsigned char *)address;
    sum = 0;
    for (i = 0; i < 12; i++) {
        unsigned long expected;
        if (i < 4) expected = 42;
        else if (i < 7) expected = 255;
        else if (i < 9) expected = 128;
        else expected = 0;
        if (data[i] != expected) return -1;
        sum += data[i];
    }
    if (sum != 1189) return -2;
    *(volatile unsigned long *)0x80000000UL = address;
    *(volatile unsigned long *)0x80000008UL = sum;
    *(volatile unsigned char *)0x10000000UL = 'B';
    *(volatile unsigned char *)0x10000000UL = data[0];
    *(volatile unsigned char *)0x10000000UL = '\n';
    return 42;
}
