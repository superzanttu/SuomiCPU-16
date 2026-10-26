#ifndef SC16_FOLDER_FS_H
#define SC16_FOLDER_FS_H

#define FOLDER_FS_REGISTERS ((volatile unsigned char *)0x43100UL)
#define FOLDER_FS_COMMAND_OFFSET 0
#define FOLDER_FS_ENTRY_INDEX_OFFSET 10
#define FOLDER_FS_RESULT_OFFSET 14
#define FOLDER_FS_STATUS_OFFSET 16
#define FOLDER_FS_OFFSET_OR_DESTINATION_OFFSET 18
#define FOLDER_FS_PATH_OFFSET 22
#define FOLDER_FS_BUFFER_OFFSET 26
#define FOLDER_FS_LENGTH_OFFSET 30
#define FOLDER_FS_FILE_SIZE_OFFSET 32

#define FOLDER_FS_LIST 0x41
#define FOLDER_FS_READ 0x42
#define FOLDER_FS_WRITE 0x43
#define FOLDER_FS_CREATE_DIR 0x44
#define FOLDER_FS_DELETE 0x45
#define FOLDER_FS_RENAME 0x46
#define FOLDER_FS_STAT 0x47

static void folder_fs_write16(volatile unsigned char *registers, int offset,
                              unsigned int value)
{
    registers[offset] = (unsigned char)(value >> 8);
    registers[offset + 1] = (unsigned char)value;
}

static void folder_fs_write32(volatile unsigned char *registers, int offset,
                              unsigned long value)
{
    registers[offset] = (unsigned char)(value >> 24);
    registers[offset + 1] = (unsigned char)(value >> 16);
    registers[offset + 2] = (unsigned char)(value >> 8);
    registers[offset + 3] = (unsigned char)value;
}

static unsigned int folder_fs_read16(volatile unsigned char *registers,
                                     int offset)
{
    return ((unsigned int)registers[offset] << 8) | registers[offset + 1];
}

static unsigned long folder_fs_read32(volatile unsigned char *registers,
                                      int offset)
{
    return ((unsigned long)registers[offset] << 24) |
           ((unsigned long)registers[offset + 1] << 16) |
           ((unsigned long)registers[offset + 2] << 8) |
           (unsigned long)registers[offset + 3];
}

static int folder_fs_list(unsigned int index, const char *path,
                          volatile char *buffer, int capacity)
{
    volatile unsigned char *registers;

    if (path == 0 || buffer == 0 || capacity < 6 || capacity > 4096)
        return -1;
    registers = FOLDER_FS_REGISTERS;
    folder_fs_write16(registers, FOLDER_FS_ENTRY_INDEX_OFFSET, index);
    folder_fs_write32(registers, FOLDER_FS_PATH_OFFSET,
                      (unsigned long)path);
    folder_fs_write32(registers, FOLDER_FS_BUFFER_OFFSET,
                      (unsigned long)buffer);
    folder_fs_write16(registers, FOLDER_FS_LENGTH_OFFSET,
                      (unsigned int)capacity);
    registers[FOLDER_FS_COMMAND_OFFSET] = FOLDER_FS_LIST;
    if (registers[FOLDER_FS_STATUS_OFFSET] != 0)
        return -1;
    return (int)folder_fs_read16(registers, FOLDER_FS_RESULT_OFFSET);
}

static int folder_fs_read(const char *path, unsigned long offset,
                          volatile unsigned char *buffer, int capacity,
                          unsigned long *file_size)
{
    volatile unsigned char *registers;

    if (path == 0 || buffer == 0 || capacity < 0 || capacity > 4096)
        return -1;
    registers = FOLDER_FS_REGISTERS;
    folder_fs_write32(registers, FOLDER_FS_OFFSET_OR_DESTINATION_OFFSET,
                      offset);
    folder_fs_write32(registers, FOLDER_FS_PATH_OFFSET,
                      (unsigned long)path);
    folder_fs_write32(registers, FOLDER_FS_BUFFER_OFFSET,
                      (unsigned long)buffer);
    folder_fs_write16(registers, FOLDER_FS_LENGTH_OFFSET,
                      (unsigned int)capacity);
    registers[FOLDER_FS_COMMAND_OFFSET] = FOLDER_FS_READ;
    if (registers[FOLDER_FS_STATUS_OFFSET] != 0)
        return -1;
    if (file_size != 0)
        *file_size = folder_fs_read32(registers, FOLDER_FS_FILE_SIZE_OFFSET);
    return (int)folder_fs_read16(registers, FOLDER_FS_RESULT_OFFSET);
}

static int folder_fs_write(const char *path,
                           const volatile unsigned char *buffer, int length)
{
    volatile unsigned char *registers;

    if (path == 0 || buffer == 0 || length < 0 || length > 4096)
        return -1;
    registers = FOLDER_FS_REGISTERS;
    folder_fs_write32(registers, FOLDER_FS_PATH_OFFSET,
                      (unsigned long)path);
    folder_fs_write32(registers, FOLDER_FS_BUFFER_OFFSET,
                      (unsigned long)buffer);
    folder_fs_write16(registers, FOLDER_FS_LENGTH_OFFSET,
                      (unsigned int)length);
    registers[FOLDER_FS_COMMAND_OFFSET] = FOLDER_FS_WRITE;
    if (registers[FOLDER_FS_STATUS_OFFSET] != 0)
        return -1;
    return (int)folder_fs_read16(registers, FOLDER_FS_RESULT_OFFSET);
}

#endif
