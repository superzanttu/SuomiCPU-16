int main(void) {
    long inverted, magnitude, word, extended;
    long is_zero, nonzero, negative, positive;
    __asm__("not %0, %1" : "=r"(inverted) : "r"(-43L));
    __asm__("neg %0, %1" : "=r"(magnitude) : "r"(-42L));
    __asm__("negw %0, %1" : "=r"(word) : "r"(0x12345678ffffffd6UL));
    __asm__("sext.w %0, %1" : "=r"(extended) : "r"(0xabcdef018000002aUL));
    __asm__("seqz %0, %1" : "=r"(is_zero) : "r"(0L));
    __asm__("snez %0, %1" : "=r"(nonzero) : "r"(0x100000000UL));
    __asm__("sltz %0, %1" : "=r"(negative) : "r"(extended));
    __asm__("sgtz %0, %1" : "=r"(positive) : "r"(magnitude));
    if (inverted != 42 || magnitude != 42 || word != 42) return -1;
    if (extended != -2147483606L) return -2;
    if (is_zero + nonzero + negative + positive != 4) return -3;
    *(volatile long *)0x80000000UL = word;
    *(volatile long *)0x80000008UL = extended;
    *(volatile long *)0x80000010UL = is_zero + nonzero + negative + positive;
    *(volatile unsigned char *)0x10000000UL = 'P';
    *(volatile unsigned char *)0x10000000UL = word;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return word;
}
