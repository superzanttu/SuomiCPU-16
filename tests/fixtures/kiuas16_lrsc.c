static unsigned long counter = 19;

static inline unsigned long add_reserved(unsigned long *address, unsigned long amount) {
    unsigned long old;
    unsigned long status;
    __asm__ volatile(
        "1: lr.d.aq %0, (%2)\n"
        "add %1, %0, %3\n"
        "sc.d.rl %1, %1, (%2)\n"
        "bne %1, zero, 1b"
        : "=&r"(old), "=&r"(status)
        : "r"(address), "r"(amount)
        : "memory");
    return old;
}

int main(void) {
    unsigned long old = add_reserved(&counter, 23);
    if (old != 19 || counter != 42) return -1;
    __asm__ volatile("amomin.d %0, %1, (%2)"
        : "=r"(old) : "r"(-7L), "r"(&counter) : "memory");
    if (old != 42 || counter != (unsigned long)-7L) return -2;
    __asm__ volatile("amomax.d %0, %1, (%2)"
        : "=r"(old) : "r"(42L), "r"(&counter) : "memory");
    if (old != (unsigned long)-7L || counter != 42) return -3;
    __asm__ volatile("amominu.d %0, %1, (%2)"
        : "=r"(old) : "r"(-1L), "r"(&counter) : "memory");
    if (old != 42 || counter != 42) return -4;
    __asm__ volatile("amomaxu.d %0, %1, (%2)"
        : "=r"(old) : "r"(41UL), "r"(&counter) : "memory");
    if (old != 42 || counter != 42) return -5;
    *(volatile unsigned long *)0x80000000UL = counter;
    *(volatile unsigned char *)0x10000000UL = 'A';
    *(volatile unsigned char *)0x10000000UL = counter;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return counter;
}
