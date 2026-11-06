#include <stdarg.h>
#include <stdint.h>

static long consume(int count, va_list values)
{
    long result = 0;
    for (int i = 0; i < count; i++) result += va_arg(values, long);
    return result;
}

static long sum(int count, ...)
{
    va_list values;
    va_list copy;
    va_start(values, count);
    va_copy(copy, values);
    long first = consume(count, values);
    long second = consume(count, copy);
    va_end(copy);
    va_end(values);
    return first + second;
}

int main(void)
{
    long total = sum(11, 1L, 2L, 3L, 4L, 5L, 6L, 7L, 8L, 9L, 10L, 11L);
    volatile uint64_t *report = (volatile uint64_t *)UINT32_C(0x80000000);
    report[0] = total;
    *(volatile unsigned char *)UINT32_C(0x10000000) = 'V';
    *(volatile unsigned char *)UINT32_C(0x10000000) = total;
    *(volatile unsigned char *)UINT32_C(0x10000000) = '\n';
    return total;
}
