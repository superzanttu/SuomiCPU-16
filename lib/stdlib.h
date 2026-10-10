#ifndef _SC16_STDLIB_H
#define _SC16_STDLIB_H

#include <stddef.h>

#define RAND_MAX 32767

void abort(void);
int abs(int value);
long labs(long value);
int atoi(const char *text);
long atol(const char *text);
long strtol(const char *text, char **endptr, int base);
unsigned long strtoul(const char *text, char **endptr, int base);
int rand(void);
void srand(unsigned int seed);
char *getenv(const char *name);
int system(const char *command);

#endif
