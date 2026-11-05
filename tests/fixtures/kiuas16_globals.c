unsigned long total = 100;
unsigned int calls;
unsigned char seed[] = {7, 11, 13, 17};
unsigned char history[8];

void record(unsigned char value) {
    history[calls] = value;
    ++calls;
    total += value;
}

int main(void) {
    if (total != 100 || calls != 0) return 1;
    for (unsigned int i = 0; i < sizeof(history); ++i) {
        if (history[i] != 0) return 2;
    }
    for (unsigned int i = 0; i < sizeof(seed); ++i) record(seed[i]);
    volatile unsigned long *results = (volatile unsigned long *)0x80000000UL;
    results[0] = (unsigned long)&total;
    results[1] = (unsigned long)&calls;
    results[2] = (unsigned long)seed;
    results[3] = (unsigned long)history;
    volatile unsigned char *uart = (volatile unsigned char *)0x10000000;
    *uart = 71;
    *uart = total;
    *uart = calls;
    *uart = history[3];
    *uart = 10;
    return total;
}
