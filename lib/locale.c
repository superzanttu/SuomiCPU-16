#include <locale.h>
#include <limits.h>

char __sc16_locale_name[2] = "C";
struct lconv __sc16_locale_data;

char *setlocale(int category, const char *locale)
{
    if (category < LC_ALL || category > LC_TIME)
        return NULL;
    if (locale == NULL)
        return __sc16_locale_name;
    if (locale[0] == 'C' && locale[1] == '\0')
        return __sc16_locale_name;
    return NULL;
}

struct lconv *localeconv(void)
{
    __sc16_locale_data.decimal_point = ".";
    __sc16_locale_data.thousands_sep = "";
    __sc16_locale_data.grouping = "";
    __sc16_locale_data.int_curr_symbol = "";
    __sc16_locale_data.currency_symbol = "";
    __sc16_locale_data.mon_decimal_point = "";
    __sc16_locale_data.mon_thousands_sep = "";
    __sc16_locale_data.mon_grouping = "";
    __sc16_locale_data.positive_sign = "";
    __sc16_locale_data.negative_sign = "";
    __sc16_locale_data.int_frac_digits = CHAR_MAX;
    __sc16_locale_data.frac_digits = CHAR_MAX;
    __sc16_locale_data.p_cs_precedes = CHAR_MAX;
    __sc16_locale_data.p_sep_by_space = CHAR_MAX;
    __sc16_locale_data.n_cs_precedes = CHAR_MAX;
    __sc16_locale_data.n_sep_by_space = CHAR_MAX;
    __sc16_locale_data.p_sign_posn = CHAR_MAX;
    __sc16_locale_data.n_sign_posn = CHAR_MAX;
    __sc16_locale_data.int_p_cs_precedes = CHAR_MAX;
    __sc16_locale_data.int_p_sep_by_space = CHAR_MAX;
    __sc16_locale_data.int_n_cs_precedes = CHAR_MAX;
    __sc16_locale_data.int_n_sep_by_space = CHAR_MAX;
    __sc16_locale_data.int_p_sign_posn = CHAR_MAX;
    __sc16_locale_data.int_n_sign_posn = CHAR_MAX;
    return &__sc16_locale_data;
}
