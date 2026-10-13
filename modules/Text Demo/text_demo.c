#include <stdio.h>

int main(void)
{
    char name[24];
    int i;

    printf("Kuka olet? ");
    if (fgets(name, sizeof(name), stdin) == 0)
    {
        puts("Nimen lukeminen epaonnistui.");
        return 1;
    }
    for (i = 0; name[i] != 0 && name[i] != '\n'; i++)
    {
    }
    name[i] = 0;
    printf("Huomenta! Kohta kouluun, %s!\n", name);
    return 0;
}
