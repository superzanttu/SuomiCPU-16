#ifndef _SC16_STDLIB_H
#define _SC16_STDLIB_H

#include <stddef.h>

#define RAND_MAX 32767
#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1

typedef struct {
    int quot;
    int rem;
} div_t;

typedef struct {
    long quot;
    long rem;
} ldiv_t;

void abort(void);
void exit(int status);
int abs(int value);
long labs(long value);
div_t div(int numerator, int denominator);
ldiv_t ldiv(long numerator, long denominator);
int atoi(const char *text);
long atol(const char *text);
double atof(const char *text);
long strtol(const char *text, char **endptr, int base);
unsigned long strtoul(const char *text, char **endptr, int base);
double strtod(const char *text, char **endptr);
void *malloc(size_t size);
void *calloc(size_t count, size_t size);
void *realloc(void *pointer, size_t size);
void free(void *pointer);
int rand(void);
void srand(unsigned int seed);
char *getenv(const char *name);
int system(const char *command);

#endif
