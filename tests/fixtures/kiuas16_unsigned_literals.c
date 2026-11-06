int main(void) {
    unsigned long all, high, pattern, table;
    unsigned long *values;
    __asm__("li %0, 0xffffffffffffffff" : "=r"(all));
    __asm__("li %0, 9223372036854775808" : "=r"(high));
    __asm__("li %0, 0xfedc_ba98_7654_3210" : "=r"(pattern));
    __asm__(
        "j .Lread_%=\n.balign 8\n"
        ".Lvalues_%=: .dword 18446744073709551615, 0x8000000000000000, "
        "0xfedcba9876543210\n"
        ".Lread_%=: la %0, .Lvalues_%="
        : "=r"(table));
    values = (unsigned long *)table;
    if (all != ~0UL || high != (1UL << 63) || pattern != 0xfedcba9876543210UL)
        return -1;
    if (values[0] != all || values[1] != high || values[2] != pattern)
        return -2;
    *(volatile unsigned long *)0x80000000UL = all;
    *(volatile unsigned long *)0x80000008UL = high;
    *(volatile unsigned long *)0x80000010UL = pattern;
    *(volatile unsigned char *)0x10000000UL = 'U';
    *(volatile unsigned char *)0x10000000UL = 42;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return 42;
}
