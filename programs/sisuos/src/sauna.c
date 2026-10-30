#include "preemptive_os_console.h"

#define LINE_CAPACITY 256
#define ARGUMENT_CAPACITY 1024
#define MAX_BACKGROUND 16
#define ARGUMENT(index) (line + argument_offsets[(index)])

static long background[MAX_BACKGROUND];
static unsigned int argument_offsets[SC16_MAX_ARGUMENTS];
static unsigned char pending_input[LINE_CAPACITY];
static unsigned int pending_input_length;
static unsigned int pending_input_offset;
static char setup_password[129];
static char login_name[33];
static char session_name[33];
static char admin_name[33];
static char admin_group[33];

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

static int copy_text(char *destination, unsigned int capacity, const char *source)
{
    unsigned int index;

    for (index = 0; source[index] != 0; index++)
    {
        if (index + 1 >= capacity)
            return 0;
        destination[index] = source[index];
    }
    destination[index] = 0;
    return 1;
}

static unsigned int text_length(const char *text)
{
    unsigned int length = 0;

    while (text[length] != 0)
        length++;
    return length;
}

static unsigned int append_signed_number(char *buffer, unsigned int offset, int value)
{
    unsigned int magnitude;

    if (value < 0)
    {
        buffer[offset++] = '-';
        magnitude = (unsigned int)(-value);
    }
    else
    {
        magnitude = (unsigned int)value;
    }
    return append_number(buffer, offset, (unsigned long)magnitude);
}

static int parse_octal_mode(const char *text)
{
    unsigned int index;
    unsigned int value = 0;

    for (index = 0; text[index] != 0; index++)
    {
        if (index == 3 || text[index] < '0' || text[index] > '7')
            return -1;
        value = value * 8 + (unsigned int)(text[index] - '0');
    }
    return index == 0 ? -1 : (int)value;
}

static long parse_process_id(const char *text)
{
    unsigned int index;
    unsigned long value = 0UL;

    if (text[0] == 0)
        return -1L;
    for (index = 0; text[index] != 0; index++)
    {
        unsigned int digit;

        if (text[index] < '0' || text[index] > '9')
            return -1L;
        digit = (unsigned int)(text[index] - '0');
        if (value > 107374182UL ||
            (value == 107374182UL && digit > 3))
            return -1L;
        value = value * 10UL + digit;
    }
    return (long)value;
}

static int parse_command(char *line)
{
    unsigned int read = 0;
    unsigned int write = 0;
    unsigned int count = 0;

    while (line[read] != 0)
    {
        char quote = 0;

        while (line[read] == ' ')
            read++;
        if (line[read] == 0)
            break;
        if (count == SC16_MAX_ARGUMENTS)
            return -1;
        argument_offsets[count++] = write;
        while (line[read] != 0)
        {
            if (quote != 0)
            {
                if (line[read] == quote)
                {
                    quote = 0;
                    read++;
                    continue;
                }
            }
            else if (line[read] == '"' || line[read] == '\'')
            {
                quote = line[read++];
                continue;
            }
            else if (line[read] == ' ')
            {
                break;
            }
            line[write++] = line[read++];
        }
        if (quote != 0)
            return -1;
        if (line[read] == ' ')
            read++;
        line[write++] = 0;
    }
    return (int)count;
}

static int valid_relative_path(const char *path)
{
    unsigned int segment_length = 0;
    unsigned int index;
    unsigned int segment_start = 0;

    if (path[0] == 0)
        return 0;
    for (index = 0; path[index] != 0; index++)
    {
        unsigned char character = (unsigned char)path[index];

        if (character == '/')
        {
            if (segment_length == 0 ||
                (segment_length == 1 && path[segment_start] == '.') ||
                (segment_length == 2 && path[segment_start] == '.' &&
                 path[segment_start + 1] == '.'))
                return 0;
            segment_length = 0;
            segment_start = index + 1;
        }
        else if ((character >= 'a' && character <= 'z') ||
                 (character >= 'A' && character <= 'Z') ||
                 (character >= '0' && character <= '9') ||
                 character == '_' || character == '.' || character == '-')
        {
            segment_length++;
        }
        else
        {
            return 0;
        }
    }
    return segment_length != 0 &&
           !(segment_length == 1 && path[segment_start] == '.') &&
           !(segment_length == 2 && path[segment_start] == '.' &&
             path[segment_start + 1] == '.');
}

