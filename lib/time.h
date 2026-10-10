#ifndef _SC16_TIME_H
#define _SC16_TIME_H

typedef long clock_t;
typedef long time_t;

#define CLOCKS_PER_SEC 1

clock_t clock(void);
time_t time(time_t *timer);

#endif
