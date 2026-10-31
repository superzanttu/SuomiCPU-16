#include "preemptive_os_console.h"

#define LINE_CAPACITY 256
#define ARGUMENT_CAPACITY 1024
#define EDITOR_CAPACITY 4096
#define EDITOR_ROWS 22
#define EDITOR_COLUMNS 80
#define MAX_BACKGROUND 16
#define ARGUMENT(index) (line + argument_offsets[(index)])

static long background[MAX_BACKGROUND];
static unsigned int argument_offsets[SC16_MAX_ARGUMENTS];
static char editor_content[EDITOR_CAPACITY];
static unsigned char pending_input[LINE_CAPACITY];
static unsigned int pending_input_length;
static unsigned int pending_input_offset;
static int command_failed;
static int completion_enabled;
static unsigned int script_depth;
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

static void erase_input_line(unsigned int prompt_length, unsigned int input_length)
{
    unsigned int occupied = prompt_length + input_length;
    unsigned int rows = occupied == 0 ? 0 : (occupied - 1) / 80;

    while (rows != 0)
    {
        write_text("\r\033[2K\033[1A");
        rows--;
    }
    write_text("\r\033[2K");
}

static void complete_filename(char *buffer, unsigned int *length, unsigned int capacity)
{
    char directory[129];
    char entry[130];
    char selected[130];
    unsigned int token_start = *length;
    unsigned int prefix_start;
    unsigned int prefix_length;
    unsigned int directory_length;
    unsigned int index;
    unsigned int count = 0;
    int result;
    int trailing_slash = 0;

    while (token_start != 0 && buffer[token_start - 1] != ' ')
        token_start--;
    if (token_start == *length)
        return;
    prefix_start = *length;
    while (prefix_start > token_start && buffer[prefix_start - 1] != '/')
        prefix_start--;
    if (prefix_start > token_start)
    {
        directory_length = prefix_start - token_start - 1;
        if (directory_length == 0)
        {
            directory[0] = '/';
            directory[1] = 0;
        }
        else
        {
            if (directory_length >= sizeof(directory))
                return;
            for (index = 0; index < directory_length; index++)
                directory[index] = buffer[token_start + index];
            directory[directory_length] = 0;
        }
    }
    else
    {
        directory[0] = '/';
        directory[1] = 0;
    }
    prefix_length = *length - prefix_start;
    for (index = 0; ; index++)
    {
        unsigned int character;
        int matches = 1;

        result = sc16_directory_entry(directory, index, entry, sizeof(entry));
        if (result == 0)
            break;
        if (result < 0)
        {
            write_text("Filename completion failed.\n");
            write_text("sauna> ");
            write_text(buffer);
            return;
        }
        for (character = 0; character < prefix_length; character++)
        {
            if (entry[character] == 0 ||
                entry[character] != buffer[prefix_start + character])
            {
                matches = 0;
                break;
            }
        }
        if (matches)
        {
            count++;
            if (count == 1)
                copy_text(selected, sizeof(selected), entry);
        }
    }
    if (count == 1)
    {
        unsigned int selected_length = text_length(selected);
        unsigned int extra_length = selected_length - prefix_length;

        trailing_slash = selected_length != 0 && selected[selected_length - 1] == '/';
        if (*length + extra_length + (trailing_slash ? 0 : 1) >= capacity)
            return;
        for (index = prefix_length; index < selected_length; index++)
            buffer[(*length)++] = selected[index];
        if (!trailing_slash)
            buffer[(*length)++] = ' ';
        buffer[*length] = 0;
        if (extra_length != 0)
            write_text(selected + prefix_length);
        if (!trailing_slash)
            write_text(" ");
        return;
    }
    if (count > 1)
    {
        erase_input_line(7, *length);
        for (index = 0; ; index++)
        {
            unsigned int character;
            int matches = 1;

            result = sc16_directory_entry(directory, index, entry, sizeof(entry));
            if (result <= 0)
                break;
            for (character = 0; character < prefix_length; character++)
            {
                if (entry[character] == 0 ||
                    entry[character] != buffer[prefix_start + character])
                {
                    matches = 0;
                    break;
                }
            }
            if (matches)
            {
                write_text(entry);
                write_text("\n");
            }
        }
        write_text("sauna> ");
        write_text(buffer);
    }
}

