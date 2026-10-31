#include "preemptive_os_console.h"

int main(void)
{
    volatile unsigned long value = 1UL;

    for (;;)
    {
        value = value * 1664525UL + 1013904223UL;
        sc16_yield();
    }
}
