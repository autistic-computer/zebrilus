#define _POSIX_C_SOURCE 200809L
#include <ghostty/vt/terminal.h>
#include <ghostty/vt/formatter.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <time.h>
#include <sys/resource.h>

static double stamp(clockid_t clock) {
    struct timespec now;
    if (clock_gettime(clock, &now)) abort();
    return now.tv_sec + now.tv_nsec / 1e9;
}

static GhosttyTerminal create(uint16_t cols, uint16_t rows) {
    GhosttyTerminal terminal = NULL;
    size_t lines = 2000;
    if (ghostty_terminal_new(NULL, &terminal, cols, rows) != GHOSTTY_SUCCESS) abort();
    if (ghostty_terminal_set(terminal, GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_LINES, &lines) != GHOSTTY_SUCCESS) abort();
    return terminal;
}

static uint64_t digest(GhosttyTerminal terminal) {
    GhosttyFormatter formatter = NULL;
    GhosttyFormatterTerminalOptions options = { .size = sizeof(options), .emit = GHOSTTY_FORMATTER_FORMAT_VT };
    options.extra.size = sizeof(options.extra);
    options.extra.screen.size = sizeof(options.extra.screen);
    options.extra.screen.cursor = true;
    options.extra.screen.hyperlink = true;
    options.extra.screen.style = true;
    if (ghostty_formatter_terminal_new(NULL, &formatter, terminal, options) != GHOSTTY_SUCCESS) abort();
    size_t size = 0;
    GhosttyResult result = ghostty_formatter_format_buf(formatter, NULL, 0, &size);
    if (result != GHOSTTY_OUT_OF_SPACE && result != GHOSTTY_SUCCESS) abort();
    uint8_t *bytes = malloc(size ? size : 1);
    if (!bytes) abort();
    if (ghostty_formatter_format_buf(formatter, bytes, size, &size) != GHOSTTY_SUCCESS) abort();
    uint64_t hash = UINT64_C(14695981039346656037);
    for (size_t i = 0; i < size; ++i) hash = (hash ^ bytes[i]) * UINT64_C(1099511628211);
    free(bytes);
    ghostty_formatter_free(formatter);
    return hash;
}

int main(int argc, char **argv) {
    if (argc != 3 && argc != 5) return 2;
    size_t amount = strtoull(argv[2], NULL, 10);
    unsigned long cols = argc == 5 ? strtoul(argv[3], NULL, 10) : 120;
    unsigned long rows = argc == 5 ? strtoul(argv[4], NULL, 10) : 40;
    if (cols == 0 || cols > UINT16_MAX || rows == 0 || rows > UINT16_MAX) return 2;
    uint8_t *data = NULL;
    size_t length = 0;
    int churn = strcmp(argv[1], "churn") == 0;
    if (!churn) {
        data = malloc(amount);
        if (!data) abort();
        if (strcmp(argv[1], "ascii") == 0 || strcmp(argv[1], "links") == 0) {
            const char *line = strcmp(argv[1], "ascii") == 0
                ? "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ\r\n"
                : "\033]8;id=fixed;https://example.com/a-long-stable-url/abcdefghijklmnopqrstuvwxyz\033\\abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ\033]8;;\033\\\r\n";
            size_t line_len = strlen(line);
            while (length < amount) {
                size_t n = line_len < amount - length ? line_len : amount - length;
                memcpy(data + length, line, n);
                length += n;
            }
        } else {
            FILE *file = fopen(argv[1], "rb");
            if (!file) return 3;
            length = fread(data, 1, amount, file);
            if (length != amount) return 4;
            fclose(file);
        }
    }
    struct rusage before, after;
    getrusage(RUSAGE_SELF, &before);
    double wall = stamp(CLOCK_MONOTONIC);
    double cpu = stamp(CLOCK_PROCESS_CPUTIME_ID);
    GhosttyTerminal terminal = NULL;
    if (churn) {
        for (size_t i = 0; i < amount; ++i) {
            terminal = create((uint16_t)cols, (uint16_t)rows);
            ghostty_terminal_vt_write(terminal, (const uint8_t *)"hello\r\n", 7);
            ghostty_terminal_free(terminal);
        }
    } else {
        terminal = create((uint16_t)cols, (uint16_t)rows);
        for (size_t i = 0; i < length; i += 65536) {
            size_t count = length - i < 65536 ? length - i : 65536;
            ghostty_terminal_vt_write(terminal, data + i, count);
        }
    }
    cpu = stamp(CLOCK_PROCESS_CPUTIME_ID) - cpu;
    wall = stamp(CLOCK_MONOTONIC) - wall;
    getrusage(RUSAGE_SELF, &after);
    uint64_t hash = churn ? 0 : digest(terminal);
    if (!churn) ghostty_terminal_free(terminal);
    printf("{\"mode\":\"%s\",\"amount\":%zu,\"columns\":%lu,\"rows\":%lu,\"wall_s\":%.9f,\"cpu_s\":%.9f,\"minor_faults\":%ld,\"state_hash\":\"%016llx\"}\n", argv[1], amount, cols, rows, wall, cpu, after.ru_minflt - before.ru_minflt, (unsigned long long)hash);
    free(data);
    return 0;
}
