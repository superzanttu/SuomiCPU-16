#include "preemptive_os_console.h"

#define CLOCK_MASK 1073741823UL
#define CLOCK_HALF_RANGE 536870912UL
#define SLEEP_INTERVAL 10000UL

int main(void)
{
    unsigned long pid;
    unsigned long deadline;
    unsigned long now;
    unsigned long remaining;
    unsigned long counter = 0UL;
    char message[96];
    unsigned int offset;

    pid = (unsigned long)sc16_getpid();
    deadline = ((unsigned long)sc16_monotonic_ms() + SLEEP_INTERVAL) & CLOCK_MASK;
    for (;;)
    {
        now = (unsigned long)sc16_monotonic_ms() & CLOCK_MASK;
        remaining = (deadline - now) & CLOCK_MASK;
        if (remaining != 0UL && remaining < CLOCK_HALF_RANGE)
        {
            if (sc16_sleep_ms(remaining) < 0)
            {
                write_text("sleeper: sleep failed.\n");
                sc16_exit(1);
            }
            continue;
        }
        counter++;
        offset = 0;
        offset += append_number(message, offset, pid);
        message[offset++] = ' ';
        message[offset++] = 'u';
        message[offset++] = 'p';
        message[offset++] = 't';
        message[offset++] = 'i';
        message[offset++] = 'm';
        message[offset++] = 'e';
        message[offset++] = '=';
        offset += append_number(message, offset, counter * 10UL);
        message[offset++] = 's';
        message[offset++] = ' ';
        message[offset++] = 'c';
        message[offset++] = 'o';
        message[offset++] = 'u';
        message[offset++] = 'n';
        message[offset++] = 't';
        message[offset++] = 'e';
        message[offset++] = 'r';
        message[offset++] = '=';
        offset += append_number(message, offset, counter);
        message[offset++] = '\n';
        message[offset] = 0;
        write_text(message);
        deadline = (deadline + SLEEP_INTERVAL) & CLOCK_MASK;
    }
}
