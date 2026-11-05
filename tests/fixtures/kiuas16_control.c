void put(unsigned char value)
{
    *(volatile unsigned char *)0x10000000 = value;
}

int poll(volatile unsigned int *status, unsigned int budget)
{
    unsigned int attempt;
    for (attempt = 0; attempt < budget; attempt = attempt + 1) {
        if (status && (*status & 1U)) return 1;
    }
    return 0;
}

int main(void)
{
    volatile unsigned int *status = (volatile unsigned int *)0x80000000UL;
    unsigned int total = 0;
    unsigned int row;
    unsigned int column;
    *status = 0;
    if (poll((volatile unsigned int *)0, 3)) return 1;
    if (poll(status, 3)) return 2;
    *status = 1;
    if (!poll(status, 3)) return 3;
    for (row = 0; row < 3; row = row + 1) {
        column = 0;
        do {
            column = column + 1;
            if (column == 2) continue;
            if (column == 4) break;
            total = total + row + column;
        } while (column < 5);
    }
    put(76);
    put(total == 18U ? total : 255);
    put(10);
    return total;
}
