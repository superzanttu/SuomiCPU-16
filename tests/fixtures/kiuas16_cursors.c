unsigned char buffer[8];
unsigned char *cursor = buffer + 1;
unsigned char *limit = &buffer[7];
unsigned int count;
unsigned int *counter = &count;
volatile unsigned char *uart = (volatile unsigned char *)0x10000000;
unsigned char *empty;

void append(unsigned char value) {
    *cursor++ = value;
    ++*counter;
}

int main(void) {
    if (empty || cursor != &buffer[1] || limit != buffer + 7) return 1;
    while (cursor < limit) append(10 + count);
    volatile unsigned long *results = (volatile unsigned long *)0x80000000UL;
    results[0] = (unsigned long)buffer;
    results[1] = (unsigned long)cursor;
    results[2] = (unsigned long)limit;
    results[3] = (unsigned long)counter;
    results[4] = (unsigned long)&count;
    *uart = 82;
    *uart = count;
    *uart = buffer[6];
    *uart = 10;
    return count + (cursor - buffer);
}
