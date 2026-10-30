#include "preemptive_os_console.h"

int main(void)
{
    unsigned long pid;
    unsigned long started;
    char message[64];
    unsigned int offset = 0;

    pid = (unsigned long)sc16_getpid();
    started = (unsigned long)sc16_monotonic_ms();
    message[offset++] = 'd';
    message[offset++] = 'a';
    message[offset++] = 'e';
    message[offset++] = 'm';
    message[offset++] = 'o';
    message[offset++] = 'n';
    message[offset++] = ' ';
    message[offset++] = 'p';
    message[offset++] = 'i';
    message[offset++] = 'd';
    message[offset++] = '=';
    offset = append_number(message, offset, pid);
    message[offset++] = ' ';
    message[offset++] = 's';
    message[offset++] = 't';
    message[offset++] = 'a';
    message[offset++] = 'r';
    message[offset++] = 't';
    message[offset++] = 'e';
    message[offset++] = 'd';
    message[offset++] = '_';
    message[offset++] = 'm';
    message[offset++] = 's';
    message[offset++] = '=';
    offset = append_number(message, offset, started);
    message[offset++] = '\n';
    message[offset] = 0;
    write_text(message);
    for (;;)
    {
        if (sc16_sleep_ms(60000UL) < 0)
        {
            write_text("daemon: sleep failed.\n");
            sc16_exit(1);
        }
    }
}
