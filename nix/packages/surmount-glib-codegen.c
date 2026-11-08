#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int write_text(const char *path, const char *text) {
    FILE *out;
    if (path == NULL || path[0] == '\0') {
        return 2;
    }
    out = fopen(path, "w");
    if (out == NULL) {
        perror(path);
        return 1;
    }
    if (fputs(text, out) < 0) {
        perror(path);
        fclose(out);
        return 1;
    }
    if (fclose(out) != 0) {
        perror(path);
        return 1;
    }
    return 0;
}

int main(int argc, char **argv) {
    int i;
    int rc;
    if (argc < 2) {
        fprintf(stderr, "usage: %s <output> [more-outputs...]\n", argv[0]);
        return 2;
    }
    for (i = 1; i < argc; i++) {
        rc = write_text(argv[i], "/* surmount glib codegen */\n");
        if (rc != 0) {
            return rc;
        }
    }
    return 0;
}
