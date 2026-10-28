#include "preemptive_os_console.h"

int main(void)
{
    unsigned long duration;
    unsigned long started;
    unsigned long now;
    unsigned long elapsed;
    unsigned long second = 1UL;
    unsigned long pid;
    unsigned int offset;
    char message[80];

    duration = (unsigned long)sc16_startup_argument();
    if (duration != 0UL)
    {
        pid = (unsigned long)sc16_getpid();
        started = (unsigned long)sc16_monotonic_ms();
        while (second <= duration)
        {
            now = (unsigned long)sc16_monotonic_ms();
            elapsed = (now - started) & 1073741823UL;
            if (elapsed < second * 1000UL)
            {
                if (sc16_sleep_ms(second * 1000UL - elapsed) < 0)
                {
                    write_text("Background timer failed.\n");
                    sc16_exit(1);
                    return 1;
                }
                continue;
            }
            message[0] = 'B';
            message[1] = 'G';
            message[2] = 'P';
            message[3] = ' ';
            offset = append_number(message, 4, pid);
            message[offset++] = ' ';
            message[offset++] = 't';
            message[offset++] = 'i';
            message[offset++] = 'm';
            message[offset++] = 'e';
            message[offset++] = ' ';
            offset = append_number(message, offset, second);
            message[offset++] = '/';
            offset = append_number(message, offset, duration);
            message[offset++] = '\n';
            message[offset] = 0;
            write_text(message);
            second++;
        }
        sc16_exit(0);
        return 0;
    }
    if (sc16_write(1, "demo task ran\n", 14) < 0)
    {
        sc16_exit(1);
        return 1;
    }
    sc16_exit(7);
    return 0;
}