static int finish_command_read(int result)
{
    if (sc16_console_echo(1) < 0)
        return -1;
    return result;
}

static int read_command(char *buffer, unsigned int capacity, int echo_input)
{
    unsigned int length = 0;
    int excess = 0;

    if (sc16_console_echo(0) < 0)
        return -1;
    for (;;)
    {
        unsigned char character;
        int result;

        reap_background();
        if (pending_input_offset == pending_input_length)
        {
            result = sc16_read(0, pending_input, sizeof(pending_input));
            if (result < 0)
                return finish_command_read(result);
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
            return finish_command_read(excess == 0 ? (int)length : -2);
        }
        if (character == 3)
        {
            buffer[0] = 0;
            write_text("^C\n");
            return finish_command_read(-3);
        }
        if (character == '\t' && completion_enabled)
        {
            complete_filename(buffer, &length, capacity);
            continue;
        }
        if (character == 8 || character == 127)
        {
            if (excess != 0)
                excess--;
            else if (length != 0)
            {
                length--;
                buffer[length] = 0;
                if (echo_input)
                    write_text("\b");
            }
            continue;
        }
        if (character < 32 || character > 126)
            continue;
        if (length + 1 < capacity && excess == 0)
        {
            buffer[length++] = (char)character;
            buffer[length] = 0;
            if (echo_input)
            {
                char echo[2];
                echo[0] = (char)character;
                echo[1] = 0;
                write_text(echo);
            }
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
    return read_command(buffer, capacity, 0);
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
        length = read_command(login_name, sizeof(login_name), 1);
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
            command_failed = 1;
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
            command_failed = 1;
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
        command_failed = 1;
        write_text("Usage: kill PID\n");
        return;
    }
    process_id = parse_process_id(ARGUMENT(1));
    if (process_id <= 0L)
    {
        command_failed = 1;
        write_text("Invalid process ID.\n");
        return;
    }
    result = sc16_kill((unsigned long)process_id, 137);
    if (result < 0)
    {
        command_failed = 1;
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
            command_failed = 1;
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

static int execute_command(char *line, unsigned int count);
static int run_script(const char *path);

static int has_cmd_extension(const char *path)
{
    unsigned int length = text_length(path);

    return length >= 4 &&
           path[length - 4] == '.' &&
           path[length - 3] == 'c' &&
           path[length - 2] == 'm' &&
           path[length - 1] == 'd';
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
        command_failed = 1;
        write_text("Usage: run PROGRAM [args] [&]\n");
        return;
    }
    if (strings_equal(ARGUMENT(count - 1), "&"))
    {
        if (count < 3)
        {
            command_failed = 1;
            write_text("Usage: run PROGRAM [args] [&]\n");
            return;
        }
        background_run = 1;
        argument_count--;
        count--;
    }
    if (argument_count == 0 || argument_count > SC16_MAX_ARGUMENTS)
    {
        command_failed = 1;
        write_text("Too many program arguments.\n");
        return;
    }
    if (has_cmd_extension(ARGUMENT(1)))
    {
        const char *script_path = ARGUMENT(1);

        if (background_run || count != 2)
        {
            write_text("Usage: run SCRIPT.cmd\n");
            command_failed = 1;
            return;
        }
        if (*script_path == '/')
            script_path++;
        if (!copy_text(path, sizeof(path), script_path) ||
            !valid_relative_path(path))
        {
            write_text("Invalid script path.\n");
            command_failed = 1;
            return;
        }
        run_script(path);
        return;
    }
    if (!executable_path(ARGUMENT(1), path, sizeof(path)))
    {
        command_failed = 1;
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
            command_failed = 1;
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
                command_failed = 1;
                write_text("Program arguments exceed the 1024-byte limit.\n");
                return;
            }
            argument_block[argument_length++] = line[source_offset + character];
        }
        argument_block[argument_length++] = 0;
    }
    if (argument_length >= sizeof(argument_block))
    {
        command_failed = 1;
        write_text("Program arguments exceed the 1024-byte limit.\n");
        return;
    }
    child = sc16_spawn_exec(path, argument_block, argument_length, argument_count);
    if (child < 0L)
    {
        command_failed = 1;
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
        command_failed = 1;
        write_text("Waiting for program failed.\n");
        return;
    }
    if (status != 0)
        command_failed = 1;
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
        command_failed = 1;
        write_text("Usage: groupadd GROUP\n");
        return;
    }
    if (!copy_text(admin_group, sizeof(admin_group), ARGUMENT(1)))
    {
        command_failed = 1;
        write_text("Group name is too long.\n");
        return;
    }
    result = sc16_group_add(admin_group);
    if (result < 0)
    {
        command_failed = 1;
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
        command_failed = 1;
        write_text("Usage: mkdir DIRECTORY\n");
        return;
    }
    result = sc16_mkdir(ARGUMENT(1));
    if (result < 0)
    {
        command_failed = 1;
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
        command_failed = 1;
        write_text("Usage: touch FILE\n");
        return;
    }
    handle = sc16_open(
        ARGUMENT(1),
        SC16_OPEN_WRITE | SC16_OPEN_CREATE);
    if (handle < 0)
    {
        command_failed = 1;
        write_text("Unable to create file (");
        append_signed_number(message, 0, handle);
        write_text(message);
        write_text(").\n");
        return;
    }
    result = sc16_close(handle);
    if (result < 0)
    {
        command_failed = 1;
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
        command_failed = 1;
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
        command_failed = 1;
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
        command_failed = 1;
        write_text("Unable to append text.\n");
        if (sc16_close(handle) < 0)
            write_text("Unable to close file.\n");
        return;
    }
    if (sc16_close(handle) < 0)
    {
        command_failed = 1;
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
        command_failed = 1;
        write_text("Usage: cat FILE\n");
        return;
    }
    handle = sc16_open(ARGUMENT(1), SC16_OPEN_READ);
    if (handle < 0)
    {
        command_failed = 1;
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
            command_failed = 1;
            write_text("Unable to write file contents to the console.\n");
            if (sc16_close(handle) < 0)
                write_text("Unable to close file.\n");
            return;
        }
    }
    if (result < 0)
    {
        command_failed = 1;
        write_text("Unable to read file.\n");
    }
    if (sc16_close(handle) < 0)
    {
        command_failed = 1;
        write_text("Unable to close file.\n");
    }
}

