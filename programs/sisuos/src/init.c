#include "preemptive_os_console.h"

static long shells[4];
static int account_error_reported;

int main(void)
{
    unsigned int console;
    int status;
    int accounts_ready;

    shells[0] = sc16_spawn("bin/sauna.bin");
    for (;;)
    {
        status = sc16_account_status();
        if (status < 0 && !account_error_reported)
        {
            write_text("SisuOS init: cannot read account status.\n");
            account_error_reported = 1;
        }
        accounts_ready = status > 0;
        for (console = 0; console < 4; console++)
        {
            if (shells[console] > 0L)
            {
                status = sc16_trywait((unsigned long)shells[console]);
                if (status != -11)
                    shells[console] = 0L;
            }
            if (shells[console] == 0L && (console == 0 || accounts_ready))
            {
                shells[console] = console == 0
                                     ? sc16_spawn("bin/sauna.bin")
                                     : sc16_spawn_console("bin/sauna.bin", console);
            }
            if (shells[console] < 0L)
            {
                write_text("SisuOS init: cannot start a console shell.\n");
                shells[console] = 0L;
            }
        }
        sc16_sleep_ms(100UL);
    }
}
