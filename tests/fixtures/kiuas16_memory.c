void put(unsigned char value)
{
    *(volatile unsigned char *)0x10000000 = value;
}

unsigned long copy(volatile unsigned char *destination, volatile unsigned char *source, unsigned long count)
{
    volatile unsigned char *end = source + count;
    unsigned long checksum = 0;
    while (source < end) {
        *destination = *source;
        checksum = checksum + *destination;
        source = source + 1;
        destination = destination + 1;
    }
    return checksum;
}

int main(void)
{
    volatile unsigned char *source = (volatile unsigned char *)0x80000000UL;
    volatile unsigned char *destination = source + 512;
    volatile unsigned int *words = (volatile unsigned int *)0x80000400UL;
    unsigned int index = 0;
    unsigned long checksum;
    while (index < 256U) {
        source[index] = index;
        index = index + 1;
    }
    checksum = copy(destination, source, 256);
    words[0] = checksum;
    words[1] = (unsigned int)(destination + 256 - destination);
    put(80);
    put(words[0] & 255U);
    put(words[0] >> 8);
    put(10);
    return words[1];
}
