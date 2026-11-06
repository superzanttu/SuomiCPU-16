int main(void) {
    long address, maximum, minimum, pattern;
    __asm__("li %0, 2147483648" : "=r"(address));
    __asm__("li %0, 9223372036854775807" : "=r"(maximum));
    __asm__("li %0, -9223372036854775808" : "=r"(minimum));
    __asm__("li %0, 0x123456789abcdef" : "=r"(pattern));
    if (address != 0x80000000L || maximum != 0x7fffffffffffffffL) return -1;
    if (minimum != (-9223372036854775807L - 1L)) return -2;
    if (pattern != 0x123456789abcdefL) return -3;
    *(volatile long *)address = maximum;
    *(volatile long *)(address + 8) = minimum;
    *(volatile long *)(address + 16) = pattern;
    *(volatile unsigned char *)0x10000000UL = 'L';
    *(volatile unsigned char *)0x10000000UL = 42;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return 42;
}
