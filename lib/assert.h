#ifndef _SC16_ASSERT_H
#define _SC16_ASSERT_H

#ifdef NDEBUG
#define assert(expression) ((void)0)
#else
void abort(void);
#define assert(expression) ((expression) ? 0 : (abort(), 0))
#endif

#endif
