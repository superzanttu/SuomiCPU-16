#define COUNTDOWN(value) __asm__ volatile( \
    ".Lcount_%=: addi %[value], %[value], -1\n" \
    "bne %[value], zero, .Lcount_%=\n" \
    "j .Ldone_%=\n" \
    "addi %[value], zero, -99\n" \
    ".Ldone_%=: addi %[value], %[value], 21" \
    : [value] "+r"(value))

static inline long first(long value) {
    COUNTDOWN(value);
    return value;
}

static inline long second(long value) {
    COUNTDOWN(value);
    return value;
}

int main(void) {
    long result = first(3) + second(5);
    *(volatile unsigned long *)0x80000000UL = result;
    *(volatile unsigned char *)0x10000000UL = 'U';
    *(volatile unsigned char *)0x10000000UL = result;
    *(volatile unsigned char *)0x10000000UL = '\n';
    return result;
}
