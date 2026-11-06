int main(void) {
    long result, table, signed_value, unsigned_value;
    __asm__ volatile(
        "j .Lread_%=\n.balign 2\n"
        ".Ltable_%=: .half 0x8000\n.short -32768\n.2byte 42\n.half 65535\n"
        ".balign 4\n.Lread_%=: la %1, .Ltable_%=\n"
        "lhu %3, 0(%1)\nlh %2, 2(%1)\n"
        "add %0, %3, %2\nlhu %2, 4(%1)\nadd %0, %0, %2\n"
        "lh %2, 6(%1)"
        : "=&r"(result), "=&r"(table), "=&r"(signed_value), "=&r"(unsigned_value));
    if (result != 42 || signed_value != -1 || unsigned_value != 32768) return -1;
    *(volatile long *)0x80000000UL = result;
    *(volatile long *)0x80000008UL = signed_value;
    *(volatile long *)0x80000010UL = unsigned_value;
    *(volatile unsigned char *)0x10000000UL = 'H';
    *(volatile unsigned char *)0x10000000UL = result;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return result;
}
