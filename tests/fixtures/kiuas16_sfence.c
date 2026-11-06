int main(void) {
    unsigned long before, after;
    __asm__ volatile("csrr %0, satp" : "=r"(before));
    __asm__ volatile("sfence.vma\nsfence.vma %0\nsfence.vma %0, %1"
        : : "r"(0x40000000UL), "r"(7UL) : "memory");
    __asm__ volatile("csrr %0, satp" : "=r"(after));
    if (before != 0 || after != before) return -1;
    *(volatile unsigned long *)0x80000000UL = 42;
    *(volatile unsigned long *)0x80000008UL = after;
    *(volatile unsigned char *)0x10000000UL = 'V';
    *(volatile unsigned char *)0x10000000UL = 42;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return 42;
}
