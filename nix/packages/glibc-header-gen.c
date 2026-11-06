/* Library-build header generator for the mail-host glibc.
   as-const: same header lines as scripts/gen-as-const.py (not --test, not --python).
   translit: same header text as locale/gen-translit.py.
   This file is source only. The glibc build compiles it. */

#define _POSIX_C_SOURCE 200809L

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static int
is_ws(unsigned char c)
{
  return c == ' ' || c == '\t' || c == '\v' || c == '\f' || c == '\r';
}

static int
is_hex(unsigned char c)
{
  return (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F');
}

static unsigned
hex_val(unsigned char c)
{
  if (c >= '0' && c <= '9')
    return (unsigned)(c - '0');
  if (c >= 'a' && c <= 'f')
    return (unsigned)(c - 'a' + 10);
  return (unsigned)(c - 'A' + 10);
}

static int
is_val_char(unsigned char c)
{
  return is_hex(c) || c == 'x' || c == 'X' || c == '-';
}

static void
die(const char *msg)
{
  fprintf(stderr, "glibc-header-gen: %s\n", msg);
  exit(1);
}

static void *
xmalloc(size_t n)
{
  void *p = malloc(n);
  if (p == NULL)
    die("out of memory");
  return p;
}

static void *
xrealloc(void *p, size_t n)
{
  void *q = realloc(p, n);
  if (q == NULL)
    die("out of memory");
  return q;
}

static char *
dup_n(const char *s, size_t n)
{
  char *d = xmalloc(n + 1);
  memcpy(d, s, n);
  d[n] = '\0';
  return d;
}

static char *
dup_str(const char *s)
{
  return dup_n(s, strlen(s));
}

/* Read one line without the newline. 1 = line, 0 = EOF, -1 = error. */
static int
read_line(FILE *fp, char **out)
{
  size_t cap = 128;
  size_t n = 0;
  char *buf = xmalloc(cap);
  int c;

  while ((c = fgetc(fp)) != EOF) {
    if (c == '\n')
      break;
    if (n + 1 >= cap) {
      cap *= 2;
      buf = xrealloc(buf, cap);
    }
    buf[n++] = (char)c;
  }
  if (c == EOF && n == 0) {
    free(buf);
    *out = NULL;
    return 0;
  }
  if (n > 0 && buf[n - 1] == '\r')
    n--;
  buf[n] = '\0';
  *out = buf;
  return 1;
}

static void
strip_inplace(char *s)
{
  unsigned char *p = (unsigned char *)s;
  size_t n;

  while (*p != '\0' && is_ws(*p))
    p++;
  if (p != (unsigned char *)s)
    memmove(s, p, strlen((char *)p) + 1);
  n = strlen(s);
  while (n > 0 && is_ws((unsigned char)s[n - 1])) {
    n--;
    s[n] = '\0';
  }
}

/* ---- as-const -------------------------------------------------------- */

enum { SYM_LINE = 1, SYM_START = 2, SYM_PAIR = 3 };

struct sym_item {
  int kind;
  char *a;
  char *b;
};

struct sym_list {
  struct sym_item *items;
  size_t n;
  size_t cap;
};

static void
sym_add(struct sym_list *list, int kind, const char *a, const char *b)
{
  struct sym_item *it;

  if (list->n == list->cap) {
    list->cap = list->cap == 0 ? 16 : list->cap * 2;
    list->items = xrealloc(list->items, list->cap * sizeof *list->items);
  }
  it = &list->items[list->n++];
  it->kind = kind;
  it->a = a == NULL ? NULL : dup_str(a);
  it->b = b == NULL ? NULL : dup_str(b);
}

static void
parse_sym_file(const char *path, struct sym_list *list)
{
  FILE *fp = fopen(path, "r");
  char *line = NULL;
  int started = 0;
  int rc;

  if (fp == NULL)
    die("cannot open sym file");
  list->items = NULL;
  list->n = 0;
  list->cap = 0;
  while ((rc = read_line(fp, &line)) == 1) {
    char *words;
    char *name;
    char *value;

    strip_inplace(line);
    if (line[0] == '\0') {
      free(line);
      continue;
    }
    if (line[0] == '#') {
      sym_add(list, SYM_LINE, line, NULL);
      free(line);
      continue;
    }
    words = line;
    while (*words != '\0' && is_ws((unsigned char)*words))
      words++;
    name = words;
    while (*words != '\0' && !is_ws((unsigned char)*words))
      words++;
    if (*words != '\0') {
      *words = '\0';
      words++;
      while (*words != '\0' && is_ws((unsigned char)*words))
        words++;
      value = words;
    } else {
      value = name;
    }
    if (!started) {
      sym_add(list, SYM_START, NULL, NULL);
      started = 1;
    }
    if (strcmp(name, "--") == 0) {
      free(line);
      continue;
    }
    sym_add(list, SYM_PAIR, name, value);
    free(line);
  }
  if (rc < 0)
    die("cannot read sym file");
  fclose(fp);
  if (!started)
    sym_add(list, SYM_START, NULL, NULL);
}

static void
write_probe_c(FILE *out, const struct sym_list *list)
{
  size_t i;

  for (i = 0; i < list->n; i++) {
    const struct sym_item *it = &list->items[i];
    if (it->kind == SYM_START) {
      fputs("void\ndummy (void)\n{\n", out);
    } else if (it->kind == SYM_LINE) {
      fputs(it->a, out);
      fputc('\n', out);
    } else if (it->kind == SYM_PAIR) {
      fputs("asm (\"/* @@@name@@@", out);
      fputs(it->a, out);
      fputs("@@@value@@@%0@@@end@@@ */\" : : \"i\" ((long int) (", out);
      fputs(it->b, out);
      fputs(")));\n", out);
    }
  }
  fputs("}\n", out);
}

struct c_const {
  char *name;
  char *value;
};

static int
cmp_const(const void *va, const void *vb)
{
  const struct c_const *a = va;
  const struct c_const *b = vb;
  int c = strcmp(a->name, b->name);
  if (c != 0)
    return c;
  return strcmp(a->value, b->value);
}

/* First match of the glibcextract assembly comment, per line. */
static int
parse_asm_line(const char *line, char **name_out, char **value_out)
{
  const char *scan = line;

  while ((scan = strstr(scan, "@@@name@@@")) != NULL) {
    const char *p = scan + strlen("@@@name@@@");
    const char *name_b = p;
    const char *name_e = strstr(p, "@@@");
    const char *val_b;
    size_t name_n;
    size_t val_n;

    if (name_e == NULL)
      return 0;
    if (strncmp(name_e, "@@@value@@@", strlen("@@@value@@@")) != 0) {
      scan += 1;
      continue;
    }
    name_n = (size_t)(name_e - name_b);
    p = name_e + strlen("@@@value@@@");
    while (*p != '\0' && !is_val_char((unsigned char)*p))
      p++;
    if (*p == '\0') {
      scan += 1;
      continue;
    }
    val_b = p;
    while (is_val_char((unsigned char)*p))
      p++;
    if (strstr(p, "@@@end@@@") == NULL) {
      scan += 1;
      continue;
    }
    val_n = (size_t)(p - val_b);
    if (val_n == 0) {
      scan += 1;
      continue;
    }
    *name_out = dup_n(name_b, name_n);
    *value_out = dup_n(val_b, val_n);
    return 1;
  }
  return 0;
}

static void
note_const(struct c_const **arr, size_t *n, size_t *cap, char *name, char *value)
{
  size_t i;

  for (i = 0; i < *n; i++) {
    if (strcmp((*arr)[i].name, name) == 0) {
      if (strcmp((*arr)[i].value, value) != 0)
        die("duplicate constant with two values");
      free(name);
      free(value);
      return;
    }
  }
  if (*n == *cap) {
    *cap = *cap == 0 ? 16 : *cap * 2;
    *arr = xrealloc(*arr, *cap * sizeof **arr);
  }
  (*arr)[*n].name = name;
  (*arr)[*n].value = value;
  (*n)++;
}

static char *
temp_dir(void)
{
  const char *base = getenv("TMPDIR");
  char *path;
  size_t n;

  if (base == NULL || base[0] == '\0')
    base = "/tmp";
  n = strlen(base) + strlen("/glibc-as-const-XXXXXX") + 1;
  path = xmalloc(n);
  snprintf(path, n, "%s/glibc-as-const-XXXXXX", base);
  if (mkdtemp(path) == NULL)
    die("cannot make temp directory");
  return path;
}

static void
run_cc(const char *cc, const char *s_path, const char *c_path)
{
  size_t n = strlen(cc) + strlen(s_path) + strlen(c_path) + 32;
  char *cmd = xmalloc(n);
  int wr = snprintf(cmd, n, "%s -S -o %s -x c - < %s", cc, s_path, c_path);
  int rc;

  if (wr < 0 || (size_t)wr >= n)
    die("compiler command does not fit");
  rc = system(cmd);
  free(cmd);
  if (rc != 0)
    die("compiler failed");
}

static void
emit_as_const(const char *cc, const char *sym_path)
{
  struct sym_list list;
  char *dir;
  char *c_path;
  char *s_path;
  FILE *c_file;
  FILE *s_file;
  struct c_const *consts = NULL;
  size_t nconst = 0;
  size_t cap = 0;
  size_t i;
  char *line = NULL;
  int rc;

  if (cc == NULL || cc[0] == '\0' || sym_path == NULL)
    die("as-const needs --cc and a sym file");
  parse_sym_file(sym_path, &list);
  dir = temp_dir();
  c_path = xmalloc(strlen(dir) + strlen("/test.c") + 1);
  s_path = xmalloc(strlen(dir) + strlen("/test.s") + 1);
  sprintf(c_path, "%s/test.c", dir);
  sprintf(s_path, "%s/test.s", dir);
  c_file = fopen(c_path, "w");
  if (c_file == NULL)
    die("cannot write probe");
  write_probe_c(c_file, &list);
  if (fclose(c_file) != 0)
    die("cannot close probe");
  run_cc(cc, s_path, c_path);
  s_file = fopen(s_path, "r");
  if (s_file == NULL)
    die("cannot read assembly");
  while ((rc = read_line(s_file, &line)) == 1) {
    char *name = NULL;
    char *value = NULL;
    if (parse_asm_line(line, &name, &value))
      note_const(&consts, &nconst, &cap, name, value);
    free(line);
  }
  if (rc < 0)
    die("cannot read assembly");
  fclose(s_file);
  qsort(consts, nconst, sizeof *consts, cmp_const);
  for (i = 0; i < nconst; i++) {
    fputs("#define ", stdout);
    fputs(consts[i].name, stdout);
    fputc(' ', stdout);
    fputs(consts[i].value, stdout);
    fputc('\n', stdout);
  }
  remove(c_path);
  remove(s_path);
  rmdir(dir);
}

/* ---- translit -------------------------------------------------------- */

struct translit {
  char *cp_src;
  char *rep_src;
  uint32_t *cp_dec;
  size_t cp_n;
  uint32_t *rep_dec;
  size_t rep_n;
};

static int
decode_literal(const char *s, uint32_t **out, size_t *out_n)
{
  size_t cap = 8;
  size_t n = 0;
  uint32_t *r = xmalloc(cap * sizeof *r);
  int state = 0;
  const unsigned char *p;

  for (p = (const unsigned char *)s; *p != '\0'; p++) {
    unsigned char ch = *p;
    if (state == 0) {
      if (ch == '\\') {
        state = 1;
      } else {
        if (n == cap) {
          cap *= 2;
          r = xrealloc(r, cap * sizeof *r);
        }
        r[n++] = ch;
      }
    } else if (state == 1) {
      if (ch == '\\' || ch == '"') {
        if (n == cap) {
          cap *= 2;
          r = xrealloc(r, cap * sizeof *r);
        }
        r[n++] = ch;
        state = 0;
      } else if (ch == 'x') {
        if (n == cap) {
          cap *= 2;
          r = xrealloc(r, cap * sizeof *r);
        }
        r[n++] = 0;
        state = 2;
      } else {
        free(r);
        return -1;
      }
    } else if (is_hex(ch)) {
      uint32_t cur = r[n - 1];
      unsigned digit = hex_val(ch);
      if (cur > (UINT32_MAX - digit) / 16) {
        free(r);
        return -1;
      }
      r[n - 1] = cur * 16u + digit;
    } else if (ch == '\\') {
      state = 1;
    } else {
      state = 0;
    }
  }
  if (state == 1) {
    free(r);
    return -1;
  }
  *out = r;
  *out_n = n;
  return 0;
}

/* Whole-line match of gen-translit.py __RE_TRANSLIT, after strip. */
static int
parse_translit_line(const char *s, char **cp, char **rep)
{
  const char *p = s;
  const char *start;
  const char *start2;
  size_t n1;
  size_t n2;
  int any = 0;

  if (*p != '"')
    return -1;
  p++;
  start = p;
  while (*p != '\0' && *p != '"') {
    if (*p == '\\') {
      if (p[1] == 'x' && is_hex((unsigned char)p[2])) {
        p += 3;
        any = 1;
        continue;
      }
      return -1;
    }
    p++;
    any = 1;
  }
  if (!any || *p != '"')
    return -1;
  n1 = (size_t)(p - start);
  p++;
  if (!is_ws((unsigned char)*p))
    return -1;
  while (is_ws((unsigned char)*p))
    p++;
  if (*p != '"')
    return -1;
  p++;
  start2 = p;
  while (*p != '\0' && *p != '"') {
    if (*p == '\\') {
      if (p[1] == '"' || p[1] == '\\') {
        p += 2;
        continue;
      }
      return -1;
    }
    p++;
  }
  if (*p != '"')
    return -1;
  n2 = (size_t)(p - start2);
  p++;
  while (is_ws((unsigned char)*p))
    p++;
  if (*p == '#') {
    while (*p != '\0')
      p++;
  }
  if (*p != '\0')
    return -1;
  *cp = dup_n(start, n1);
  *rep = dup_n(start2, n2);
  return 0;
}

static int
cmp_decoded(const uint32_t *a, size_t na, const uint32_t *b, size_t nb)
{
  size_t n = na < nb ? na : nb;
  size_t i;

  for (i = 0; i < n; i++) {
    if (a[i] < b[i])
      return -1;
    if (a[i] > b[i])
      return 1;
  }
  if (na < nb)
    return -1;
  if (na > nb)
    return 1;
  return 0;
}

static void
write_u4(unsigned long n)
{
  char buf[64];
  int k = snprintf(buf, sizeof buf, "%4lu", n);

  if (k < 0 || (size_t)k >= sizeof buf)
    die("index does not fit");
  fputs(buf, stdout);
}

static void
write_idx(const char *header, const struct translit *t, size_t n, int from_tbl)
{
  int col = 2;
  unsigned long total = 0;
  size_t i;

  fputs(header, stdout);
  for (i = 0; i < n; i++) {
    size_t dec_n = from_tbl ? t[i].cp_n : t[i].rep_n;
    unsigned long extra = from_tbl ? 1ul : 2ul;

    if (total > 0) {
      if (col + 7 >= 79) {
        fputs(",\n  ", stdout);
        col = 2;
      } else {
        fputs(", ", stdout);
        col += 2;
      }
    }
    write_u4(total);
    total += (unsigned long)dec_n + extra;
    col += 4;
  }
  fputs("\n};\n", stdout);
}

static void
write_tbl(const struct translit *t, size_t n, int from_tbl)
{
  int col = 1;
  int first = 1;
  size_t i;
  const char *header = from_tbl
    ? "static const wchar_t translit_from_tbl[] =\n "
    : "static const wchar_t translit_to_tbl[] =\n ";

  fputs(header, stdout);
  for (i = 0; i < n; i++) {
    const char *src = from_tbl ? t[i].cp_src : t[i].rep_src;
    int sl = (int)strlen(src);
    int wrap_extra = from_tbl ? 4 : 6;
    int col_extra = from_tbl ? 3 : 5;

    if (first) {
      first = 0;
    } else {
      if (col + 6 >= 79) {
        fputs("\n ", stdout);
        col = 1;
      }
      fputs(" L\"\\0\"", stdout);
      col += 6;
    }
    if (col > 2 && col + sl + wrap_extra >= 79) {
      fputs("\n  ", stdout);
      col = 2;
    } else {
      fputc(' ', stdout);
      col += 1;
    }
    fputs("L\"", stdout);
    fputs(src, stdout);
    if (from_tbl)
      fputc('"', stdout);
    else
      fputs("\\0\"", stdout);
    col += sl + col_extra;
  }
  fputs(";\n", stdout);
}

static void
emit_translit(void)
{
  struct translit *rows = NULL;
  size_t n = 0;
  size_t cap = 0;
  char *line = NULL;
  unsigned long lineno = 0;
  int rc;

  while ((rc = read_line(stdin, &line)) == 1) {
    char *cp = NULL;
    char *rep = NULL;
    struct translit *row;

    lineno++;
    strip_inplace(line);
    if (line[0] == '\0' || line[0] == '#') {
      free(line);
      continue;
    }
    if (parse_translit_line(line, &cp, &rep) != 0) {
      fprintf(stderr, "glibc-header-gen: invalid line %lu\n", lineno);
      exit(1);
    }
    if (n == cap) {
      cap = cap == 0 ? 32 : cap * 2;
      rows = xrealloc(rows, cap * sizeof *rows);
    }
    row = &rows[n];
    row->cp_src = cp;
    row->rep_src = rep;
    if (decode_literal(cp, &row->cp_dec, &row->cp_n) != 0)
      die("invalid codepoint string");
    if (decode_literal(rep, &row->rep_dec, &row->rep_n) != 0)
      die("invalid replacement string");
    if (n > 0 &&
        cmp_decoded(row->cp_dec, row->cp_n, rows[n - 1].cp_dec, rows[n - 1].cp_n) <= 0) {
      fprintf(stderr, "glibc-header-gen: codepoints not increasing on line %lu\n", lineno);
      exit(1);
    }
    n++;
    free(line);
  }
  if (rc < 0)
    die("cannot read transliteration input");

  fputs("#include <stdint.h>\n", stdout);
  fprintf(stdout, "#define NTRANSLIT %zu\n", n);
  write_idx("static const uint32_t translit_from_idx[] =\n{\n  ", rows, n, 1);
  write_tbl(rows, n, 1);
  write_idx("static const uint32_t translit_to_idx[] =\n{\n  ", rows, n, 0);
  write_tbl(rows, n, 0);
}

static const char *
arg_cc(int argc, char **argv, const char **sym)
{
  const char *cc = NULL;
  int i;

  *sym = NULL;
  for (i = 2; i < argc; i++) {
    if (strcmp(argv[i], "--test") == 0 || strcmp(argv[i], "--python") == 0)
      die("test and python output are not the library header");
    if (strncmp(argv[i], "--cc=", 5) == 0) {
      cc = argv[i] + 5;
    } else if (strcmp(argv[i], "--cc") == 0) {
      if (i + 1 >= argc)
        die("missing --cc value");
      i++;
      cc = argv[i];
    } else if (argv[i][0] == '-') {
      die("unknown option");
    } else if (*sym != NULL) {
      die("extra sym file");
    } else {
      *sym = argv[i];
    }
  }
  return cc;
}

int
main(int argc, char **argv)
{
  if (argc < 2)
    die("usage: glibc-header-gen as-const --cc=CC SYM | translit");
  if (strcmp(argv[1], "as-const") == 0) {
    const char *sym = NULL;
    const char *cc = arg_cc(argc, argv, &sym);
    emit_as_const(cc, sym);
    return 0;
  }
  if (strcmp(argv[1], "translit") == 0) {
    if (argc != 2)
      die("translit reads stdin and takes no arguments");
    emit_translit();
    return 0;
  }
  die("unknown mode");
  return 1;
}
