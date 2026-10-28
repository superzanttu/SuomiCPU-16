#include <time.h>
#include <stddef.h>
#include <string.h>

static struct tm __sc16_time_value;
static char __sc16_asctime_buffer[26];
static char __sc16_strftime_buffer[256];
static const int __sc16_month_lengths[12] =
    {31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31};

static const char *__sc16_weekday_name(int day)
{
    if (day == 0) return "Sunday";
    if (day == 1) return "Monday";
    if (day == 2) return "Tuesday";
    if (day == 3) return "Wednesday";
    if (day == 4) return "Thursday";
    if (day == 5) return "Friday";
    return "Saturday";
}

static const char *__sc16_weekday_abbreviation(int day)
{
    if (day == 0) return "Sun";
    if (day == 1) return "Mon";
    if (day == 2) return "Tue";
    if (day == 3) return "Wed";
    if (day == 4) return "Thu";
    if (day == 5) return "Fri";
    return "Sat";
}

static const char *__sc16_month_name(int month)
{
    if (month == 0) return "January";
    if (month == 1) return "February";
    if (month == 2) return "March";
    if (month == 3) return "April";
    if (month == 4) return "May";
    if (month == 5) return "June";
    if (month == 6) return "July";
    if (month == 7) return "August";
    if (month == 8) return "September";
    if (month == 9) return "October";
    if (month == 10) return "November";
    return "December";
}

static const char *__sc16_month_abbreviation(int month)
{
    if (month == 0) return "Jan";
    if (month == 1) return "Feb";
    if (month == 2) return "Mar";
    if (month == 3) return "Apr";
    if (month == 4) return "May";
    if (month == 5) return "Jun";
    if (month == 6) return "Jul";
    if (month == 7) return "Aug";
    if (month == 8) return "Sep";
    if (month == 9) return "Oct";
    if (month == 10) return "Nov";
    return "Dec";
}

static int __sc16_leap_year(int year)
{
    return year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
}

static int __sc16_month_length(int year, int month)
{
    if (month == 1 && __sc16_leap_year(year))
        return 29;
    return __sc16_month_lengths[month];
}

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

double difftime(time_t time1, time_t time0)
{
    return (double)time1 - (double)time0;
}

struct tm *gmtime(const time_t *timer)
{
    long days;
    long seconds;
    int year;
    int month;
    int day_of_week;

    if (timer == NULL)
        return NULL;
    days = *timer / 86400L;
    seconds = *timer % 86400L;
    if (seconds < 0) {
        seconds += 86400L;
        --days;
    }
    year = 1970;
    while (days < 0) {
        --year;
        days += __sc16_leap_year(year) ? 366L : 365L;
    }
    while (days >= (__sc16_leap_year(year) ? 366L : 365L)) {
        days -= __sc16_leap_year(year) ? 366L : 365L;
        ++year;
    }
    __sc16_time_value.tm_yday = (int)days;
    month = 0;
    while (month < 11 &&
           days >= __sc16_month_length(year, month)) {
        days -= __sc16_month_length(year, month);
        ++month;
    }
    __sc16_time_value.tm_year = year - 1900;
    __sc16_time_value.tm_mon = month;
    __sc16_time_value.tm_mday = (int)days + 1;
    __sc16_time_value.tm_hour = (int)(seconds / 3600L);
    seconds %= 3600L;
    __sc16_time_value.tm_min = (int)(seconds / 60L);
    __sc16_time_value.tm_sec = (int)(seconds % 60L);
    day_of_week = (int)((*timer / 86400L + 4L) % 7L);
    if (day_of_week < 0)
        day_of_week += 7;
    __sc16_time_value.tm_wday = day_of_week;
    __sc16_time_value.tm_isdst = 0;
    return &__sc16_time_value;
}

struct tm *localtime(const time_t *timer)
{
    return gmtime(timer);
}

time_t mktime(struct tm *timeptr)
{
    long days;
    long seconds;
    int year;
    int month;

    if (timeptr == NULL)
        return (time_t)-1;
    year = timeptr->tm_year + 1900;
    month = timeptr->tm_mon;
    while (month < 0) {
        month += 12;
        --year;
    }
    while (month >= 12) {
        month -= 12;
        ++year;
    }
    days = 0;
    while (year > 1970) {
        --year;
        days += __sc16_leap_year(year) ? 366L : 365L;
    }
    while (year < 1970) {
        days -= __sc16_leap_year(year) ? 366L : 365L;
        ++year;
    }
    while (month > 0) {
        --month;
        days += __sc16_month_length(year, month);
    }
    days += (long)timeptr->tm_mday - 1L;
    seconds = days * 86400L + (long)timeptr->tm_hour * 3600L +
              (long)timeptr->tm_min * 60L + (long)timeptr->tm_sec;
    {
        time_t result;
        struct tm *normalized;

        result = (time_t)seconds;
        normalized = gmtime(&result);
        if (normalized != NULL)
            *timeptr = *normalized;
        return result;
    }
}

