#ifndef _SC16_MATH_H
#define _SC16_MATH_H

#include <errno.h>

#define HUGE_VAL (65504.0)

double acos(double value);
double asin(double value);
double atan(double value);
double atan2(double y, double x);
double cos(double value);
double sin(double value);
double tan(double value);
double cosh(double value);
double sinh(double value);
double tanh(double value);
double exp(double value);
double frexp(double value, int *exponent);
double ldexp(double value, int exponent);
double log(double value);
double log10(double value);
double modf(double value, double *integer_part);
double pow(double base, double exponent);
double sqrt(double value);
double ceil(double value);
double fabs(double value);
double floor(double value);
double fmod(double x, double y);

#endif