static void delete_file(char *line, unsigned int count)
{
    int result;
    char message[32];

    if (count != 2)
    {
        command_failed = 1;
        write_text("Usage: rm FILE\n");
        return;
    }
    result = sc16_delete_file(ARGUMENT(1));
    if (result < 0)
    {
        command_failed = 1;
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
        command_failed = 1;
        write_text("Usage: mv SOURCE DESTINATION\n");
        return;
    }
    result = sc16_rename_file(ARGUMENT(1), ARGUMENT(2));
    if (result < 0)
    {
        command_failed = 1;
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
        command_failed = 1;
        write_text("Usage: chmod MODE FILE\n");
        return;
    }
    mode = parse_octal_mode(ARGUMENT(1));
    if (mode < 0)
    {
        command_failed = 1;
        write_text("Mode must be one to three octal digits.\n");
        return;
    }
    result = sc16_set_file_mode(ARGUMENT(2), (unsigned int)mode);
    if (result < 0)
    {
        command_failed = 1;
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
        command_failed = 1;
        write_text("Usage: chown FILE USER GROUP\n");
        return;
    }
    result = sc16_set_file_owner(ARGUMENT(1), ARGUMENT(2), ARGUMENT(3));
    if (result < 0)
    {
        command_failed = 1;
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

static unsigned int nano_line_start(const char *text, unsigned int position)
{
    while (position != 0 && text[position - 1] != '\n')
        position--;
    return position;
}

static unsigned int nano_line_end(
    const char *text,
    unsigned int length,
    unsigned int position)
{
    while (position < length && text[position] != '\n')
        position++;
    return position;
}

static unsigned int nano_visual_column(
    const char *text,
    unsigned int start,
    unsigned int position)
{
    unsigned int column = 0;
    unsigned int index;

    for (index = start; index < position; index++)
        column += text[index] == '\t' ? 4 : 1;
    return column;
}

static unsigned int nano_column_offset(
    const char *text,
    unsigned int start,
    unsigned int end,
    unsigned int column)
{
    unsigned int current = 0;

    while (start < end && current < column)
    {
        unsigned int width = text[start] == '\t' ? 4 : 1;

        if (current + width > column)
            break;
        current += width;
        start++;
    }
    return start;
}

static void nano_move_vertical(
    const char *text,
    unsigned int length,
    unsigned int *cursor,
    int direction)
{
    unsigned int start = nano_line_start(text, *cursor);
    unsigned int column = nano_visual_column(text, start, *cursor);
    unsigned int end;

    if (direction < 0)
    {
        if (start == 0)
            return;
        end = start - 1;
        start = nano_line_start(text, end);
        *cursor = nano_column_offset(text, start, end, column);
        return;
    }
    end = nano_line_end(text, length, *cursor);
    if (end == length)
        return;
    start = end + 1;
    end = nano_line_end(text, length, start);
    *cursor = nano_column_offset(text, start, end, column);
}

static void nano_write_cursor(unsigned int row, unsigned int column)
{
    char position[16];
    unsigned int offset = 0;

    position[offset++] = '\033';
    position[offset++] = '[';
    offset = append_number(position, offset, row);
    position[offset++] = ';';
    offset = append_number(position, offset, column);
    position[offset++] = 'H';
    position[offset] = 0;
    write_text(position);
}

static void nano_render(
    const char *path,
    const char *text,
    unsigned int length,
    unsigned int cursor,
    int changed,
    const char *status)
{
    unsigned int cursor_line = 0;
    unsigned int cursor_column = 0;
    unsigned int top_line;
    unsigned int offset = 0;
    unsigned int line = 0;
    unsigned int row;
    unsigned int index;

    for (index = 0; index < cursor; index++)
    {
        if (text[index] == '\n')
        {
            cursor_line++;
            cursor_column = 0;
        }
        else
        {
            cursor_column += text[index] == '\t' ? 4 : 1;
        }
    }
    top_line = cursor_line >= EDITOR_ROWS
                   ? cursor_line - EDITOR_ROWS + 1
                   : 0;
    while (line < top_line && offset < length)
    {
        if (text[offset++] == '\n')
            line++;
    }

    write_text("\033[2J\033[H");
    write_text("SC-16 nano: ");
    write_text(path);
    if (changed)
        write_text(" *");
    write_text("\r\n");
    for (row = 0; row < EDITOR_ROWS; row++)
    {
        unsigned int end = nano_line_end(text, length, offset);
        unsigned int column = 0;
        unsigned int left_column =
            line == cursor_line && cursor_column >= EDITOR_COLUMNS
                ? cursor_column - EDITOR_COLUMNS + 1
                : 0;

        while (offset < end)
        {
            unsigned int width = text[offset] == '\t' ? 4 : 1;
            unsigned int cell;

            for (cell = 0; cell < width; cell++)
            {
                if (column >= left_column &&
                    column < left_column + EDITOR_COLUMNS)
                {
                    if (text[offset] == '\t')
                        write_text(" ");
                    else
                    {
                        char character[2];
                        character[0] = text[offset];
                        character[1] = 0;
                        write_text(character);
                    }
                }
                column++;
            }
            offset++;
        }
        if (end < length)
        {
            offset = end + 1;
            line++;
        }
        else
        {
            offset = end;
        }
        if (row + 1 < EDITOR_ROWS)
            write_text("\r\n");
    }
    write_text("\033[24;1H\033[2K^O Save  ^X Exit");
    if (changed)
        write_text("  * Modified");
    if (status != 0 && status[0] != 0)
    {
        write_text("  ");
        write_text(status);
    }
    nano_write_cursor(
        cursor_line - top_line + 2,
        cursor_column < EDITOR_COLUMNS
            ? cursor_column + 1
            : EDITOR_COLUMNS);
}

static int nano_read_key(void)
{
    unsigned char character;
    int result;

    for (;;)
    {
        result = sc16_read(0, &character, 1);
        if (result < 0)
            return -1;
        if (result == 0)
        {
            sc16_yield();
            continue;
        }
        return character;
    }
    return -1;
}

static int nano_save(const char *path, const char *text, unsigned int length)
{
    int handle = sc16_open(
        path,
        SC16_OPEN_WRITE | SC16_OPEN_CREATE | SC16_OPEN_TRUNCATE);
    int result;
    int close_result;

    if (handle < 0)
        return handle;
    result = length == 0 ? 0 : sc16_write(handle, text, length);
    close_result = sc16_close(handle);
    if (close_result < 0)
        return close_result;
    if (result != (int)length)
        return result < 0 ? result : -5;
    return 0;
}

static void nano_close_after_read_error(int handle)
{
    if (sc16_close(handle) < 0)
        write_text("Unable to close file after a read error.\n");
}

static int nano_file(char *line, unsigned int count)
{
    char path[129];
    char status[48];
    unsigned int length = 0;
    unsigned int cursor = 0;
    unsigned int index;
    int changed = 0;
    int echo_disabled = 0;
    int handle;
    int result;
    int key;
    unsigned char extra;
    unsigned int escape_state = 0;

    if (count != 2 || !copy_text(path, sizeof(path), ARGUMENT(1)))
    {
        write_text("Usage: nano FILE\n");
        command_failed = 1;
        return 0;
    }
    handle = sc16_open(path, SC16_OPEN_READ);
    if (handle >= 0)
    {
        while (length < EDITOR_CAPACITY - 1)
        {
            result = sc16_read(
                handle,
                editor_content + length,
                EDITOR_CAPACITY - 1 - length);
            if (result < 0)
            {
                write_text("Unable to read file for nano.\n");
                nano_close_after_read_error(handle);
                command_failed = 1;
                return 0;
            }
            if (result == 0)
                break;
            for (index = length; index < length + (unsigned int)result; index++)
            {
                if (editor_content[index] == 0 ||
                    (editor_content[index] < 32 &&
                     editor_content[index] != '\r' &&
                     editor_content[index] != '\n' &&
                     editor_content[index] != '\t'))
                {
                    write_text("Nano supports text files only.\n");
                    nano_close_after_read_error(handle);
                    command_failed = 1;
                    return 0;
                }
            }
            length += (unsigned int)result;
        }
        if (length == EDITOR_CAPACITY - 1)
        {
            result = sc16_read(handle, &extra, 1);
            if (result < 0)
            {
                write_text("Unable to read file for nano.\n");
                nano_close_after_read_error(handle);
                command_failed = 1;
                return 0;
            }
            if (result > 0)
            {
                write_text("File exceeds nano's 4095-byte buffer limit.\n");
                nano_close_after_read_error(handle);
                command_failed = 1;
                return 0;
            }
        }
        if (sc16_close(handle) < 0)
        {
            write_text("Unable to close file after reading it.\n");
            command_failed = 1;
            return 0;
        }
    }
    else if (handle != -2)
    {
        write_text("Unable to open file for nano.\n");
        command_failed = 1;
        return 0;
    }
    if (length != 0)
    {
        unsigned int raw_length = length;
        length = 0;

        for (index = 0; index < raw_length; index++)
        {
            if (editor_content[index] == '\r')
            {
                if (index + 1 < raw_length && editor_content[index + 1] == '\n')
                    continue;
                editor_content[length++] = '\n';
            }
            else
            {
                editor_content[length++] = editor_content[index];
            }
        }
    }
    editor_content[length] = 0;
    if (sc16_console_echo(0) < 0)
    {
        write_text("Unable to disable console echo for nano.\n");
        command_failed = 1;
        return 0;
    }
    echo_disabled = 1;
    status[0] = 0;
    for (;;)
    {
        nano_render(path, editor_content, length, cursor, changed, status);
        key = nano_read_key();
        if (key < 0)
        {
            write_text("\033[2J\033[HUnable to read nano input.\n");
            command_failed = 1;
            break;
        }
        if (escape_state == 1)
        {
            escape_state = key == '[' ? 2 : 0;
            continue;
        }
        if (escape_state == 2)
        {
            unsigned int start;
            unsigned int end;

            if (key == 'A')
                nano_move_vertical(editor_content, length, &cursor, -1);
            else if (key == 'B')
                nano_move_vertical(editor_content, length, &cursor, 1);
            else if (key == 'C' && cursor < length)
                cursor++;
            else if (key == 'D' && cursor != 0)
                cursor--;
            else if (key == 'H')
                cursor = nano_line_start(editor_content, cursor);
            else if (key == 'F')
                cursor = nano_line_end(editor_content, length, cursor);
            else if (key == '3')
            {
                escape_state = 3;
                continue;
            }
            else
            {
                escape_state = 0;
                continue;
            }
            start = nano_line_start(editor_content, cursor);
            end = nano_line_end(editor_content, length, start);
            if (key == 'D' && cursor < start)
                cursor = start;
            if (key == 'C' && cursor > end)
                cursor = end;
            escape_state = 0;
            status[0] = 0;
            continue;
        }
        if (escape_state == 3)
        {
            if (key == '~' && cursor < length)
            {
                for (index = cursor; index < length; index++)
                    editor_content[index] = editor_content[index + 1];
                length--;
                changed = 1;
            }
            escape_state = 0;
            status[0] = 0;
            continue;
        }
        if (key == 27)
        {
            escape_state = 1;
            continue;
        }
        if (key == 15)
        {
            result = nano_save(path, editor_content, length);
            if (result < 0)
            {
                write_text("\033[24;1H\033[2KUnable to save file.\n");
                command_failed = 1;
                copy_text(status, sizeof(status), "Save failed");
            }
            else
            {
                changed = 0;
                copy_text(status, sizeof(status), "Saved");
            }
            continue;
        }
        if (key == 24 || key == 3)
        {
            if (!changed)
                break;
            write_text("\033[24;1H\033[2KDiscard changes? (y/n) ");
            key = nano_read_key();
            if (key == 'y' || key == 'Y')
                break;
            status[0] = 0;
            continue;
        }
        if (key == 8 || key == 127)
        {
            if (cursor != 0)
            {
                for (index = cursor - 1; index < length; index++)
                    editor_content[index] = editor_content[index + 1];
                cursor--;
                length--;
                changed = 1;
            }
            status[0] = 0;
            continue;
        }
        if (key == '\r' || key == '\n' || key == '\t' ||
            (key >= 32 && key <= 126))
        {
            unsigned int insert_count = key == '\t' ? 4 : 1;
            unsigned int insert_index;

            if (length + insert_count >= EDITOR_CAPACITY)
            {
                copy_text(status, sizeof(status), "Buffer full");
                continue;
            }
            for (insert_index = 0; insert_index < insert_count; insert_index++)
            {
                unsigned char inserted =
                    key == '\t' ? ' ' :
                    (key == '\r' ? '\n' : (unsigned char)key);
                for (index = length; index > cursor; index--)
                    editor_content[index] = editor_content[index - 1];
                editor_content[cursor++] = (char)inserted;
                length++;
            }
            editor_content[length] = 0;
            changed = 1;
            status[0] = 0;
        }
    }
    if (echo_disabled && sc16_console_echo(1) < 0)
    {
        write_text("Unable to restore console echo after nano.\n");
        command_failed = 1;
    }
    write_text("\033[2J\033[H");
    return 1;
}

static void add_user(char *line, unsigned int count)
{
    char home[129];
    char message[32];
    int result;

    if (count != 3)
    {
        command_failed = 1;
        write_text("Usage: useradd USER GROUP\n");
        return;
    }
    if (!copy_text(admin_name, sizeof(admin_name), ARGUMENT(1)) ||
        !copy_text(admin_group, sizeof(admin_group), ARGUMENT(2)))
    {
        command_failed = 1;
        write_text("User or group name is too long.\n");
        return;
    }
    if (!copy_text(home, sizeof(home), "home/") ||
        !copy_text(home + 5, sizeof(home) - 5, admin_name))
    {
        command_failed = 1;
        write_text("User name is too long.\n");
        return;
    }
    result = sc16_user_add(admin_name, admin_group, home);
    if (result < 0)
    {
        command_failed = 1;
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
        command_failed = 1;
        write_text("Usage: passwd [USER]\n");
        return;
    }
    if (count == 2)
    {
        if (!copy_text(admin_name, sizeof(admin_name), ARGUMENT(1)))
        {
            command_failed = 1;
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
        command_failed = 1;
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
        command_failed = 1;
        return;
    }
    result = sc16_set_password(name, password);
    if (result < 0)
    {
        command_failed = 1;
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

static int execute_command(char *line, unsigned int count)
{
    command_failed = 0;
    if (count == 0)
        return 1;
    if (strings_equal(ARGUMENT(0), "help"))
    {
        write_text("Commands: help, info, ps, top, kill PID, list [DIR], mkdir DIRECTORY, touch FILE, append FILE TEXT, cat FILE, nano FILE, rm FILE, mv SOURCE DESTINATION, chmod MODE FILE, chown FILE USER GROUP, run PROGRAM [args] [&], run SCRIPT.cmd, groupadd GROUP, useradd USER GROUP, passwd [USER], logout\n");
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
        {
            write_text("Usage: list [DIR]\n");
            command_failed = 1;
        }
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
    else if (strings_equal(ARGUMENT(0), "nano"))
    {
        nano_file(line, count);
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
        command_failed = 1;
    }
    return command_failed == 0;
}

static int run_script_line(char *line, unsigned int line_number)
{
    int count = parse_command(line);

    if (count < 0)
    {
        write_text("Invalid command syntax in script line ");
        append_number(line, 0, line_number);
        write_text(line);
        write_text(".\n");
        command_failed = 1;
        return 0;
    }
    if (count != 0 && !execute_command(line, (unsigned int)count))
    {
        write_text("Script stopped at line ");
        append_number(line, 0, line_number);
        write_text(line);
        write_text(".\n");
        command_failed = 1;
        return 0;
    }
    return 1;
}

static int run_script(const char *path)
{
    char script_line[LINE_CAPACITY];
    unsigned char input[128];
    unsigned int line_length = 0;
    unsigned int line_number = 1;
    unsigned int index;
    int line_overflow = 0;
    int failed = 0;
    int handle;
    int result;

    if (script_depth >= 4)
    {
        write_text("Maximum nested script depth exceeded.\n");
        command_failed = 1;
        return 0;
    }
    script_depth++;
    handle = sc16_open(path, SC16_OPEN_READ);
    if (handle < 0)
    {
        write_text("Unable to open script file.\n");
        script_depth--;
        command_failed = 1;
        return 0;
    }
    for (;;)
    {
        result = sc16_read(handle, input, sizeof(input));
        if (result < 0)
        {
            write_text("Unable to read script file.\n");
            failed = 1;
            break;
        }
        if (result == 0)
            break;
        for (index = 0; index < (unsigned int)result; index++)
        {
            unsigned char character = input[index];

            if (character == '\r')
                continue;
            if (character == '\n')
            {
                if (line_overflow)
                {
                    write_text("Script line exceeds 255 characters.\n");
                    failed = 1;
                }
                else
                {
                    script_line[line_length] = 0;
                    if (!run_script_line(script_line, line_number))
                        failed = 1;
                }
                line_number++;
                line_length = 0;
                line_overflow = 0;
                if (failed)
                    break;
                continue;
            }
            if (character == '\t')
                character = ' ';
            if (character < 32 || character > 126)
            {
                write_text("Script contains unsupported characters.\n");
                failed = 1;
                break;
            }
            if (line_length + 1 < sizeof(script_line) && !line_overflow)
                script_line[line_length++] = (char)character;
            else
                line_overflow = 1;
        }
        if (failed)
            break;
    }
    if (!failed && (line_length != 0 || line_overflow))
    {
        if (line_overflow)
        {
            write_text("Script line exceeds 255 characters.\n");
            failed = 1;
        }
        else
        {
            script_line[line_length] = 0;
            if (!run_script_line(script_line, line_number))
                failed = 1;
        }
    }
    if (sc16_close(handle) < 0)
    {
        write_text("Unable to close script file.\n");
        failed = 1;
    }
    script_depth--;
    if (failed)
        command_failed = 1;
    return failed == 0;
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
        completion_enabled = 1;
        length = read_command(command, LINE_CAPACITY, 1);
        completion_enabled = 0;
        if (length == -3)
            continue;
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
