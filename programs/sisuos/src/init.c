#include "preemptive_os_console.h"

int main(void)
{
    long shell;
    int status;

    for (;;)
    {
        shell = sc16_spawn("bin/sauna.bin");
        if (shell < 0L)
        {
            write_text("SisuOS init: cannot start SaunaShell.\n");
            sc16_sleep_ms(1000UL);
            continue;
        }
        status = sc16_wait((unsigned long)shell);
        if (status < 0)
            write_text("SisuOS init: SaunaShell wait failed.\n");
    }
}
