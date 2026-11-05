void put(unsigned char value)
{
    *(volatile unsigned char *)0x10000000 = value;
}

void mmio(long offset, int value)
{
    *(volatile int *)(0x10001000L + offset) = value;
}

int mmio_read(long offset)
{
    return *(volatile int *)(0x10001000L + offset);
}

long ram(void)
{
    return 2147483647L + 1L;
}

void ram_word(long offset, int value)
{
    *(volatile int *)(ram() + offset) = value;
}

void ram_long(long offset, long value)
{
    *(volatile long *)(ram() + offset) = value;
}

void ram_byte(long offset, unsigned char value)
{
    *(volatile unsigned char *)(ram() + offset) = value;
}

unsigned char ram_read_byte(long offset)
{
    return *(volatile unsigned char *)(ram() + offset);
}

void descriptor(long offset, long address, int length, int flags)
{
    ram_long(offset, address);
    ram_word(offset + 8L, length);
    ram_word(offset + 12L, flags);
}

void submit(int index)
{
    ram_byte(0x600L, 255);
    ram_word(0x100L, index * 65536);
    mmio(0x50L, 0);
    put(ram_read_byte(0x600L));
}

int main(void)
{
    put(67);
    mmio(0x70L, 3);
    mmio(0x24L, 1);
    mmio(0x20L, 1);
    mmio(0x24L, 0);
    mmio(0x20L, 512);
    mmio(0x70L, 11);
    mmio(0x38L, 8);
    mmio(0x80L, (int)ram());
    mmio(0x90L, (int)(ram() + 0x100L));
    mmio(0xa0L, (int)(ram() + 0x200L));
    mmio(0x44L, 1);
    mmio(0x70L, 15);
    descriptor(0L, ram() + 0x300L, 16, 0x10001);
    descriptor(16L, ram() + 0x400L, 512, 0x20003);
    descriptor(32L, ram() + 0x600L, 1, 2);
    ram_long(0x308L, 1L);
    submit(1);
    put(ram_read_byte(0x400L));
    ram_word(0x300L, 1);
    ram_byte(0x400L, 87);
    ram_word(28L, 0x20001);
    submit(2);
    ram_word(0x300L, 4);
    ram_word(12L, 0x20001);
    submit(3);
    put(mmio_read(0x60L));
    put(ram_read_byte(0x202L));
    mmio(0x64L, 1);
    mmio(0x70L, 0);
    put(mmio_read(0x44L));
    return 0;
}