char *asctime(const struct tm *timeptr)
{
    int year;
    int month;
    int day;
    int hour;
    int minute;
    int second;

    if (timeptr == NULL || timeptr->tm_wday < 0 || timeptr->tm_wday > 6 ||
        timeptr->tm_mon < 0 || timeptr->tm_mon > 11)
        return NULL;
    year = timeptr->tm_year + 1900;
    month = timeptr->tm_mon;
    day = timeptr->tm_mday;
    hour = timeptr->tm_hour;
    minute = timeptr->tm_min;
    second = timeptr->tm_sec;
    __sc16_asctime_buffer[0] = __sc16_weekday_abbreviation(timeptr->tm_wday)[0];
    __sc16_asctime_buffer[1] = __sc16_weekday_abbreviation(timeptr->tm_wday)[1];
    __sc16_asctime_buffer[2] = __sc16_weekday_abbreviation(timeptr->tm_wday)[2];
    __sc16_asctime_buffer[3] = ' ';
    __sc16_asctime_buffer[4] = __sc16_month_abbreviation(month)[0];
    __sc16_asctime_buffer[5] = __sc16_month_abbreviation(month)[1];
    __sc16_asctime_buffer[6] = __sc16_month_abbreviation(month)[2];
    __sc16_asctime_buffer[7] = ' ';
    __sc16_asctime_buffer[8] = day < 10 ? ' ' : (char)('0' + day / 10);
    __sc16_asctime_buffer[9] = (char)('0' + day % 10);
    __sc16_asctime_buffer[10] = ' ';
    __sc16_asctime_buffer[11] = (char)('0' + hour / 10);
    __sc16_asctime_buffer[12] = (char)('0' + hour % 10);
    __sc16_asctime_buffer[13] = ':';
    __sc16_asctime_buffer[14] = (char)('0' + minute / 10);
    __sc16_asctime_buffer[15] = (char)('0' + minute % 10);
    __sc16_asctime_buffer[16] = ':';
    __sc16_asctime_buffer[17] = (char)('0' + second / 10);
    __sc16_asctime_buffer[18] = (char)('0' + second % 10);
    __sc16_asctime_buffer[19] = ' ';
    __sc16_asctime_buffer[20] = (char)('0' + (year / 1000) % 10);
    __sc16_asctime_buffer[21] = (char)('0' + (year / 100) % 10);
    __sc16_asctime_buffer[22] = (char)('0' + (year / 10) % 10);
    __sc16_asctime_buffer[23] = (char)('0' + year % 10);
    __sc16_asctime_buffer[24] = '\n';
    __sc16_asctime_buffer[25] = '\0';
    return __sc16_asctime_buffer;
}

char *ctime(const time_t *timer)
{
    return asctime(localtime(timer));
}

static int __sc16_append(char *text, int *length, const char *source)
{
    while (*source != '\0') {
        if (*length >= 255)
            return 0;
        text[(*length)++] = *source++;
    }
    return 1;
}

static int __sc16_append_number(char *text, int *length, int value,
                                int width)
{
    char digits[12];
    int count;
    int index;

    count = 0;
    if (value < 0) {
        if (*length >= 255)
            return 0;
        text[(*length)++] = '-';
        value = -value;
        --width;
    }
    do {
        digits[count++] = (char)('0' + value % 10);
        value /= 10;
    } while (value != 0 && count < 11);
    while (count < width) {
        if (*length >= 255)
            return 0;
        text[(*length)++] = '0';
        --width;
    }
    for (index = count - 1; index >= 0; --index) {
        if (*length >= 255)
            return 0;
        text[(*length)++] = digits[index];
    }
    return 1;
}

