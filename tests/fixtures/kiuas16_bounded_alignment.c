int main(void) {
    unsigned long address, aligned, skipped, sum;
    unsigned char *data;
    int i;
    __asm__(
        "j .Lread_%=\n.balign 16\n"
        ".Ldata_%=: .byte 19\n.balign 8, 42, 7\n"
        ".Laligned_%=: .byte 23\n.p2align 4, -1, 6\n"
        ".Lskipped_%=: .byte 5\n.align 2,,2\n.byte 7\n"
        ".balign 4, 128\n.Lread_%=: la %0, .Ldata_%=\n"
        "la %1, .Laligned_%=\nla %2, .Lskipped_%="
        : "=&r"(address), "=&r"(aligned), "=&r"(skipped));
    if ((address & 15) != 0 || aligned != address + 8 || skipped != address + 9)
        return -1;
    data = (unsigned char *)address;
    sum = 0;
    for (i = 0; i < 16; i++) {
        unsigned long expected;
        if (i == 0) expected = 19;
        else if (i < 8) expected = 42;
        else if (i == 8) expected = 23;
        else if (i == 9) expected = 5;
        else if (i < 12) expected = 0;
        else if (i == 12) expected = 7;
        else expected = 128;
        if (data[i] != expected) return -2;
        sum += data[i];
    }
    if (sum != 732) return -3;
    *(volatile unsigned long *)0x80000000UL = address;
    *(volatile unsigned long *)0x80000008UL = aligned;
    *(volatile unsigned long *)0x80000010UL = skipped;
    *(volatile unsigned long *)0x80000018UL = sum;
    *(volatile unsigned char *)0x10000000UL = 'A';
    *(volatile unsigned char *)0x10000000UL = data[1];
    *(volatile unsigned char *)0x10000000UL = '\n';
    return 42;
}
