int main(void) {
    volatile unsigned long *payload = (volatile unsigned long *)0x80000000UL;
    volatile unsigned long *ready = (volatile unsigned long *)0x80000008UL;
    *ready = 0;
    *payload = 42;
    __asm__ volatile("fence w, w" : : : "memory");
    *ready = 1;
    __asm__ volatile("fence r, rw" : : : "memory");
    if (*ready != 1 || *payload != 42) return -1;
    __asm__ volatile("fence rw, rw\nfence iorw, iorw" : : : "memory");
    *(volatile unsigned char *)0x10000000UL = 'F';
    __asm__ volatile("fence o, o" : : : "memory");
    *(volatile unsigned char *)0x10000000UL = *payload;
    __asm__ volatile("fence io, rw" : : : "memory");
    *(volatile unsigned char *)0x10000000UL = '\n';
    __asm__ volatile("fence 0, w\nfence r, 0\nfence\nfence.i" : : : "memory");
    return *payload;
}