size_t strftime(char *text, size_t maxsize, const char *format,
                const struct tm *timeptr)
{
    char *result;
    int length;
    int value;

    if (text == NULL || format == NULL || timeptr == NULL || maxsize == 0)
        return 0;
    result = __sc16_strftime_buffer;
    length = 0;
    while (*format != '\0') {
        if (*format != '%') {
            if (length >= 255)
                return 0;
            result[length++] = *format++;
            continue;
        }
        ++format;
        if (*format == 'E' || *format == 'O')
            ++format;
        if (*format == '\0')
            return 0;
        switch (*format++) {
        case '%':
            if (length >= 255)
                return 0;
            result[length++] = '%';
            break;
        case 'a':
            if (timeptr->tm_wday < 0 || timeptr->tm_wday > 6 ||
                !__sc16_append(result, &length,
                               __sc16_weekday_abbreviation(timeptr->tm_wday)))
                return 0;
            break;
        case 'A':
            if (timeptr->tm_wday < 0 || timeptr->tm_wday > 6 ||
                !__sc16_append(result, &length,
                               __sc16_weekday_name(timeptr->tm_wday)))
                return 0;
            break;
        case 'b':
        case 'h':
            if (timeptr->tm_mon < 0 || timeptr->tm_mon > 11 ||
                !__sc16_append(result, &length,
                               __sc16_month_abbreviation(timeptr->tm_mon)))
                return 0;
            break;
        case 'B':
            if (timeptr->tm_mon < 0 || timeptr->tm_mon > 11 ||
                !__sc16_append(result, &length,
                               __sc16_month_name(timeptr->tm_mon)))
                return 0;
            break;
        case 'c':
            if (!__sc16_append(result, &length, asctime(timeptr)))
                return 0;
            --length;
            break;
        case 'd': if (!__sc16_append_number(result, &length, timeptr->tm_mday, 2)) return 0; break;
        case 'H': if (!__sc16_append_number(result, &length, timeptr->tm_hour, 2)) return 0; break;
        case 'I':
            value = timeptr->tm_hour % 12;
            if (value == 0)
                value = 12;
            if (!__sc16_append_number(result, &length, value, 2))
                return 0;
            break;
        case 'j': if (!__sc16_append_number(result, &length, timeptr->tm_yday + 1, 3)) return 0; break;
        case 'm': if (!__sc16_append_number(result, &length, timeptr->tm_mon + 1, 2)) return 0; break;
        case 'M': if (!__sc16_append_number(result, &length, timeptr->tm_min, 2)) return 0; break;
        case 'p':
            if (!__sc16_append(result, &length,
                               timeptr->tm_hour < 12 ? "AM" : "PM"))
                return 0;
            break;
        case 'S': if (!__sc16_append_number(result, &length, timeptr->tm_sec, 2)) return 0; break;
        case 'U':
            if (!__sc16_append_number(result, &length,
                    (timeptr->tm_yday + 7 - timeptr->tm_wday) / 7, 2))
                return 0;
            break;
        case 'w': if (!__sc16_append_number(result, &length, timeptr->tm_wday, 1)) return 0; break;
        case 'W':
            value = (timeptr->tm_yday + 7 -
                     ((timeptr->tm_wday + 6) % 7)) / 7;
            if (!__sc16_append_number(result, &length, value, 2))
                return 0;
            break;
        case 'x':
            if (!__sc16_append_number(result, &length, timeptr->tm_mon + 1, 2) ||
                !__sc16_append(result, &length, "/") ||
                !__sc16_append_number(result, &length, timeptr->tm_mday, 2) ||
                !__sc16_append(result, &length, "/") ||
                !__sc16_append_number(result, &length,
                                      (timeptr->tm_year + 1900) % 100, 2))
                return 0;
            break;
        case 'X':
            if (!__sc16_append_number(result, &length, timeptr->tm_hour, 2) ||
                !__sc16_append(result, &length, ":") ||
                !__sc16_append_number(result, &length, timeptr->tm_min, 2) ||
                !__sc16_append(result, &length, ":") ||
                !__sc16_append_number(result, &length, timeptr->tm_sec, 2))
                return 0;
            break;
        case 'y':
            if (!__sc16_append_number(result, &length,
                                      (timeptr->tm_year + 1900) % 100, 2))
                return 0;
            break;
        case 'Y':
            if (!__sc16_append_number(result, &length,
                                      timeptr->tm_year + 1900, 4))
                return 0;
            break;
        default:
            return 0;
        }
    }
    if ((size_t)length + 1UL > maxsize)
        return 0;
    result[length] = '\0';
    memcpy(text, result, (size_t)length + 1UL);
    return (size_t)length;
}
