#include <time.h>
#include <stddef.h>

clock_t clock(void)
{
    return (clock_t)-1;
}

time_t time(time_t *timer)
{
    time_t unavailable;

    unavailable = (time_t)-1;
    if (timer != NULL)
        *timer = unavailable;
    return unavailable;
}
