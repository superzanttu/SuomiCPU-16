#include <stdlib.h>

unsigned int __sc16_rand_seed;

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
    int sign;
    int value;
    sign = 1;
    value = 0;
    while (*text == ' ' || (*text >= '\t' && *text <= '\r'))
        ++text;
    if (*text == '+' || *text == '-') {
        if (*text == '-')
            sign = -1;
        ++text;
    }
    while (*text >= '0' && *text <= '9') {
        value = value * 10 + (*text - '0');
        ++text;
    }
    return sign * value;
}

long atol(const char *text)
{
    return (long)atoi(text);
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
