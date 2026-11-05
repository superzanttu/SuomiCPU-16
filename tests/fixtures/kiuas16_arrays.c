unsigned long copy(unsigned char destination[], unsigned char source[], unsigned long count) {
    unsigned long checksum = 0;
    for (unsigned long i = 0; i < count; ++i) {
        destination[i] = source[i];
        checksum += destination[i];
    }
    return checksum;
}

int main(void) {
    unsigned char source[256];
    unsigned char destination[256];
    unsigned int header[] = {256, 0x80000000U, 127};
    for (unsigned long i = 0; i < sizeof(source); ++i) source[i] = i;
    unsigned long checksum = copy(destination, source, sizeof(destination));
    volatile unsigned long *results = (volatile unsigned long *)0x80000000UL;
    results[0] = (unsigned long)source;
    results[1] = (unsigned long)destination;
    results[2] = checksum;
    results[3] = sizeof(header);
    volatile unsigned char *uart = (volatile unsigned char *)0x10000000;
    *uart = 65;
    *uart = checksum;
    *uart = destination[header[2]];
    *uart = 10;
    return header[0];
}