static int executable_path(const char *program, char *path, unsigned int capacity)
{
    const char *name = program;
    const char *suffix = ".bin";
    unsigned int prefix_length = 0;
    unsigned int length = 0;
    unsigned int index;
    int has_directory = 0;

    if (*name == '/')
        name++;
    for (index = 0; name[index] != 0; index++)
    {
        if (name[index] == '/')
            has_directory = 1;
    }
    if (!has_directory)
    {
        path[length++] = 'b';
        path[length++] = 'i';
        path[length++] = 'n';
        path[length++] = '/';
        prefix_length = length;
    }
    for (index = 0; name[index] != 0; index++)
    {
        if (length + 1 >= capacity)
            return 0;
        path[length++] = name[index];
    }
    path[length] = 0;
    if (!valid_relative_path(path + prefix_length))
        return 0;
    if (length < 4 || path[length - 4] != '.' ||
        path[length - 3] != 'b' || path[length - 2] != 'i' ||
        path[length - 1] != 'n')
    {
        for (index = 0; suffix[index] != 0; index++)
        {
            if (length + 1 >= capacity)
                return 0;
            path[length++] = suffix[index];
        }
    }
    path[length] = 0;
    return 1;
}

static void reap_background(void)
{
    unsigned int index;
    int status;
    char message[32];

    for (index = 0; index < MAX_BACKGROUND; index++)
    {
        if (background[index] == 0L)
            continue;
        status = sc16_trywait((unsigned long)background[index]);
        if (status == -11)
            continue;
        if (status < 0)
        {
            write_text("Unable to reap background process.\n");
            background[index] = 0L;
            continue;
        }
        write_text("Background process ");
        append_number(message, 0, (unsigned long)background[index]);
        write_text(message);
        write_text(" exited with status ");
        append_number(message, 0, (unsigned long)status);
        write_text(message);
        write_text("\n");
        background[index] = 0L;
    }
}

