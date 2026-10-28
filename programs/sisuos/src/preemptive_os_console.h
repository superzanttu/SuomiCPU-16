#include <sc16_syscall.h>

static void write_text(const char *text)
{
    unsigned int length = 0;

    while (text[length] != 0)
        length++;
    if (sc16_write(1, text, length) != (int)length)
        sc16_exit(1);
}

static unsigned int append_number(char *buffer, unsigned int offset, unsigned long value)
{
    char digits[10];
    unsigned int count = 0;

    do
    {
        digits[count++] = '0' + (int)(value % 10UL);
        value = value / 10UL;
    } while (value != 0UL);
    while (count != 0)
        buffer[offset++] = digits[--count];
    buffer[offset] = 0;
    return offset;
}
