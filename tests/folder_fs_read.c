#include "folder_fs.h"

int main(void)
{
    char *path;
    volatile unsigned char *registers;
    unsigned long address;

    registers = FOLDER_FS_REGISTERS;
    path = "main.bin";
    address = (unsigned long)path;
    registers[FOLDER_FS_PATH_OFFSET] = (unsigned char)(address >> 24);
    registers[FOLDER_FS_PATH_OFFSET + 1] = (unsigned char)(address >> 16);
    registers[FOLDER_FS_PATH_OFFSET + 2] = (unsigned char)(address >> 8);
    registers[FOLDER_FS_PATH_OFFSET + 3] = (unsigned char)address;
    address = 0x40000UL;
    registers[FOLDER_FS_BUFFER_OFFSET] = (unsigned char)(address >> 24);
    registers[FOLDER_FS_BUFFER_OFFSET + 1] = (unsigned char)(address >> 16);
    registers[FOLDER_FS_BUFFER_OFFSET + 2] = (unsigned char)(address >> 8);
    registers[FOLDER_FS_BUFFER_OFFSET + 3] = (unsigned char)address;
    registers[FOLDER_FS_LENGTH_OFFSET] = 0;
    registers[FOLDER_FS_LENGTH_OFFSET + 1] = 3;
    registers[FOLDER_FS_COMMAND_OFFSET] = FOLDER_FS_READ;
    return registers[FOLDER_FS_STATUS_OFFSET];
}
