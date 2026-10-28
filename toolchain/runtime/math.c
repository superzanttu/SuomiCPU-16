#include <math.h>

#define SC16_PI 3.14159265358979323846
#define SC16_HALF_PI 1.57079632679489661923
#define SC16_TWO_PI 6.28318530717958647692
#define SC16_LN2 0.69314718055994530942

double fabs(double value)
{
    return value < 0.0 ? -value : value;
}

double floor(double value)
{
    long integral;

    integral = (long)value;
    if (value < 0.0 && (double)integral != value)
        return (double)integral - 1.0;
    return (double)integral;
}

double ceil(double value)
{
    long integral;

    integral = (long)value;
    if (value > 0.0 && (double)integral != value)
        return (double)integral + 1.0;
    return (double)integral;
}

double modf(double value, double *integer_part)
{
    long integral;

    integral = (long)value;
    *integer_part = (double)integral;
    return value - *integer_part;
}

double fmod(double x, double y)
{
    double quotient;

    if (y == 0.0) {
        errno = EDOM;
        return 0.0;
    }
    quotient = (double)(long)(x / y);
    return x - quotient * y;
}

double frexp(double value, int *exponent)
{
    int scale;

    if (value == 0.0) {
        *exponent = 0;
        return value;
    }
    scale = 0;
    while (fabs(value) >= 1.0) {
        value = value / 2.0;
        ++scale;
    }
    while (fabs(value) < 0.5) {
        value = value * 2.0;
        --scale;
    }
    *exponent = scale;
    return value;
}

double ldexp(double value, int exponent)
{
    if (exponent > 0) {
        while (exponent != 0) {
            value = value * 2.0;
            --exponent;
        }
    } else {
        while (exponent != 0) {
            value = value / 2.0;
            ++exponent;
        }
    }
    return value;
}

double sqrt(double value)
{
    double estimate;
    int iteration;

    if (value < 0.0) {
        errno = EDOM;
        return 0.0;
    }
    if (value == 0.0)
        return 0.0;
    estimate = value >= 1.0 ? value : 1.0;
    for (iteration = 0; iteration < 24; ++iteration)
        estimate = (estimate + value / estimate) / 2.0;
    return estimate;
}

double exp(double value)
{
    double term;
    double sum;
    int iteration;
    int squarings;

    if (value > 10.0) {
        errno = ERANGE;
        return HUGE_VAL;
    }
    if (value < -10.0) {
        errno = ERANGE;
        return 0.0;
    }
    squarings = 4;
    value = value / 16.0;
    term = 1.0;
    sum = 1.0;
    for (iteration = 1; iteration <= 12; ++iteration) {
        term = term * value / (double)iteration;
        sum = sum + term;
    }
    for (iteration = 0; iteration < squarings; ++iteration)
        sum = sum * sum;
    return sum;
}

double log(double value)
{
    double z;
    double z_squared;
    double term;
    double sum;
    int exponent;
    int iteration;

    if (value <= 0.0) {
        errno = EDOM;
        return 0.0;
    }
    exponent = 0;
    while (value >= 1.0) {
        value = value / 2.0;
        ++exponent;
    }
    while (value < 0.5) {
        value = value * 2.0;
        --exponent;
    }
    z = (value - 1.0) / (value + 1.0);
    z_squared = z * z;
    term = z;
    sum = term;
    for (iteration = 3; iteration <= 31; iteration += 2) {
        term = term * z_squared;
        sum = sum + term / (double)iteration;
    }
    return 2.0 * sum + (double)exponent * SC16_LN2;
}

double log10(double value)
{
    return log(value) / 2.30258509299404568402;
}

double pow(double base, double exponent)
{
    int integer_exponent;
    int count;
    double result;
    int negative;

    if (base == 0.0 && exponent <= 0.0) {
        errno = EDOM;
        return 0.0;
    }
    integer_exponent = (int)exponent;
    if (base < 0.0 && (double)integer_exponent != exponent) {
        errno = EDOM;
        return 0.0;
    }
    if ((double)integer_exponent == exponent && fabs(exponent) < 32.0) {
        negative = integer_exponent < 0;
        count = negative ? -integer_exponent : integer_exponent;
        result = 1.0;
        while (count != 0) {
            result = result * base;
            --count;
        }
        return negative ? 1.0 / result : result;
    }
    if (base < 0.0) {
        base = -base;
        if ((double)integer_exponent == exponent && integer_exponent % 2 != 0)
            return -exp(exponent * log(base));
    }
    return exp(exponent * log(base));
}

double atan(double value)
{
    double reduced;
    double square;
    double term;
    double sum;
    int reciprocal;
    int iteration;

    reciprocal = fabs(value) > 1.0;
    reduced = reciprocal ? 1.0 / value : value;
    square = reduced * reduced;
    term = reduced;
    sum = term;
    for (iteration = 3; iteration <= 31; iteration += 2) {
        term = term * square;
        if ((iteration / 2) % 2)
            sum = sum - term / (double)iteration;
        else
            sum = sum + term / (double)iteration;
    }
    if (reciprocal)
        sum = value < 0.0 ? -SC16_HALF_PI - sum : SC16_HALF_PI - sum;
    return sum;
}

double atan2(double y, double x)
{
    if (x > 0.0)
        return atan(y / x);
    if (x < 0.0)
        return y < 0.0 ? atan(y / x) - SC16_PI : atan(y / x) + SC16_PI;
    if (y > 0.0)
        return SC16_HALF_PI;
    if (y < 0.0)
        return -SC16_HALF_PI;
    return 0.0;
}

double sin(double value)
{
    double square;
    double term;
    double sum;
    int iteration;

    value = value - floor((value + SC16_PI) / SC16_TWO_PI) * SC16_TWO_PI;
    square = value * value;
    term = value;
    sum = term;
    for (iteration = 3; iteration <= 13; iteration += 2) {
        term = -term * square / ((double)(iteration - 1) * iteration);
        sum = sum + term;
    }
    return sum;
}

double cos(double value)
{
    return sin(value + SC16_HALF_PI);
}

double tan(double value)
{
    double cosine;

    cosine = cos(value);
    if (cosine == 0.0) {
        errno = EDOM;
        return 0.0;
    }
    return sin(value) / cosine;
}

double asin(double value)
{
    if (value < -1.0 || value > 1.0) {
        errno = EDOM;
        return 0.0;
    }
    return atan2(value, sqrt(1.0 - value * value));
}

double acos(double value)
{
    if (value < -1.0 || value > 1.0) {
        errno = EDOM;
        return 0.0;
    }
    return atan2(sqrt(1.0 - value * value), value);
}

double sinh(double value)
{
    return (exp(value) - exp(-value)) / 2.0;
}

double cosh(double value)
{
    return (exp(value) + exp(-value)) / 2.0;
}

double tanh(double value)
{
    double positive;
    double negative;

    positive = exp(value);
    negative = exp(-value);
    return (positive - negative) / (positive + negative);
}
