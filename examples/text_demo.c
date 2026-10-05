void SCREEN_CLEAR(void);
void SCREEN_SET_CURSOR(unsigned char column, unsigned char row);
int SCREEN_GET_CURSOR(void);
void SCREEN_PRINT(char *text, unsigned char color);
unsigned int SCREEN_INPUT(char *buffer, unsigned int capacity, unsigned char color);

unsigned int name_length;
int cursor_position;

int main(void)
{
    char name[24];

    SCREEN_CLEAR();
    SCREEN_PRINT("Kuka olet? ", 1);
    name_length = SCREEN_INPUT(name, 23, 1);

    SCREEN_SET_CURSOR(0, 2);
    SCREEN_PRINT("Huomenta! Kohta kouluun, ", 1);
    SCREEN_PRINT(name, 1);
    cursor_position = SCREEN_GET_CURSOR();
    return 0;
}