static int read_command(char *buffer, unsigned int capacity)
{
    unsigned int length = 0;
    int excess = 0;

    for (;;)
    {
        unsigned char character;
        int result;

        reap_background();
        if (pending_input_offset == pending_input_length)
        {
            result = sc16_read(0, pending_input, sizeof(pending_input));
            if (result < 0)
                return result;
            if (result == 0)
            {
                sc16_yield();
                continue;
            }
            pending_input_length = (unsigned int)result;
            pending_input_offset = 0;
        }
        character = pending_input[pending_input_offset++];
        if (character == '\r' || character == '\n')
        {
            buffer[length] = 0;
            write_text("\n");
            return excess == 0 ? (int)length : -2;
        }
        if (character == 8 || character == 127)
        {
            if (excess != 0)
                excess--;
            else if (length != 0)
            {
                length--;
                buffer[length] = 0;
            }
            continue;
        }
        if (character < 32 || character > 126)
            continue;
        if (length + 1 < capacity && excess == 0)
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

static int read_secret(char *buffer, unsigned int capacity)
{
    int result;
    int restore_result;

    if (sc16_console_echo(0) < 0)
        return -1;
    result = read_command(buffer, capacity);
    restore_result = sc16_console_echo(1);
    if (restore_result < 0)
        return -1;
    return result;
}

static int authenticate_session(char *password, unsigned int capacity)
{
    int status = sc16_account_status();
    int length;
    unsigned int index;

    if (status < 0)
        return -1;
    if (status == 0)
    {
        write_text("First boot: set the root password.\n");
        for (;;)
        {
            write_text("New root password: ");
            length = read_secret(password, capacity);
            if (length < 8 || length > 128)
            {
                for (index = 0; password[index] != 0; index++)
                    password[index] = 0;
                write_text("Password must contain 8 to 128 characters.\n");
                continue;
            }
            for (index = 0; index <= (unsigned int)length; index++)
                setup_password[index] = password[index];
            write_text("Confirm root password: ");
            length = read_secret(password, capacity);
            if (length <= 0 || length > 128)
                return -1;
            if (!strings_equal(setup_password, password))
            {
                for (index = 0; setup_password[index] != 0; index++)
                    setup_password[index] = 0;
                for (index = 0; password[index] != 0; index++)
                    password[index] = 0;
                write_text("Passwords do not match.\n");
                continue;
            }
            if (sc16_account_initialize_root(password) < 0)
            {
                write_text("Unable to initialize the root account.\n");
                return -1;
            }
            if (sc16_authenticate("root", password) < 0)
                return -1;
            copy_text(session_name, sizeof(session_name), "root");
            for (index = 0; setup_password[index] != 0; index++)
                setup_password[index] = 0;
            for (index = 0; password[index] != 0; index++)
                password[index] = 0;
            write_text("Root account ready.\n");
            return 0;
        }
    }

    for (;;)
    {
        write_text("login: ");
        length = read_command(login_name, sizeof(login_name));
        if (length <= 0 || length > 32)
        {
            write_text("Invalid user name.\n");
            continue;
        }
        write_text("Password: ");
        length = read_secret(password, capacity);
        if (length <= 0 || length > 128)
        {
            write_text("Invalid password.\n");
            continue;
        }
        if (sc16_authenticate(login_name, password) >= 0)
        {
            copy_text(session_name, sizeof(session_name), login_name);
            for (index = 0; password[index] != 0; index++)
                password[index] = 0;
            return 0;
        }
        for (index = 0; password[index] != 0; index++)
            password[index] = 0;
        write_text("Login failed.\n");
    }
    return -1;
}

static void show_processes(void)
{
    unsigned int index;
    char row[128];
    int length;

    write_text("PID PPID STATE PROGRAM\n");
    for (index = 0;; index++)
    {
        length = sc16_process_status(index, row, sizeof(row) - 1);
        if (length == 0)
            break;
        if (length < 0)
        {
            write_text("Unable to list processes.\n");
            return;
        }
        row[length] = 0;
        write_text(row);
    }
}

static void show_process_accounting(void)
{
    unsigned int index;
    char row[48];
    int length;

    write_text("PID INSTRUCTIONS_SINCE_START\n");
    for (index = 0;; index++)
    {
        length = sc16_process_accounting(index, row, sizeof(row) - 1);
        if (length == 0)
            return;
        if (length < 0)
        {
            write_text("Unable to read process accounting.\n");
            return;
        }
        row[length] = 0;
        write_text(row);
    }
}

static void terminate_process(char *line, unsigned int count)
{
    long process_id;
    int result;
    char message[32];

    if (count != 2)
    {
        write_text("Usage: kill PID\n");
        return;
    }
    process_id = parse_process_id(ARGUMENT(1));
    if (process_id <= 0L)
    {
        write_text("Invalid process ID.\n");
        return;
    }
    result = sc16_kill((unsigned long)process_id, 137);
    if (result < 0)
    {
        write_text("Unable to terminate process (");
        append_signed_number(message, 0, result);
        write_text(message);
        write_text(").\n");
        return;
    }
    write_text("Process terminated.\n");
}

static void list_directory(const char *directory)
{
    char entry[130];
    unsigned int index = 0;
    int result;
    char message[32];

    for (;;)
    {
        result = sc16_directory_entry(directory, index, entry, sizeof(entry));
        if (result == 0)
            return;
        if (result < 0)
        {
            write_text("Unable to list directory (");
            append_signed_number(message, 0, result);
            write_text(message);
            write_text(").\n");
            return;
        }
        if (!strings_equal(directory, "/"))
        {
            write_text(directory);
            write_text("/");
        }
        write_text(entry);
        write_text("\n");
        index++;
    }
}

static void run_program(char *line, unsigned int count)
{
    char path[129];
    char argument_block[ARGUMENT_CAPACITY];
    unsigned int argument_length = 0;
    unsigned int argument_count = count - 1;
    unsigned int index;
    int background_run = 0;
    int slot = -1;
    long child;
    int status;
    char message[32];
    char start_message[64];
    unsigned int start_offset = 0;

    if (count < 2)
    {
        write_text("Usage: run PROGRAM [args] [&]\n");
        return;
    }
    if (strings_equal(ARGUMENT(count - 1), "&"))
    {
        if (count < 3)
        {
            write_text("Usage: run PROGRAM [args] [&]\n");
            return;
        }
        background_run = 1;
        argument_count--;
        count--;
    }
    if (argument_count == 0 || argument_count > SC16_MAX_ARGUMENTS)
    {
        write_text("Too many program arguments.\n");
        return;
    }
    if (!executable_path(ARGUMENT(1), path, sizeof(path)))
    {
        write_text("Invalid program path.\n");
        return;
    }
    if (background_run)
    {
        for (slot = 0; slot < MAX_BACKGROUND && background[slot] != 0L; slot++)
        {
        }
        if (slot == MAX_BACKGROUND)
        {
            write_text("All background process slots are busy.\n");
            return;
        }
    }
    for (index = 1; index < count; index++)
    {
        unsigned int character;
        unsigned int source_offset = argument_offsets[index];

        for (character = 0; line[source_offset + character] != 0; character++)
        {
            if (argument_length + 1 >= sizeof(argument_block))
            {
                write_text("Program arguments exceed the 1024-byte limit.\n");
                return;
            }
            argument_block[argument_length++] = line[source_offset + character];
        }
        argument_block[argument_length++] = 0;
    }
    if (argument_length >= sizeof(argument_block))
    {
        write_text("Program arguments exceed the 1024-byte limit.\n");
        return;
    }
    child = sc16_spawn_exec(path, argument_block, argument_length, argument_count);
    if (child < 0L)
    {
        write_text("Unable to start program.\n");
        return;
    }
    if (background_run)
    {
        background[slot] = child;
        start_message[start_offset++] = 'S';
        start_message[start_offset++] = 't';
        start_message[start_offset++] = 'a';
        start_message[start_offset++] = 'r';
        start_message[start_offset++] = 't';
        start_message[start_offset++] = 'e';
        start_message[start_offset++] = 'd';
        start_message[start_offset++] = ' ';
        start_message[start_offset++] = 'b';
        start_message[start_offset++] = 'a';
        start_message[start_offset++] = 'c';
        start_message[start_offset++] = 'k';
        start_message[start_offset++] = 'g';
        start_message[start_offset++] = 'r';
        start_message[start_offset++] = 'o';
        start_message[start_offset++] = 'u';
        start_message[start_offset++] = 'n';
        start_message[start_offset++] = 'd';
        start_message[start_offset++] = ' ';
        start_message[start_offset++] = 'p';
        start_message[start_offset++] = 'r';
        start_message[start_offset++] = 'o';
        start_message[start_offset++] = 'c';
        start_message[start_offset++] = 'e';
        start_message[start_offset++] = 's';
        start_message[start_offset++] = 's';
        start_message[start_offset++] = ' ';
        start_offset = append_number(start_message, start_offset, (unsigned long)child);
        start_message[start_offset++] = '\n';
        start_message[start_offset] = 0;
        write_text(start_message);
        return;
    }
    status = sc16_wait((unsigned long)child);
    if (status < 0)
    {
        write_text("Waiting for program failed.\n");
        return;
    }
    write_text("Program exited with status ");
    append_number(message, 0, (unsigned long)status);
    write_text(message);
    write_text("\n");
}

static void add_group(char *line, unsigned int count)
{
    int result;
    char message[32];

    if (count != 2)
    {
        write_text("Usage: groupadd GROUP\n");
        return;
    }
    if (!copy_text(admin_group, sizeof(admin_group), ARGUMENT(1)))
    {
        write_text("Group name is too long.\n");
        return;
    }
    result = sc16_group_add(admin_group);
    if (result < 0)
    {
        write_text("Unable to create group (");
        append_signed_number(message, 0, result);
        write_text(message);
        write_text(").\n");
    }
    else
        write_text("Group created.\n");
}

static void make_directory(char *line, unsigned int count)
{
    int result;
    char message[32];

    if (count != 2)
    {
        write_text("Usage: mkdir DIRECTORY\n");
        return;
    }
    result = sc16_mkdir(ARGUMENT(1));
    if (result < 0)
    {
        write_text("Unable to create directory (");
        append_signed_number(message, 0, result);
        write_text(message);
        write_text(").\n");
    }
    else
    {
        write_text("Directory created.\n");
    }
}

static void touch_file(char *line, unsigned int count)
{
    int handle;
    int result;
    char message[32];

    if (count != 2)
    {
        write_text("Usage: touch FILE\n");
        return;
    }
    handle = sc16_open(
        ARGUMENT(1),
        SC16_OPEN_WRITE | SC16_OPEN_CREATE);
    if (handle < 0)
    {
        write_text("Unable to create file (");
        append_signed_number(message, 0, handle);
        write_text(message);
        write_text(").\n");
        return;
    }
    result = sc16_close(handle);
    if (result < 0)
    {
        write_text("Unable to close file.\n");
        return;
    }
    write_text("File ready.\n");
}

static void append_file(char *line, unsigned int count)
{
    unsigned int length;
    char *text;
    int handle;
    int result;
    char newline = '\n';
    char message[32];

    if (count != 3)
    {
        write_text("Usage: append FILE TEXT\n");
        return;
    }
    text = ARGUMENT(2);
    length = text_length(text);
    handle = sc16_open(
        ARGUMENT(1),
        SC16_OPEN_WRITE | SC16_OPEN_CREATE | SC16_OPEN_APPEND);
    if (handle < 0)
    {
        write_text("Unable to open file for appending (");
        append_signed_number(message, 0, handle);
        write_text(message);
        write_text(").\n");
        return;
    }
    result = sc16_write(handle, text, length);
    if (result != (int)length ||
        sc16_write(handle, &newline, 1) != 1)
    {
        write_text("Unable to append text.\n");
        if (sc16_close(handle) < 0)
            write_text("Unable to close file.\n");
        return;
    }
    if (sc16_close(handle) < 0)
    {
        write_text("Unable to close file.\n");
        return;
    }
    write_text("Text appended.\n");
}

static void show_file(char *line, unsigned int count)
{
    unsigned char buffer[128];
    int handle;
    int result;
    char message[32];

    if (count != 2)
    {
        write_text("Usage: cat FILE\n");
        return;
    }
    handle = sc16_open(ARGUMENT(1), SC16_OPEN_READ);
    if (handle < 0)
    {
        write_text("Unable to open file (");
        append_signed_number(message, 0, handle);
        write_text(message);
        write_text(").\n");
        return;
    }
    for (;;)
    {
        result = sc16_read(handle, buffer, sizeof(buffer));
        if (result <= 0)
            break;
        if (sc16_write(1, buffer, (unsigned int)result) != result)
        {
            write_text("Unable to write file contents to the console.\n");
            if (sc16_close(handle) < 0)
                write_text("Unable to close file.\n");
            return;
        }
    }
    if (result < 0)
    {
        write_text("Unable to read file.\n");
    }
    if (sc16_close(handle) < 0)
        write_text("Unable to close file.\n");
}

static void delete_file(char *line, unsigned int count)
{
    int result;
    char message[32];

    if (count != 2)
    {
        write_text("Usage: rm FILE\n");
        return;
    }
    result = sc16_delete_file(ARGUMENT(1));
    if (result < 0)
    {
        write_text("Unable to delete file (");
        append_signed_number(message, 0, result);
        write_text(message);
        write_text(").\n");
    }
    else
    {
        write_text("File deleted.\n");
    }
}

static void rename_file(char *line, unsigned int count)
{
    int result;
    char message[32];

    if (count != 3)
    {
        write_text("Usage: mv SOURCE DESTINATION\n");
        return;
    }
    result = sc16_rename_file(ARGUMENT(1), ARGUMENT(2));
    if (result < 0)
    {
        write_text("Unable to rename file (");
        append_signed_number(message, 0, result);
        write_text(message);
        write_text(").\n");
        return;
    }
    write_text("File renamed.\n");
}

static void change_file_mode(char *line, unsigned int count)
{
    int mode;
    int result;
    char message[32];

    if (count != 3)
    {
        write_text("Usage: chmod MODE FILE\n");
        return;
    }
    mode = parse_octal_mode(ARGUMENT(1));
    if (mode < 0)
    {
        write_text("Mode must be one to three octal digits.\n");
        return;
    }
    result = sc16_set_file_mode(ARGUMENT(2), (unsigned int)mode);
    if (result < 0)
    {
        write_text("Unable to change mode (");
        append_signed_number(message, 0, result);
        write_text(message);
        write_text(").\n");
    }
    else
    {
        write_text("Mode changed.\n");
    }
}

static void change_file_owner(char *line, unsigned int count)
{
    int result;
    char message[32];

    if (count != 4)
    {
        write_text("Usage: chown FILE USER GROUP\n");
        return;
    }
    result = sc16_set_file_owner(ARGUMENT(1), ARGUMENT(2), ARGUMENT(3));
    if (result < 0)
    {
        write_text("Unable to change owner (");
        append_signed_number(message, 0, result);
        write_text(message);
        write_text(").\n");
    }
    else
    {
        write_text("Owner changed.\n");
    }
}

static void edit_file(char *line, unsigned int count)
{
    char path[129];
    char content[LINE_CAPACITY];
    char newline[1];
    int handle;
    int length;
    int result;

    if (count != 2 || !copy_text(path, sizeof(path), ARGUMENT(1)))
    {
        write_text("Usage: edit FILE\n");
        return;
    }
    handle = sc16_open(
        path,
        SC16_OPEN_WRITE | SC16_OPEN_CREATE | SC16_OPEN_TRUNCATE);
    if (handle < 0)
    {
        write_text("Unable to open file for editing.\n");
        return;
    }
    newline[0] = '\n';
    write_text("This replaces the file. Enter one line at a time; '.' saves.\n");
    for (;;)
    {
        write_text("edit> ");
        length = read_command(content, sizeof(content));
        if (length == -2)
        {
            write_text("Line too long (255 characters maximum).\n");
            continue;
        }
        if (length < 0)
        {
            write_text("Editor input failed.\n");
            if (sc16_close(handle) < 0)
                write_text("Unable to close edited file.\n");
            return;
        }
        if (length == 1 && content[0] == '.')
            break;
        if (length != 0 &&
            sc16_write(handle, content, (unsigned int)length) != length)
        {
            write_text("Unable to save edited text.\n");
            if (sc16_close(handle) < 0)
                write_text("Unable to close edited file.\n");
            return;
        }
        if (sc16_write(handle, newline, sizeof(newline)) != sizeof(newline))
        {
            write_text("Unable to save edited line.\n");
            if (sc16_close(handle) < 0)
                write_text("Unable to close edited file.\n");
            return;
        }
    }
    if (sc16_close(handle) < 0)
    {
        write_text("Unable to close edited file.\n");
        return;
    }
    write_text("File saved.\n");
}

static void add_user(char *line, unsigned int count)
{
    char home[129];
    char message[32];
    int result;

    if (count != 3)
    {
        write_text("Usage: useradd USER GROUP\n");
        return;
    }
    if (!copy_text(admin_name, sizeof(admin_name), ARGUMENT(1)) ||
        !copy_text(admin_group, sizeof(admin_group), ARGUMENT(2)))
    {
        write_text("User or group name is too long.\n");
        return;
    }
    if (!copy_text(home, sizeof(home), "home/") ||
        !copy_text(home + 5, sizeof(home) - 5, admin_name))
    {
        write_text("User name is too long.\n");
        return;
    }
    result = sc16_user_add(admin_name, admin_group, home);
    if (result < 0)
    {
        write_text("Unable to create user or home directory (");
        append_signed_number(message, 0, result);
        write_text(message);
        write_text(").\n");
    }
    else
        write_text("User created. Set its password with passwd.\n");
}

static void change_password(char *line, unsigned int count)
{
    char password[129];
    char confirmation[129];
    char *name = session_name;
    char message[32];
    int length;
    int result;
    unsigned int index;

    if (count > 2)
    {
        write_text("Usage: passwd [USER]\n");
        return;
    }
    if (count == 2)
    {
        if (!copy_text(admin_name, sizeof(admin_name), ARGUMENT(1)))
        {
            write_text("User name is too long.\n");
            return;
        }
        name = admin_name;
    }
    write_text("New password: ");
    length = read_secret(password, sizeof(password));
    if (length < 8 || length > 128)
    {
        for (index = 0; password[index] != 0; index++)
            password[index] = 0;
        write_text("Password must contain 8 to 128 characters.\n");
        return;
    }
    write_text("Confirm password: ");
    length = read_secret(confirmation, sizeof(confirmation));
    if (length < 0 || !strings_equal(password, confirmation))
    {
        for (index = 0; password[index] != 0; index++)
            password[index] = 0;
        for (index = 0; confirmation[index] != 0; index++)
            confirmation[index] = 0;
        write_text("Passwords do not match.\n");
        return;
    }
    result = sc16_set_password(name, password);
    if (result < 0)
    {
        write_text("Unable to change password (");
        append_signed_number(message, 0, result);
        write_text(message);
        write_text(").\n");
    }
    else
    {
        write_text("Password changed.\n");
    }
    for (index = 0; password[index] != 0; index++)
        password[index] = 0;
    for (index = 0; confirmation[index] != 0; index++)
        confirmation[index] = 0;
}

static void execute_command(char *line, unsigned int count)
{
    if (count == 0)
        return;
    if (strings_equal(ARGUMENT(0), "help"))
    {
        write_text("Commands: help, info, ps, top, kill PID, list [DIR], mkdir DIRECTORY, touch FILE, append FILE TEXT, cat FILE, edit FILE, rm FILE, mv SOURCE DESTINATION, chmod MODE FILE, chown FILE USER GROUP, run PROGRAM [args] [&], groupadd GROUP, useradd USER GROUP, passwd [USER], logout\n");
    }
    else if (strings_equal(ARGUMENT(0), "info"))
    {
        write_text("SisuOS SaunaShell | Protected mode | Syscall ABI 1\n");
    }
    else if (strings_equal(ARGUMENT(0), "ps"))
    {
        show_processes();
    }
    else if (strings_equal(ARGUMENT(0), "top"))
    {
        show_process_accounting();
    }
    else if (strings_equal(ARGUMENT(0), "kill"))
    {
        terminate_process(line, count);
    }
    else if (strings_equal(ARGUMENT(0), "list") ||
             strings_equal(ARGUMENT(0), "ls"))
    {
        if (count > 2)
            write_text("Usage: list [DIR]\n");
        else
            list_directory(count == 1 ? "/" : ARGUMENT(1));
    }
    else if (strings_equal(ARGUMENT(0), "run"))
    {
        run_program(line, count);
    }
    else if (strings_equal(ARGUMENT(0), "mkdir"))
    {
        make_directory(line, count);
    }
    else if (strings_equal(ARGUMENT(0), "touch"))
    {
        touch_file(line, count);
    }
    else if (strings_equal(ARGUMENT(0), "append"))
    {
        append_file(line, count);
    }
    else if (strings_equal(ARGUMENT(0), "cat"))
    {
        show_file(line, count);
    }
    else if (strings_equal(ARGUMENT(0), "edit"))
    {
        edit_file(line, count);
    }
    else if (strings_equal(ARGUMENT(0), "rm"))
    {
        delete_file(line, count);
    }
    else if (strings_equal(ARGUMENT(0), "mv"))
    {
        rename_file(line, count);
    }
    else if (strings_equal(ARGUMENT(0), "chmod"))
    {
        change_file_mode(line, count);
    }
    else if (strings_equal(ARGUMENT(0), "chown"))
    {
        change_file_owner(line, count);
    }
    else if (strings_equal(ARGUMENT(0), "groupadd"))
    {
        add_group(line, count);
    }
    else if (strings_equal(ARGUMENT(0), "useradd"))
    {
        add_user(line, count);
    }
    else if (strings_equal(ARGUMENT(0), "passwd"))
    {
        change_password(line, count);
    }
    else if (strings_equal(ARGUMENT(0), "logout"))
    {
        sc16_exit(0);
    }
    else
    {
        write_text("Unknown command. Type help.\n");
    }
}

int main(void)
{
    long command_memory;
    char *command;
    int length;
    int count;

    command_memory = sc16_mmap(4096UL, SC16_PROT_READ | SC16_PROT_WRITE, (void *)0);
    if (command_memory < 0L)
    {
        write_text("SaunaShell: cannot allocate its command buffer.\n");
        sc16_exit(1);
        return 1;
    }
    command = (char *)command_memory;
    if (authenticate_session(command, LINE_CAPACITY) < 0)
    {
        write_text("SisuOS: account service unavailable.\n");
        sc16_exit(1);
        return 1;
    }
    write_text("SaunaShell - SisuOS\n");
    write_text("Type 'help' to list commands.\n");
    for (;;)
    {
        write_text("sauna> ");
        length = read_command(command, LINE_CAPACITY);
        if (length == -2)
        {
            write_text("Command too long (255 characters maximum).\n");
            continue;
        }
        if (length < 0)
        {
            write_text("Console input failed.\n");
            sc16_yield();
            continue;
        }
        count = parse_command(command);
        if (count < 0)
        {
            write_text("Invalid command syntax or too many arguments.\n");
            continue;
        }
        execute_command(command, (unsigned int)count);
    }
}
