int result;

int factorial(int n)
{
    if (n <= 1)
        return 1;
    return n * factorial(n - 1);
}

int main(void)
{
    int i;
    result = factorial(5);
    for (i = 0; i < 3; i++)
        result += i;
    return 0;
}
