typedef unsigned char u8;
typedef unsigned short u16;
typedef unsigned int u32;
typedef unsigned long u64;
typedef void *buffer;

enum {
    STATUS_DRIVER = 3,
    STATUS_FEATURES_OK = 11,
    STATUS_READY = 15,
    DESC_NEXT = 1,
    DESC_WRITE = 2,
    REQUEST_READ = 0,
    REQUEST_WRITE = 1,
    REQUEST_FLUSH = 4,
    QUEUE_SIZE = 8,
    SECTOR_BYTES = 512,
    MMIO_ADDRESS = 0x10001000
};

static u32 request_lock;

static void put(u8 value)
{
    *(volatile unsigned char *)0x10000000 = value;
}

static void mmio(unsigned long offset, unsigned int value)
{
    *(volatile unsigned int *)(MMIO_ADDRESS + offset) = value;
}

static unsigned int mmio_read(unsigned long offset)
{
    return *(volatile unsigned int *)(MMIO_ADDRESS + offset);
}

unsigned long ram(void)
{
    return 0x80000000UL;
}

void ram_word(unsigned long offset, unsigned int value)
{
    *(volatile unsigned int *)(ram() + offset) = value;
}

void ram_half(unsigned long offset, unsigned short value)
{
    *(volatile unsigned short *)(ram() + offset) = value;
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

void descriptor(u64 offset, u64 address, u32 length, u16 flags, u16 next)
{
    ram_long(offset, address);
    ram_word(offset + 8L, length);
    ram_half(offset + 12L, flags);
    ram_half(offset + 14L, next);
}

void submit(u16 index)
{
    volatile u16 *available = (volatile u16 *)(ram() + 0x100UL);
    ram_byte(0x600L, 255);
    available[2 + ((index - 1) & (QUEUE_SIZE - 1))] = 0;
    available[1] = index;
    mmio(0x50L, 0);
    put(ram_read_byte(0x600L));
}

static int request(u32 type, u16 index)
{
    int result = 0;
    if (__atomic_exchange_n(&request_lock, 1, 2)) return 0;
    switch (type) {
    case REQUEST_READ:
        break;
    case REQUEST_WRITE:
        ram_half(28L, DESC_NEXT);
        break;
    case REQUEST_FLUSH:
        ram_half(14L, 2);
        break;
    default:
        goto unlock;
    }
    ram_word(0x300L, type);
    submit(index);
    result = 1;
unlock:
    __atomic_store_n(&request_lock, 0, 3);
    return result;
}

buffer copy_bytes(buffer destination, buffer source, u64 length)
{
    u8 *out = destination;
    u8 *in = source;
    u64 index;
    for (index = 0; index < length; ++index) {
        out[index] = in[index];
    }
    return destination;
}

int main(void)
{
    u8 snapshot[16];
    put(67);
    mmio(0x70L, STATUS_DRIVER);
    mmio(0x24L, 1);
    mmio(0x20L, 1);
    mmio(0x24L, 0);
    mmio(0x20L, 512);
    mmio(0x70L, STATUS_FEATURES_OK);
    mmio(0x38L, QUEUE_SIZE);
    mmio(0x80L, (unsigned int)ram());
    mmio(0x90L, (unsigned int)(ram() + 0x100UL));
    mmio(0xa0L, (unsigned int)(ram() + 0x200UL));
    mmio(0x44L, 1);
    mmio(0x70L, STATUS_READY);
    ram_half(0x100UL, 0);
    descriptor(0L, ram() + 0x300L, 16, DESC_NEXT, 1);
    descriptor(16L, ram() + 0x400L, SECTOR_BYTES, DESC_NEXT | DESC_WRITE, 2);
    descriptor(32L, ram() + 0x600L, 1, DESC_WRITE, 0);
    ram_long(0x308L, 1L);
    if (request(99, 1)) return 3;
    if (!request(REQUEST_READ, 1)) return 4;
    put(ram_read_byte(0x400L));
    unsigned char *copied = copy_bytes(snapshot, (void *)(ram() + 0x400UL), sizeof(snapshot));
    if (copied != snapshot || snapshot[0] != 'R' || snapshot[15] != 'R') return 1;
    snapshot[0] = 'W';
    copied = copy_bytes((void *)(ram() + 0x400UL), snapshot, sizeof(snapshot));
    if ((unsigned long)copied != ram() + 0x400UL) return 2;
    ram_long(0x700UL, (unsigned long)snapshot);
    ram_long(0x708UL, (unsigned long)copied);
    if (!request(REQUEST_WRITE, 2)) return 5;
    if (!request(REQUEST_FLUSH, 3)) return 6;
    if (__atomic_load_n(&request_lock, 2)) return 7;
    put(mmio_read(0x60L));
    put(*(volatile unsigned short *)(ram() + 0x202UL));
    mmio(0x64L, 1);
    mmio(0x70L, 0);
    put(mmio_read(0x44L));
    return 0;
}
