static char banner[] = "Kiuas16: ";
static const char *cursor = "ROM ";
static signed char high[] = "\200\377";
static const char *fragments[] = { [2] = "rea" "dy\n", [0] = banner, "ROM ", [3] = 0 };
extern char banner[10];
extern const char *cursor;

static int puts_uart(const char *text)
{
    int count = 0;
    while (*text) {
        *(volatile unsigned char *)0x10000000UL = *text++;
        __atomic_fetch_add(&count, 1, 0);
    }
    return count;
}

int main(void)
{
    char scratch[4] = "OK";
    const char **fragment = fragments;
    int count = 0;
    while (*fragment != 0) {
        count += puts_uart(*fragment++);
    }
    cursor = fragments[1];
    cursor += 4;
    *(volatile unsigned char *)0x10000000UL = high[0];
    *(volatile unsigned char *)0x10000000UL = high[1];
    *(volatile unsigned char *)0x10000000UL = '\n';
    *(volatile unsigned long *)0x80000000UL = (unsigned long)high;
    *(volatile unsigned long *)0x80000008UL = (unsigned long)cursor;
    *(volatile unsigned long *)0x80000010UL = (unsigned long)&scratch[0];
    *(volatile unsigned long *)0x80000018UL = (unsigned long)fragments;
    if (sizeof(banner) != 10 || sizeof("rea" "dy\n") != 7) return 1;
    if (high[0] != -128 || high[1] != -1 || *cursor != 0) return 2;
    if (scratch[0] != 'O' || scratch[1] != 'K' || scratch[2] || scratch[3]) return 3;
    return count;
}
