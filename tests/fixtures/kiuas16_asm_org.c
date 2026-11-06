int main(void) {
    unsigned long address, result, fill;
    __asm__(
        "j .Lread_%=\n.org 4096, 42\n"
        ".Lfixed_%=: .quad 19, 23, .\n"
        ".Lread_%=: la %0, .Lfixed_%=\n"
        "ld %1, 0(%0)\nld %2, 8(%0)\nadd %1, %1, %2\n"
        "lbu %2, -1(%0)"
        : "=&r"(address), "=&r"(result), "=&r"(fill));
    if (address != 0x2000UL || result != 42 || fill != 42) return -1;
    if (*(unsigned long *)(address + 16) != 0x2010UL) return -2;
    *(volatile unsigned long *)0x80000000UL = address;
    *(volatile unsigned long *)0x80000008UL = result;
    *(volatile unsigned long *)0x80000010UL = fill;
    *(volatile unsigned char *)0x10000000UL = 'G';
    *(volatile unsigned char *)0x10000000UL = result;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return result;
}
