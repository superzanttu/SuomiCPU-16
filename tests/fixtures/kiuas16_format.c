#include <stdarg.h>
#include <stdint.h>

static int log_line(const char *format, ...) __attribute__((format(printf, 1, 2)));
static int emitted;

static void put(unsigned char value)
{
    *(volatile unsigned char *)UINT32_C(0x10000000) = value;
    emitted++;
}

static int log_line(const char *format, ...)
{
    va_list values;
    va_start(values, format);
    for (; *format; format++) {
        if (*format != '%') put(*format);
        else {
            format++;
            if (*format == '%') put('%');
            else if (*format == 'c') put(va_arg(values, int));
            else if (*format == 'd') put('0' + va_arg(values, int));
            else if (*format == 'l') {
                format++;
                put('0' + va_arg(values, unsigned long));
            } else if (*format == 's') {
                const char *text = va_arg(values, const char *);
                while (*text) put(*text++);
            }
        }
    }
    va_end(values);
    return emitted;
}

int main(void)
{
    int total = log_line("%s:" "%d/%lu%c%%\n", "F", (short)7, 9UL, '!');
    *(volatile uint64_t *)UINT32_C(0x80000000) = total;
    return total;
}
