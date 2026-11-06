int main(void) {
    long packed, extracted, signed_half, unchanged;
    __asm__("sllw %0, %1, %2" : "=r"(packed)
        : "r"(0x1234567880000015UL), "r"(33UL));
    __asm__("srlw %0, %1, %2" : "=r"(extracted)
        : "r"(0xabcdef0100005400UL), "r"(0x100000029UL));
    __asm__("sraw %0, %1, %2" : "=r"(signed_half)
        : "r"(0x1234567880000054UL), "r"(1UL));
    __asm__("srlw %0, %1, %2" : "=r"(unchanged)
        : "r"(0x1234567880000054UL), "r"(32UL));
    if (packed != 42 || extracted != 42) return -1;
    if (signed_half != -1073741782L || unchanged != -2147483564L) return -2;
    *(volatile long *)0x80000000UL = packed;
    *(volatile long *)0x80000008UL = extracted;
    *(volatile long *)0x80000010UL = signed_half;
    *(volatile long *)0x80000018UL = unchanged;
    *(volatile unsigned char *)0x10000000UL = 'W';
    *(volatile unsigned char *)0x10000000UL = extracted;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return extracted;
}
