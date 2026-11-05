long sum(long a, long b, long c, long d, long e, long f, long g, long h, long i, long j)
{
    return a + b + c + d + e + f + g + h + i + j;
}

long recurse(int depth, long a, long b, long c, long d, long e, long f, long g, long h, long i)
{
    if (depth == 0) return a + b + c + d + e + f + g + h + i;
    return recurse(depth - 1, a, b, c, d, e, f, g, h, i) + 1;
}

int main(void)
{
    long result = sum(1, 2, 3, 4, 5, 6, 7, 8, recurse(3, 1, 2, 3, 4, 5, 6, 7, 8, 9), sum(1, 2, 3, 4, 5, 6, 7, 8, 9, 10));
    *(volatile unsigned char *)0x10000000 = result;
    return result;
}
