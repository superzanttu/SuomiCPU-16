int main(void) {
    long result, table, entry;
    __asm__ volatile(
        "j .Lentry_%=\n.byte 0xa5\n.p2align 3\n"
        ".Ltable_%=: .dword 42\n.byte 0x5a\n.align 4\n"
        ".Lentry_%=: la %1, .Ltable_%=\nla %2, .Lentry_%=\nld %0, 0(%1)"
        : "=&r"(result), "=&r"(table), "=&r"(entry));
    if (result != 42 || (table & 7) != 0 || (entry & 15) != 0) return -1;
    *(volatile long *)0x80000000UL = result;
    *(volatile long *)0x80000008UL = table;
    *(volatile long *)0x80000010UL = entry;
    *(volatile unsigned char *)0x10000000UL = 'E';
    *(volatile unsigned char *)0x10000000UL = result;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return result;
}
