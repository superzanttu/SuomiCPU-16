int main(void)
{
    volatile unsigned int *status = (volatile unsigned int *)0x80000000UL;
    volatile unsigned char *buffer = (volatile unsigned char *)0x80000100UL;
    unsigned long checksum = 0;
    unsigned int index;
    *status = 0;
    *status |= 0x80000001U;
    *status &= ~1U;
    *status ^= 0x80000000U;
    for (index = 0; index < 16; ++index) {
        *buffer++ = index;
        checksum += index;
    }
    buffer -= 16;
    buffer[3] *= 7;
    checksum += buffer[3] - 3;
    *(volatile unsigned char *)0x10000000 = checksum;
    return checksum + *status;
}
