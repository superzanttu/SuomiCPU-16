#include "preemptive_os_console.h"

static long background[16];

static void reap_background(void)
{
    int index;
    int result;

    for (index = 0; index < 16; index++)
    {
        if (background[index] == 0L)
            continue;
        result = sc16_trywait((unsigned long)background[index]);
        if (result == -11)
            continue;
        if (result != 0)
            write_text("Background demo failed.\n");
        background[index] = 0L;
    }
}

static void show_processes(void)
{
    unsigned int index;
    char row[64];
    int length;

    write_text("PID PPID STATE\n");
    for (index = 0; ; index++)
    {
        length = sc16_process_info(index, row, sizeof(row) - 1);
        if (length == 0)
            return;
        if (length < 0)
        {
            write_text("Unable to list processes.\n");
            return;
        }
        row[length] = 0;
        write_text(row);
    }
}

static void start_background(const char *duration)
{
    unsigned long seconds = 0UL;
    unsigned int index;
    int slot;
    long child;
    char message[64];

    for (index = 0; duration[index] != 0; index++)
    {
        if (duration[index] < '0' || duration[index] > '9' || seconds > 6553UL)
            break;
        seconds = seconds * 10UL + (unsigned long)(duration[index] - '0');
        if (seconds > 65535UL)
            break;
    }
    if (index == 0 || duration[index] != 0 || seconds == 0UL || seconds > 65535UL)
    {
        write_text("Usage: run demo X (X = 1..65535 seconds)\n");
        return;
    }
    for (slot = 0; slot < 16 && background[slot] != 0L; slot++)
    {
    }
    if (slot == 16)
    {
        write_text("All 16 background slots are busy.\n");
        return;
    }
    child = sc16_spawn_arg("demo.bin", seconds);
    if (child < 0L)
    {
        write_text("Unable to start background demo.\n");
        return;
    }
    background[slot] = child;
    write_text("Started background process ");
    append_number(message, 0, (unsigned long)child);
    write_text(message);
    write_text("\n");
}

static int strings_equal(const char *left, const char *right)
{
    unsigned int index;

    for (index = 0; left[index] == right[index]; index++)
    {
        if (left[index] == 0)
            return 1;
    }
    return 0;
}

static int read_command(char *buffer, unsigned int capacity)
{
    unsigned int length = 0;
    unsigned int excess = 0;
    int escape = 0;

    for (;;)
    {
        unsigned char character;
        int result;

        reap_background();
        result = sc16_read(0, &character, 1);

        if (result < 0)
            return result;
        if (result == 0)
        {
            sc16_yield();
            continue;
        }
        if (escape != 0)
        {
            if (escape == 1 && character == '[')
                escape = 2;
            else if (escape == 2 && character >= 64 && character <= 126)
                escape = 0;
            else if (escape == 1)
                escape = 0;
            continue;
        }
        if (character == 27)
        {
            escape = 1;
            continue;
        }
        if (character == '\r' || character == '\n')
        {
            buffer[length] = 0;
            return excess == 0 ? (int)length : -2;
        }
        if (character == 8 || character == 127)
        {
            if (excess != 0)
                excess--;
            else if (length != 0)
                buffer[--length] = 0;
            continue;
        }
        if (character < 32 || character > 126)
            continue;
        if (length + 1 < capacity)
        {
            buffer[length++] = (char)character;
            buffer[length] = 0;
        }
        else
        {
            excess++;
        }
    }
    return -1;
}

static void run_demo(void)
{
    long child;
    int status;
    volatile int work;

    child = sc16_spawn("demo.bin");
    if (child < 0)
    {
        write_text("Unable to start demo.\n");
        return;
    }
    write_text("Running demo...\n");
    for (work = 0; work < 64; work++)
    {
    }
    write_text("Waiting for demo...\n");
    status = sc16_wait((unsigned long)child);
    if (status < 0)
        write_text("Waiting for demo failed.\n");
    else if (status == 7)
        write_text("demo exited with status 7\n");
    else
        write_text("demo returned an unexpected status\n");
}

int main(void)
{
    char command[80];
    int result;

    write_text("SisuOS interactive shell\n");
    write_text("Type 'run demo' to see preemptive task switching.\n");
    write_text("Commands: help, list, run demo, info, ps, run demo X\n");
    for (;;)
    {
        write_text("sc16> ");
        result = read_command(command, sizeof(command));
        if (result == -2)
        {
            write_text("Command too long (79 characters maximum)\n");
            continue;
        }
        if (result < 0)
        {
            write_text("Console input failed.\n");
            sc16_yield();
            continue;
        }
        if (strings_equal(command, "help"))
        {
            write_text("Commands: help, list, run demo, info, ps, run demo X\n");
        }
        else if (strings_equal(command, "list"))
        {
            write_text("Programs: demo\n");
        }
        else if (strings_equal(command, "info"))
        {
            write_text("SisuOS | Protected mode | Syscall ABI 1 | Preemptive scheduler active\n");
        }
        else if (strings_equal(command, "run demo"))
        {
            run_demo();
        }
        else if (strings_equal(command, "ps"))
        {
            show_processes();
        }
        else if (command[0] == 'r' && command[1] == 'u' && command[2] == 'n' &&
                 command[3] == ' ' && command[4] == 'd' && command[5] == 'e' &&
                 command[6] == 'm' && command[7] == 'o' && command[8] == ' ')
        {
            start_background(command + 9);
        }
        else if (command[0] != 0)
        {
            write_text("Unknown command. Type help.\n");
        }
    }
}
