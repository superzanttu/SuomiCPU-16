/*
 * Disk Storage Demo
 * This program demonstrates how to load a disk, write a block of data,
 * and then read it back.
 */

// Memory-mapped addresses
#define DISK_BASE 0x43100
#define DISK_CTRL (DISK_BASE + 0)
#define DISK_ID   (DISK_BASE + 1)
#define DISK_PTR  (DISK_BASE + 16) // 0x43110
#define DISK_OFF  (DISK_BASE + 18) // 0x43112

// Data buffer in RAM
char buffer[255];

void main() {
    // 1. Load disk "DEMO1234"
    // We write the ID bytes to the memory-mapped interface
    buffer[0] = 'D'; buffer[1] = 'E'; buffer[2] = 'M'; buffer[3] = 'O';
    buffer[4] = '1'; buffer[5] = '2'; buffer[6] = '3'; buffer[7] = '4';
    
    // Copy ID to disk registers (manual copy since we don't have a memset for this)
    // In a real scenario, one might use a loop or a helper function.
    // Here we just use the RAM buffer and write it manually to the interface for simplicity.
    // However, since we can't easily 'memcpy' to MMIO in C without pointers, 
    // let's use a pointer to DISK_ID.
    
    char *id_ptr = (char *)DISK_ID;
    id_ptr[0] = 'D'; id_ptr[1] = 'E'; id_ptr[2] = 'M'; id_ptr[3] = 'O';
    id_ptr[4] = '1'; id_ptr[5] = '2'; id_ptr[6] = '3'; id_ptr[7] = '4';
    
    // Trigger LOAD (0x01)
    *(unsigned char *)DISK_CTRL = 0x01;

    // 2. Prepare data to write
    // "Hello Disk!"
    buffer[0] = 'H'; buffer[1] = 'e'; buffer[2] = 'l'; buffer[3] = 'l';
    buffer[4] = 'o'; buffer[5] = ' '; buffer[6] = 'D'; buffer[7] = 'i';
    buffer[8] = 's'; buffer[9] = 'k'; buffer[10] = '!';
    
    // Write block 0 to disk
    // Pointer = 0, Offset = 0 (buffer is at RAM_START, but the emulator 
    // implementation uses RAM_START + offset. Since buffer is global, 
    // we need to find its offset relative to RAM_START).
    // Note: In this emulator, global variables are placed at GLOBAL_BASE (0xE800).
    // However, the disk implementation specifically uses RAM_START (0x20000).
    // For this demo to work, we should move the buffer to RAM_START or adjust.
    // Let's assume we use a pointer to RAM_START for the buffer.
    
    char *ram_buf = (char *)0x20000;
    ram_buf[0] = 'H'; ram_buf[1] = 'e'; ram_buf[2] = 'l'; ram_buf[3] = 'l';
    ram_buf[4] = 'o'; ram_buf[5] = ' '; ram_buf[6] = 'D'; ram_buf[7] = 'i';
    ram_buf[8] = 's'; ram_buf[9] = 'k'; ram_buf[10] = '!';

    *(unsigned short *)DISK_PTR = 0;      // Block 0
    *(unsigned short *)DISK_OFF = 0;      // Offset 0 from RAM_START
    *(unsigned char *)DISK_CTRL = 0x08;    // WRITE

    // 3. Read it back to a different location in RAM
    *(unsigned short *)DISK_PTR = 0;      // Block 0
    *(unsigned short *)DISK_OFF = 512;    // Offset 512 from RAM_START
    *(unsigned char *)DISK_CTRL = 0x04;    // READ

    // 4. Unload disk
    *(unsigned char *)DISK_CTRL = 0x02;    // UNLOAD
}
