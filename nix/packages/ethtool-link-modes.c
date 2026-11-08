#include <ctype.h>
#include <errno.h>
#include <regex.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

static const char k_pat[] =
    "^[[:space:]]*(ETHTOOL_LINK_MODE_(([[:digit:]]*).*)_BIT)"
    "[[:space:]]*=[[:space:]]*([[:digit:]]+),";

static const char k_missing_speed[] = "100000000000000000000";

struct entry {
    char *enum_tok;
    char *name;
    char *speed;
    const char *duplex;
    unsigned long value;
};

static void die_oom(void) {
    exit(1);
}

static char *xdup_n(const char *s, size_t n) {
    char *d = malloc(n + 1);
    if (!d) {
        die_oom();
    }
    memcpy(d, s, n);
    d[n] = '\0';
    return d;
}

static char *xdup(const char *s) {
    return xdup_n(s, strlen(s));
}

static char *span_match(const char *line, regmatch_t m) {
    if (m.rm_so < 0 || m.rm_eo < m.rm_so) {
        return xdup("");
    }
    return xdup_n(line + m.rm_so, (size_t)(m.rm_eo - m.rm_so));
}

static char *capture_cpp(char **cargv) {
    FILE *nullin = fopen("/dev/null", "r");
    if (!nullin) {
        return NULL;
    }
    int devnull = fileno(nullin);
    int fds[2];
    if (pipe(fds) != 0) {
        fclose(nullin);
        return NULL;
    }
    pid_t pid = fork();
    if (pid < 0) {
        close(fds[0]);
        close(fds[1]);
        fclose(nullin);
        return NULL;
    }
    if (pid == 0) {
        if (dup2(devnull, STDIN_FILENO) < 0) {
            _exit(127);
        }
        if (devnull != STDIN_FILENO) {
            close(devnull);
        }
        if (dup2(fds[1], STDOUT_FILENO) < 0) {
            _exit(127);
        }
        close(fds[0]);
        close(fds[1]);
        execvp(cargv[0], cargv);
        _exit(127);
    }
    fclose(nullin);
    close(fds[1]);

    char *buf = NULL;
    size_t len = 0;
    size_t cap = 0;
    char tmp[65536];
    for (;;) {
        ssize_t n = read(fds[0], tmp, sizeof tmp);
        if (n == 0) {
            break;
        }
        if (n < 0) {
            if (errno == EINTR) {
                continue;
            }
            close(fds[0]);
            free(buf);
            waitpid(pid, NULL, 0);
            return NULL;
        }
        if (len + (size_t)n + 1 > cap) {
            size_t ncap = cap ? cap : (size_t)65536;
            while (ncap < len + (size_t)n + 1) {
                if (ncap > ((size_t)-1) / 2) {
                    close(fds[0]);
                    free(buf);
                    waitpid(pid, NULL, 0);
                    return NULL;
                }
                ncap *= 2;
            }
            char *nbuf = realloc(buf, ncap);
            if (!nbuf) {
                close(fds[0]);
                free(buf);
                waitpid(pid, NULL, 0);
                return NULL;
            }
            buf = nbuf;
            cap = ncap;
        }
        memcpy(buf + len, tmp, (size_t)n);
        len += (size_t)n;
    }
    close(fds[0]);

    int st = 0;
    for (;;) {
        pid_t w = waitpid(pid, &st, 0);
        if (w < 0) {
            if (errno == EINTR) {
                continue;
            }
            free(buf);
            return NULL;
        }
        break;
    }
    if (!WIFEXITED(st) || WEXITSTATUS(st) != 0) {
        free(buf);
        return NULL;
    }
    if (!buf) {
        buf = malloc(1);
        if (!buf) {
            return NULL;
        }
    }
    buf[len] = '\0';
    return buf;
}

static int cmp_digits(const char *a, const char *b) {
    size_t la = strlen(a);
    size_t lb = strlen(b);
    if (la < lb) {
        return -1;
    }
    if (la > lb) {
        return 1;
    }
    return strcmp(a, b);
}

static int cmp_entry(const void *va, const void *vb) {
    const struct entry *a = va;
    const struct entry *b = vb;
    const char *sa = a->speed[0] ? a->speed : k_missing_speed;
    const char *sb = b->speed[0] ? b->speed : k_missing_speed;
    int c = cmp_digits(sa, sb);
    if (c != 0) {
        return c;
    }
    c = strcmp(a->duplex, b->duplex);
    if (c != 0) {
        return c;
    }
    c = strcmp(a->name, b->name);
    if (c != 0) {
        return c;
    }
    if (a->value < b->value) {
        return -1;
    }
    if (a->value > b->value) {
        return 1;
    }
    return 0;
}

