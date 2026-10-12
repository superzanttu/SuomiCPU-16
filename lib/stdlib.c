#include <stdlib.h>
#include <errno.h>
#include <limits.h>
#include <string.h>

unsigned int __sc16_rand_seed;
int errno;
static unsigned char __sc16_heap[512];
static unsigned char __sc16_heap_blocks[512];

static unsigned char *__sc16_heap_start(void)
{
    unsigned char *start;

    start = __sc16_heap;
    if (((size_t)start & 1UL) != 0)
        ++start;
    return start;
}

static int __sc16_digit_value(int c)
{
    if (c >= '0' && c <= '9')
        return c - '0';
    if (c >= 'a' && c <= 'z')
        return c - 'a' + 10;
    if (c >= 'A' && c <= 'Z')
        return c - 'A' + 10;
    return -1;
}

int abs(int value)
{
    return value < 0 ? -value : value;
}

long labs(long value)
{
    return value < 0 ? -value : value;
}

div_t div(int numerator, int denominator)
{
    div_t result;

    result.quot = numerator / denominator;
    result.rem = numerator % denominator;
    return result;
}

ldiv_t ldiv(long numerator, long denominator)
{
    ldiv_t result;

    result.quot = numerator / denominator;
    result.rem = numerator % denominator;
    return result;
}

int atoi(const char *text)
{
    return (int)strtol(text, NULL, 10);
}

long atol(const char *text)
{
    return strtol(text, NULL, 10);
}

double atof(const char *text)
{
    return strtod(text, NULL);
}

double strtod(const char *text, char **endptr)
{
    const char *start;
    int negative;
    int digit;
    int exponent;
    int exponent_negative;
    double value;

    start = text;
    while (*text == ' ' || (*text >= '\t' && *text <= '\r'))
        ++text;
    negative = 0;
    if (*text == '+' || *text == '-') {
        negative = *text == '-';
        ++text;
    }
    value = 0.0;
    digit = 0;
    while (*text >= '0' && *text <= '9') {
        digit = 1;
        value = value * 10.0 + (double)(*text - '0');
        ++text;
    }
    if (*text == '.') {
        double place;

        ++text;
        place = 0.1;
        while (*text >= '0' && *text <= '9') {
            digit = 1;
            value = value + (double)(*text - '0') * place;
            place = place / 10.0;
            ++text;
        }
    }
    if (!digit) {
        if (endptr != NULL)
            *endptr = (char *)start;
        return 0.0;
    }

    if (*text == 'e' || *text == 'E') {
        const char *exponent_start;

        exponent_start = text++;
        exponent_negative = 0;
        if (*text == '+' || *text == '-') {
            exponent_negative = *text == '-';
            ++text;
        }
        exponent = 0;
        digit = 0;
        while (*text >= '0' && *text <= '9') {
            digit = 1;
            if (exponent < 100)
                exponent = exponent * 10 + (*text - '0');
            ++text;
        }
        if (!digit) {
            text = exponent_start;
            exponent = 0;
        } else if (exponent_negative) {
            exponent = -exponent;
        }
    } else {
        exponent = 0;
    }

    if (value == 0.0) {
        exponent = 0;
    } else {
        while (exponent > 0) {
            value = value * 10.0;
            --exponent;
            if (value > 65504.0) {
                errno = ERANGE;
                value = 65504.0;
                exponent = 0;
            }
        }
        while (exponent < 0) {
            value = value / 10.0;
            ++exponent;
            if (value == 0.0) {
                errno = ERANGE;
                exponent = 0;
            }
        }
    }
    if (endptr != NULL)
        *endptr = (char *)text;
    return negative ? -value : value;
}

void *malloc(size_t size)
{
    unsigned char *heap_start;
    size_t start;
    size_t run;
    size_t index;

    if (size == 0)
        return NULL;
    heap_start = __sc16_heap_start();
    start = (size_t)(heap_start - __sc16_heap);
    if (size > 512UL - start) {
        errno = ENOMEM;
        return NULL;
    }
    while (start + size <= 512UL) {
        run = 0;
        while (run < size && __sc16_heap_blocks[start + run] == 0)
            ++run;
        if (run == size) {
            __sc16_heap_blocks[start] = 1;
            for (index = 1; index < size; ++index)
                __sc16_heap_blocks[start + index] = 255;
            return (void *)(__sc16_heap + start);
        }
        start += run + 1UL;
    }
    errno = ENOMEM;
    return NULL;
}

void free(void *pointer)
{
    unsigned char *heap_start;
    unsigned char *address;
    size_t block;
    size_t count;
    size_t index;

    if (pointer == NULL)
        return;
    address = (unsigned char *)pointer;
    heap_start = __sc16_heap_start();
    if (address < heap_start || address >= __sc16_heap + 512)
        return;
    block = (size_t)(address - __sc16_heap);
    count = __sc16_heap_blocks[block];
    if (count != 1)
        return;
    __sc16_heap_blocks[block] = 0;
    for (index = block + 1; index < 512UL &&
         __sc16_heap_blocks[index] == 255; ++index)
        __sc16_heap_blocks[index] = 0;
}

