static inline unsigned long high_product(unsigned long left, unsigned long right) {
    unsigned long result;
    __asm__("mulhu %0, %1, %2" : "=r"(result) : "r"(left), "r"(right));
    return result;
}

int main(void) {
    long product, quotient, remainder;
    long signed_high, mixed_high;
    __asm__("mulw %0, %3, %4\n"
            "divw %1, %0, %4\n"
            "remw %2, %0, %4"
        : "=&r"(product), "=&r"(quotient), "=&r"(remainder)
        : "r"(21L), "r"(2L));
    if (product != 42 || quotient != 21 || remainder != 0) return -1;
    unsigned long high = high_product(~0UL, 2UL);
    __asm__("mulh %0, %2, %3\nmulhsu %1, %2, %3"
        : "=r"(signed_high), "=r"(mixed_high) : "r"(-1L), "r"(~0UL));
    if (high != 1 || signed_high != 0 || mixed_high != -1) return -2;
    __asm__("divuw %0, %2, %3\nremuw %1, %2, %3"
        : "=r"(quotient), "=r"(remainder) : "r"(85UL), "r"(2UL));
    if (quotient != 42 || remainder != 1) return -3;
    *(volatile unsigned long *)0x80000000UL = product;
    *(volatile unsigned long *)0x80000008UL = high;
    *(volatile unsigned char *)0x10000000UL = 'M';
    *(volatile unsigned char *)0x10000000UL = product;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return product;
}
