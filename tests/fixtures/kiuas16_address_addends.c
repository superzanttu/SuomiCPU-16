int main(void) {
    unsigned long address, value, total;
    __asm__ volatile(
        "j .Lpoison_%= + 4\n"
        ".Lpoison_%=: li %2, -99\n"
        "li %2, 19\n"
        "la %0, .Ldata_%= + 16\n"
        "ld %1, -8(%0)\n"
        "add %2, %2, %1\n"
        "j .Ldata_end_%= + 4\n"
        ".balign 8\n"
        ".Ldata_%=: .dword 19, 23, 42\n"
        ".Ldata_end_%=: nop"
        : "=&r"(address), "=&r"(value), "=&r"(total));
    *(volatile unsigned long *)0x80000000UL = address;
    *(volatile unsigned long *)0x80000008UL = value;
    *(volatile unsigned long *)0x80000010UL = total;
    *(volatile unsigned char *)0x10000000UL = 'E';
    *(volatile unsigned char *)0x10000000UL = total;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return total;
}