void *calloc(size_t count, size_t size)
{
    size_t total;
    void *pointer;

    if (count != 0 && size > ULONG_MAX / count) {
        errno = ENOMEM;
        return NULL;
    }
    total = count * size;
    pointer = malloc(total);
    if (pointer != NULL)
        memset(pointer, 0, total);
    return pointer;
}

void *realloc(void *pointer, size_t size)
{
    unsigned char *heap_start;
    unsigned char *address;
    size_t block;
    size_t old_size;
    void *replacement;

    if (pointer == NULL)
        return malloc(size);
    if (size == 0) {
        free(pointer);
        return NULL;
    }
    address = (unsigned char *)pointer;
    heap_start = __sc16_heap_start();
    if (address < heap_start || address >= __sc16_heap + 512) {
        errno = EINVAL;
        return NULL;
    }
    block = (size_t)(address - __sc16_heap);
    if (__sc16_heap_blocks[block] != 1) {
        errno = EINVAL;
        return NULL;
    }
    old_size = 1;
    while (block + old_size < 512UL &&
           __sc16_heap_blocks[block + old_size] == 255)
        ++old_size;
    if (size <= old_size)
        return pointer;
    replacement = malloc(size);
    if (replacement == NULL)
        return NULL;
    memcpy(replacement, pointer, old_size);
    free(pointer);
    return replacement;
}

long strtol(const char *text, char **endptr, int base)
{
    const char *start;
    int negative;
    int digit;
    int any;
    unsigned long magnitude;
    unsigned long limit;

    start = text;
    while (*text == ' ' || (*text >= '\t' && *text <= '\r'))
        ++text;
    negative = 0;
    if (*text == '+' || *text == '-') {
        negative = *text == '-';
        ++text;
    }
    if (base != 0 && (base < 2 || base > 36)) {
        errno = EINVAL;
        if (endptr != NULL)
            *endptr = (char *)start;
        return 0;
    }
    if ((base == 0 || base == 16)
            && text[0] == '0' && (text[1] == 'x' || text[1] == 'X')
            && __sc16_digit_value(text[2]) >= 0
            && __sc16_digit_value(text[2]) < 16) {
        base = 16;
        text = text + 2;
    } else if (base == 0 && *text == '0') {
        base = 8;
    } else if (base == 0) {
        base = 10;
    }

    any = 0;
    magnitude = 0UL;
    limit = negative ? 2147483648UL : 2147483647UL;
    errno = 0;
    while ((digit = __sc16_digit_value(*text)) >= 0 && digit < base) {
        any = 1;
        if (magnitude > (limit - (unsigned long)digit) / (unsigned long)base) {
            errno = ERANGE;
            magnitude = limit;
        } else if (errno != ERANGE) {
            magnitude = magnitude * (unsigned long)base + (unsigned long)digit;
        }
        ++text;
    }
    if (!any) {
        if (endptr != NULL)
            *endptr = (char *)start;
        return 0;
    }
    if (endptr != NULL)
        *endptr = (char *)text;
    if (negative) {
        if (magnitude == 2147483648UL)
            return -2147483647L - 1L;
        return -(long)magnitude;
    }
    return (long)magnitude;
}

unsigned long strtoul(const char *text, char **endptr, int base)
{
    const char *start;
    int negative;
    int digit;
    int any;
    int overflow;
    unsigned long value;

    start = text;
    while (*text == ' ' || (*text >= '\t' && *text <= '\r'))
        ++text;
    negative = 0;
    if (*text == '+' || *text == '-') {
        negative = *text == '-';
        ++text;
    }
    if (base != 0 && (base < 2 || base > 36)) {
        errno = EINVAL;
        if (endptr != NULL)
            *endptr = (char *)start;
        return 0;
    }
    if ((base == 0 || base == 16)
            && text[0] == '0' && (text[1] == 'x' || text[1] == 'X')
            && __sc16_digit_value(text[2]) >= 0
            && __sc16_digit_value(text[2]) < 16) {
        base = 16;
        text = text + 2;
    } else if (base == 0 && *text == '0') {
        base = 8;
    } else if (base == 0) {
        base = 10;
    }

    any = 0;
    overflow = 0;
    value = 0;
    errno = 0;
    while ((digit = __sc16_digit_value(*text)) >= 0 && digit < base) {
        any = 1;
        if (value > (4294967295UL - (unsigned long)digit) / (unsigned long)base) {
            errno = ERANGE;
            overflow = 1;
            value = 4294967295UL;
        } else if (errno != ERANGE) {
            value = value * (unsigned long)base + (unsigned long)digit;
        }
        ++text;
    }
    if (!any) {
        if (endptr != NULL)
            *endptr = (char *)start;
        return 0;
    }
    if (endptr != NULL)
        *endptr = (char *)text;
    if (overflow)
        return 4294967295UL;
    if (negative)
        return 0UL - value;
    return value;
}

int rand(void)
{
    __sc16_rand_seed = (__sc16_rand_seed * 25173 + 13849) & RAND_MAX;
    return (int)__sc16_rand_seed;
}

void srand(unsigned int seed)
{
    __sc16_rand_seed = seed & RAND_MAX;
}

char *getenv(const char *name)
{
    (void)name;
    return NULL;
}

int system(const char *command)
{
    if (command == NULL)
        return 0;
    return -1;
}
