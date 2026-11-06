int main(void) {
    unsigned long address, result, signed_word, unsigned_word, mask;
    __asm__(
        "j .Lread_%=\n.balign 8\n"
        ".Ldata_%=: .4byte 19\n.long 23\n.int 0x80000000, -1\n"
        ".8byte 0xffffffffffffffff\n.quad .Ldata_%=\n"
        ".Lread_%=: la %0, .Ldata_%=\n"
        "lw %1, 0(%0)\nlwu %2, 4(%0)\nadd %1, %1, %2\n"
        "lw %2, 8(%0)\nlwu %3, 12(%0)\nld %4, 16(%0)"
        : "=&r"(address), "=&r"(result), "=&r"(signed_word),
          "=&r"(unsigned_word), "=&r"(mask));
    if (result != 42 || signed_word != (unsigned long)-2147483648L ||
        unsigned_word != 4294967295UL || mask != ~0UL) return -1;
    if (*(unsigned long *)(address + 24) != address) return -2;
    *(volatile unsigned long *)0x80000000UL = result;
    *(volatile unsigned long *)0x80000008UL = signed_word;
    *(volatile unsigned long *)0x80000010UL = unsigned_word;
    *(volatile unsigned long *)0x80000018UL = mask;
    *(volatile unsigned char *)0x10000000UL = 'D';
    *(volatile unsigned char *)0x10000000UL = result;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return result;
}
