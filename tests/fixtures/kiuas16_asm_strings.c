int main(void) {
    unsigned long address;
    unsigned char *data;
    int i;
    __asm__ volatile(
        "j .Lread_%=\n"
        ".Ldata_%=: .ascii \"S#,:\", \"\"\n"
        ".asciz \"*\", \"\"\n.string \"\\n\"\n"
        ".ascii \"\\377\\200\\x2a\\0\", \"\\042\\134\"\n"
        ".balign 4\n.Lread_%=: la %0, .Ldata_%="
        : "=r"(address));
    data = (unsigned char *)address;
    if (data[0] != 'S' || data[1] != '#' || data[2] != ',' || data[3] != ':' ||
        data[4] != 42 || data[5] != 0 || data[6] != 0 || data[7] != '\n' ||
        data[8] != 0 || data[9] != 255 || data[10] != 128 || data[11] != 42 ||
        data[12] != 0 || data[13] != '"' || data[14] != '\\') return -1;
    for (i = 0; i < 5; i++)
        *(volatile unsigned char *)0x10000000UL = data[i];
    *(volatile unsigned char *)0x10000000UL = data[7];
    *(volatile unsigned long *)0x80000000UL = address;
    *(volatile unsigned long *)0x80000008UL = 42;
    return 42;
}
