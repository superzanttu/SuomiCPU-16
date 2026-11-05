void put(unsigned char value)
{
    volatile unsigned char *uart = (volatile unsigned char *)0x10000000;
    *uart = value;
}

long factorial(long value)
{
    if (value <= 1L)
        return 1L;
    return value * factorial(value - 1L);
}

long sum(long a, long b, long c, long d, long e, long f, long g, long h)
{
    return a + b + c + d + e + f + g + h;
}

int main(void)
{
    long result = factorial(5L);
    int count = 0;
    while (count < 2) {
        put((unsigned char)(result - 45L));
        count = count + 1;
    }
    put((unsigned char)(sum(1L, 2L, 3L, 4L, 5L, 6L, 7L, factorial(3L)) + 27L));
    {
        long result = 10L;
        put((unsigned char)result);
    }
    return (int)result;
}
