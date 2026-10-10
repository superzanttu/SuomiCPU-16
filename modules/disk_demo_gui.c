/*
 * Retro Disk Drive Demo
 * Simulates a Commodore Amiga-style disk drive interaction.
 * Users can "insert" a disk, write data, and read data using a GUI.
 */

// Memory-mapped addresses
#define DISK_BASE 0x43100
#define DISK_CTRL (DISK_BASE + 0)
#define DISK_ID   (DISK_BASE + 1)
#define DISK_PTR  (DISK_BASE + 16) // 0x43110
#define DISK_OFF  (DISK_BASE + 18) // 0x43112

// GPU Addresses
#define GPU_BASE 0x43020
#define GPU_CMD  (GPU_BASE + 0)

// GPU Commands
#define GPU_CLEAR   1
#define GPU_PIXEL   2
#define GPU_RECT   3
#define GPU_LINE   4
#define GPU_TEXT   7
#define GPU_PRESENT 8

// RAM buffer for disk data (at RAM_START)
char *ram_buf = (char *)0x20000;

// UI state
int disk_inserted = 0;
int status_msg = 0; // 0: Idle, 1: Loading, 2: Writing, 3: Reading, 4: Error

void draw_rect(int x, int y, int w, int h, int color) {
    unsigned short *gpu = (unsigned short *)GPU_BASE;
    gpu[1] = (unsigned short)x; 
    gpu[2] = (unsigned short)y; 
    gpu[3] = (unsigned short)w; 
    gpu[4] = (unsigned short)h; 
    unsigned char *ctrl = (unsigned char *)GPU_BASE;
    ctrl[1] = (unsigned char)color;
    ctrl[0] = (unsigned char)GPU_RECT;
}

void draw_text(int x, int y, int color, char *text) {
    static char text_buffer[256];
    int i = 0;
    while(text[i]) {
        text_buffer[i] = text[i];
        i++;
    }
    text_buffer[i] = 0;
    
    unsigned char *gpu_bytes = (unsigned char *)GPU_BASE;
    
    unsigned int addr = (unsigned int)text_buffer;
    gpu_bytes[10] = (unsigned char)addr;
    gpu_bytes[11] = (unsigned char)((addr >> 8) & 0xFF);
    gpu_bytes[12] = (unsigned char)((addr >> 16) & 0xFF);
    gpu_bytes[13] = (unsigned char)((addr >> 24) & 0xFF);

    gpu_bytes[1] = (unsigned char)color;
    *(unsigned short *)(GPU_BASE + 2) = (unsigned short)x;
    *(unsigned short *)(GPU_BASE + 4) = (unsigned short)y;
    gpu_bytes[0] = (unsigned char)GPU_TEXT;
}

void main() {
    // Initialize colors
    int bg_color = 10; // Dark gray
    int drive_color = 8; // Orange/Brown (Retro)
    int text_color = 1;  // White
    int active_color = 5; // Yellow
    
    int cycle = 0;

    while (1) {
        // 1. Clear screen
        *(unsigned char *)(GPU_BASE + 1) = bg_color;
        *(unsigned char *)GPU_CMD = GPU_CLEAR;

        // 2. Draw "Disk Drive" Chassis
        draw_rect(100, 80, 120, 80, drive_color);
        draw_rect(110, 90, 100, 40, 1); // Slot
        
        // 3. Draw Status Text
        if (!disk_inserted) {
            draw_text(120, 180, text_color, "INSERT DISK");
        } else {
            if (status_msg == 0) {
                draw_text(120, 180, text_color, "DISK READY");
            } else if (status_msg == 1) {
                draw_text(120, 180, active_color, "LOADING...");
            } else if (status_msg == 2) {
                draw_text(120, 180, active_color, "WRITING...");
            } else if (status_msg == 3) {
                draw_text(120, 180, active_color, "READING...");
            } else {
                draw_text(120, 180, 2, "ERROR");
            }
        }

        // 4. Simulation cycle for the demo
        cycle++;
        
        if (cycle < 100) {
            // Waiting for disk
        } else if (cycle < 200) {
            // Insert Disk
            disk_inserted = 1;
            char *id = (char *)DISK_ID;
            id[0] = 'D'; id[1] = 'I'; id[2] = 'S'; id[3] = 'K'; 
            id[4] = '0'; id[5] = '0'; id[6] = '0'; id[7] = '1';
            *(unsigned char *)DISK_CTRL = 0x01; // LOAD
            status_msg = 1;
        } else if (cycle < 300) {
            status_msg = 0;
        } else if (cycle < 400) {
            // Write "DATA" to block 0
            status_msg = 2;
            ram_buf[0] = 'D'; ram_buf[1] = 'A'; ram_buf[2] = 'T'; ram_buf[3] = 'A';
            ram_buf[4] = 0;
            *(unsigned short *)DISK_PTR = 0;
            *(unsigned short *)DISK_OFF = 0;
            *(unsigned char *)DISK_CTRL = 0x08; // WRITE
        } else if (cycle < 500) {
            status_msg = 0;
        } else if (cycle < 600) {
            // Read block 0
            status_msg = 3;
            *(unsigned short *)DISK_PTR = 0;
            *(unsigned short *)DISK_OFF = 100; // Read to offset 100
            *(unsigned char *)DISK_CTRL = 0x04; // READ
        } else if (cycle < 700) {
            status_msg = 0;
        } else if (cycle < 800) {
            // Eject
            status_msg = 0;
            *(unsigned char *)DISK_CTRL = 0x02; // UNLOAD
            disk_inserted = 0;
        } else {
            cycle = 0;
        }

        // 5. Present
        *(unsigned char *) GPU_CMD = GPU_PRESENT;
    }
}
