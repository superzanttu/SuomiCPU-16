int accumulate(int depth, unsigned long *total, unsigned char *last) {
    unsigned long next = *total + depth;
    unsigned char narrowed = 250 + depth;
    if (depth > 0) accumulate(depth - 1, &next, &narrowed);
    *total = next;
    *last = narrowed + 1;
    return depth;
}

void report(volatile unsigned long *total, volatile unsigned char *last) {
    volatile unsigned char *uart = (volatile unsigned char *)0x10000000;
    *uart = 83;
    *uart = *total;
    *uart = *last;
    *uart = 10;
}

int main(void) {
    unsigned long total = 0;
    unsigned char last = 0;
    unsigned long untouched = sizeof(accumulate(9, &total, &last));
    accumulate(4, &total, &last);
    report(&total, &last);
    *(volatile unsigned long *)0x80000000UL = total;
    *(volatile unsigned char *)0x80000008UL = last;
    return total + untouched;
}
