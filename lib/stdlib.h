#ifndef _SC16_STDLIB_H
#define _SC16_STDLIB_H

#define RAND_MAX 32767

int abs(int value);
long labs(long value);
int atoi(const char *text);
long atol(const char *text);
int rand(void);
void srand(unsigned int seed);

#endif