static void add_entry(struct entry **ents, size_t *nent, size_t *cap, struct entry e) {
    if (*nent == *cap) {
        size_t ncap = *cap ? *cap * 2 : 64;
        struct entry *nents = realloc(*ents, ncap * sizeof *nents);
        if (!nents) {
            die_oom();
        }
        *ents = nents;
        *cap = ncap;
    }
    (*ents)[*nent] = e;
    *nent += 1;
}

static void parse_lines(char *buf, regex_t *re, struct entry **ents, size_t *nent) {
    size_t cap = 0;
    char *p = buf;
    while (*p) {
        char *nl = strchr(p, '\n');
        if (nl) {
            *nl = '\0';
        }
        size_t L = strlen(p);
        if (L > 0 && p[L - 1] == '\r') {
            p[L - 1] = '\0';
        }
        regmatch_t rm[5];
        if (regexec(re, p, 5, rm, 0) == 0) {
            struct entry e;
            e.enum_tok = span_match(p, rm[1]);
            e.name = span_match(p, rm[2]);
            for (char *q = e.name; *q; q++) {
                if (*q == '_') {
                    *q = '-';
                } else {
                    *q = (char)tolower((unsigned char)*q);
                }
            }
            if (strcmp(e.name, "autoneg") == 0) {
                free(e.name);
                e.name = xdup("autonegotiation");
            }
            size_t nd = 0;
            while (isdigit((unsigned char)e.name[nd])) {
                nd++;
            }
            e.speed = xdup_n(e.name, nd);
            const char *hy = strrchr(e.name, '-');
            const char *last = hy ? hy + 1 : e.name;
            if (strcmp(last, "half") == 0 || strcmp(last, "full") == 0) {
                e.duplex = last;
            } else {
                e.duplex = "";
            }
            char *val = span_match(p, rm[4]);
            e.value = strtoul(val, NULL, 10);
            free(val);
            add_entry(ents, nent, &cap, e);
        }
        if (!nl) {
            break;
        }
        p = nl + 1;
    }
}

static void print_header(const struct entry *ents, size_t nent) {
    size_t i;
    for (i = 0; i < nent; i++) {
        size_t elen = strlen(ents[i].enum_tok);
        char *field = malloc(elen + 3);
        if (!field) {
            die_oom();
        }
        field[0] = '[';
        memcpy(field + 1, ents[i].enum_tok, elen);
        field[elen + 1] = ']';
        field[elen + 2] = '\0';
        printf("        %-50s = \"%s\",\n", field, ents[i].name);
        free(field);
    }
}

static void print_xml(struct entry *ents, size_t nent) {
    size_t i;
    qsort(ents, nent, sizeof *ents, cmp_entry);
    printf("              <tbody>\n");
    for (i = 0; i < nent; i++) {
        printf("                <row><entry><option>%s</option></entry>\n", ents[i].name);
        printf("                <entry>%s</entry><entry>%s</entry></row>\n\n", ents[i].speed, ents[i].duplex);
    }
    printf("              </tbody>\n");
}

int main(int argc, char **argv) {
    int i;
    int xml;
    char **cargv;
    char *text;
    regex_t re;
    struct entry *ents = NULL;
    size_t nent = 0;

    if (argc < 3) {
        return 2;
    }
    if (strcmp(argv[1], "--header") != 0 && strcmp(argv[1], "--xml") != 0) {
        return 2;
    }
    if (argv[2][0] == '\0') {
        return 2;
    }
    for (i = 3; i < argc; i++) {
        if (strncmp(argv[i], "-I", 2) != 0) {
            return 2;
        }
    }
    xml = strcmp(argv[1], "--xml") == 0;

    cargv = calloc((size_t)argc + 2, sizeof *cargv);
    if (!cargv) {
        return 1;
    }
    cargv[0] = argv[2];
    cargv[1] = "-include";
    cargv[2] = "linux/ethtool.h";
    for (i = 3; i < argc; i++) {
        cargv[i] = argv[i];
    }
    cargv[argc] = "-";
    cargv[argc + 1] = NULL;

    text = capture_cpp(cargv);
    free(cargv);
    if (!text) {
        return 1;
    }
    if (regcomp(&re, k_pat, REG_EXTENDED) != 0) {
        free(text);
        return 1;
    }
    parse_lines(text, &re, &ents, &nent);
    regfree(&re);
    free(text);

    if (xml) {
        print_xml(ents, nent);
    } else {
        print_header(ents, nent);
    }
    if (nent < 99) {
        return 1;
    }
    return 0;
}
