#include <stdlib.h>
#include <errno.h>

unsigned int __sc16_rand_seed;
int errno;

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

int atoi(const char *text)
{
    return (int)strtol(text, NULL, 10);
}

long atol(const char *text)
{
    return strtol(text, NULL, 10);
}

long strtol(const char *text, char **endptr, int base)
{
    const char *start;
    int negative;
    int digit;
    int any;
    long value;
    long limit;

    start = text;
    while (*text == ' ' || (*text >= '\t' && *text <= '\r'))
        ++text;
    negative = 0;
    if (*text == '+' || *text == '-') {
        negative = *text == '-';
        ++text;
    }
    if (base != 0 && (base < 2 || base > 36)) {
        errno = EDOM;
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
    value = 0;
    limit = negative ? 32768L : 32767L;
    errno = 0;
    while ((digit = __sc16_digit_value(*text)) >= 0 && digit < base) {
        any = 1;
        if (value > (limit - digit) / base) {
            errno = ERANGE;
            value = limit;
        } else if (errno != ERANGE) {
            value = value * base + digit;
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
    if (negative)
        return -value;
    return value;
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
        errno = EDOM;
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
        if (value > (65535UL - digit) / base) {
            errno = ERANGE;
            overflow = 1;
            value = 65535UL;
        } else if (errno != ERANGE) {
            value = value * base + digit;
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
        return 65535UL;
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
