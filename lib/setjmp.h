#ifndef _SC16_SETJMP_H
#define _SC16_SETJMP_H

typedef unsigned char jmp_buf[6];

int __sc16_setjmp(unsigned char *environment);
#define setjmp(environment) __sc16_setjmp(environment)
void longjmp(unsigned char *environment, int value);

#endif
