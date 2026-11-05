void put(unsigned char value)
{
    *(volatile unsigned char *)0x10000000 = value;
}

void mmio(unsigned long offset, unsigned int value)
{
    *(volatile unsigned int *)(0x10001000UL + offset) = value;
}

unsigned int mmio_read(unsigned long offset)
{
    return *(volatile unsigned int *)(0x10001000UL + offset);
}

unsigned long ram(void)
{
    return 0x80000000UL;
}

void ram_word(unsigned long offset, unsigned int value)
{
    *(volatile unsigned int *)(ram() + offset) = value;
}

void ram_long(unsigned long offset, unsigned long value)
{
    *(volatile unsigned long *)(ram() + offset) = value;
}

void ram_byte(unsigned long offset, unsigned char value)
{
    *(volatile unsigned char *)(ram() + offset) = value;
}

unsigned char ram_read_byte(unsigned long offset)
{
    return *(volatile unsigned char *)(ram() + offset);
}

void descriptor(unsigned long offset, unsigned long address, unsigned int length, unsigned int flags)
{
    ram_long(offset, address);
    ram_word(offset + 8L, length);
    ram_word(offset + 12L, flags);
}

void submit(unsigned int index)
{
    ram_byte(0x600L, 255);
    ram_word(0x100UL, index << 16);
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
    mmio(0x80L, (unsigned int)ram());
    mmio(0x90L, (unsigned int)(ram() + 0x100UL));
    mmio(0xa0L, (unsigned int)(ram() + 0x200UL));
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
