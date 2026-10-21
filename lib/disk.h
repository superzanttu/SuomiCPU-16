#ifndef _SC16_DISK_H
#define _SC16_DISK_H

static void sc16_disk_write16(volatile unsigned char *registers,
                              int offset, unsigned int value)
{
    registers[offset] = (unsigned char)(value >> 8);
    registers[offset + 1] = (unsigned char)value;
}

static void sc16_disk_write32(volatile unsigned char *registers,
                              int offset, unsigned long value)
{
    registers[offset] = (unsigned char)(value >> 24);
    registers[offset + 1] = (unsigned char)(value >> 16);
    registers[offset + 2] = (unsigned char)(value >> 8);
    registers[offset + 3] = (unsigned char)value;
}

static unsigned int sc16_disk_read16(volatile unsigned char *registers,
                                     int offset)
{
    return ((unsigned int)registers[offset] << 8) | registers[offset + 1];
}

static unsigned long sc16_disk_read32(volatile unsigned char *registers,
                                      int offset)
{
    return ((unsigned long)registers[offset] << 24) |
           ((unsigned long)registers[offset + 1] << 16) |
           ((unsigned long)registers[offset + 2] << 8) |
           (unsigned long)registers[offset + 3];
}

static int disk_read_file(const char *path, unsigned long offset,
                          volatile unsigned char *buffer, int capacity,
                          unsigned long *file_size)
{
    volatile unsigned char *registers;
    int length;

    if (path == 0 || buffer == 0 || capacity < 0 || capacity > 4096)
        return -1;
    registers = (volatile unsigned char *)0x43100UL;
    sc16_disk_write32(registers, 18, offset);
    sc16_disk_write32(registers, 22, (unsigned long)path);
    sc16_disk_write32(registers, 26, (unsigned long)buffer);
    sc16_disk_write16(registers, 30, (unsigned int)capacity);
    registers[0] = 0x40;
    if (registers[16] != 0)
        return -1;
    if (file_size != 0)
        *file_size = sc16_disk_read32(registers, 32);
    length = (int)sc16_disk_read16(registers, 14);
    return length;
}

static int disk_list_files(unsigned int index, volatile char *path_buffer,
                           int capacity)
{
    volatile unsigned char *registers;

    if (path_buffer == 0 || capacity < 1 || capacity > 256)
        return -1;
    registers = (volatile unsigned char *)0x43100UL;
    sc16_disk_write16(registers, 10, index);
    sc16_disk_write32(registers, 26, (unsigned long)path_buffer);
    sc16_disk_write16(registers, 30, (unsigned int)capacity);
    registers[0] = 0x80;
    if (registers[16] != 0)
        return -1;
    return (int)sc16_disk_read16(registers, 14);
}

#endif
