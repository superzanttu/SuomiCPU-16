#ifndef _SC16_DISK_H
#define _SC16_DISK_H

#include "folder_fs.h"

static int disk_read_file(const char *path, unsigned long offset,
                          volatile unsigned char *buffer, int capacity,
                          unsigned long *file_size)
{
    return folder_fs_read(path, offset, buffer, capacity, file_size);
}

static int disk_list_files(unsigned int index, volatile char *path_buffer,
                           int capacity)
{
    volatile unsigned char *registers;

    if (path_buffer == 0 || capacity < 1 || capacity > 256)
        return -1;
    registers = FOLDER_FS_REGISTERS;
    folder_fs_write16(registers, FOLDER_FS_ENTRY_INDEX_OFFSET, index);
    folder_fs_write32(registers, FOLDER_FS_BUFFER_OFFSET,
                      (unsigned long)path_buffer);
    folder_fs_write16(registers, FOLDER_FS_LENGTH_OFFSET,
                      (unsigned int)capacity);
    registers[FOLDER_FS_COMMAND_OFFSET] = 0x80;
    if (registers[FOLDER_FS_STATUS_OFFSET] != 0)
        return -1;
    return (int)folder_fs_read16(registers, FOLDER_FS_RESULT_OFFSET);
}

#endif
