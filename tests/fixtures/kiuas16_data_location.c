int main(void) {
    unsigned long table, first, second, identity, result;
    __asm__(
        "j .Lread_%=\n.balign 8\n"
        ".Lfirst_%=: .quad 19\n"
        ".Ltable_%=: .word .Lfirst_%= - ., .Lsecond_%= - .\n"
        ".quad ., . + 8\n"
        ".Lsecond_%=: .quad 23\n"
        ".Lread_%=: la %0, .Ltable_%=\n"
        "lw %1, 0(%0)\nlw %2, 4(%0)\n"
        "ld %3, 8(%0)\nadd %1, %1, %0\n"
        "addi %4, %0, 4\nadd %2, %2, %4\n"
        "ld %1, 0(%1)\nld %2, 0(%2)\nadd %4, %1, %2"
        : "=&r"(table), "=&r"(first), "=&r"(second),
          "=&r"(identity), "=&r"(result));
    if (first != 19 || second != 23 || result != 42 || identity != table + 8)
        return -1;
    if (*(unsigned long *)(table + 16) != table + 24) return -2;
    *(volatile unsigned long *)0x80000000UL = result;
    *(volatile unsigned long *)0x80000008UL = table;
    *(volatile unsigned long *)0x80000010UL = identity;
    *(volatile unsigned char *)0x10000000UL = 'O';
    *(volatile unsigned char *)0x10000000UL = result;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return result;
}
