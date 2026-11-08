/* Three arguments are name, prefix, and the input path.
   Any other argc exits 1 and writes a short line on stderr.
   Each input line is printed with '_' changed to '-', then ", ",
   then prefix concatenated with the original line.
*/
#include <stdio.h>
#include <stdlib.h>

static int trailing_ws(unsigned char c)
{
	return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\v' ||
		c == '\f' || (c >= 0x1c && c <= 0x1f);
}

static int write_header(const char *name)
{
	int n;

	n = printf(
		"%%{\n"
		"_Pragma(\"GCC diagnostic ignored \\\"-Wimplicit-fallthrough\\\"\")\n"
		"#if __GNUC__ >= 15\n"
		"_Pragma(\"GCC diagnostic ignored \\\"-Wzero-as-null-pointer-constant\\\"\")\n"
		"#endif\n"
		"%%}\n"
		"struct %s_name { const char* name; int id; };\n"
		"%%null-strings\n"
		"%%%%\n",
		name);
	return n < 0 ? 1 : 0;
}

static int write_line(const char *line, size_t len, const char *prefix)
{
	size_t i;
	unsigned char ch;

	for (i = 0; i < len; i++) {
		ch = (unsigned char)line[i];
		if (ch == '_') {
			ch = '-';
		}
		if (putchar(ch) == EOF) {
			return 1;
		}
	}
	if (fputs(", ", stdout) == EOF) {
		return 1;
	}
	if (fputs(prefix, stdout) == EOF) {
		return 1;
	}
	if (len > 0 && fwrite(line, 1, len, stdout) != len) {
		return 1;
	}
	if (putchar('\n') == EOF) {
		return 1;
	}
	return 0;
}

static int grow(char **buf, size_t *cap, size_t need)
{
	size_t ncap;
	char *nbuf;

	if (need <= *cap) {
		return 0;
	}
	ncap = *cap == 0 ? 128 : *cap;
	while (ncap < need) {
		if (ncap > ((size_t)-1) / 2) {
			return 1;
		}
		ncap *= 2;
	}
	nbuf = realloc(*buf, ncap);
	if (nbuf == NULL) {
		return 1;
	}
	*buf = nbuf;
	*cap = ncap;
	return 0;
}

/* Text mode ends a line on LF, CR LF, or a bare CR. */
static int write_input(FILE *in, const char *prefix)
{
	char *buf;
	size_t cap;
	size_t len;
	int c;
	int next;
	int rc;

	buf = NULL;
	cap = 0;
	len = 0;
	rc = 0;
	for (;;) {
		c = fgetc(in);
		if (c == EOF) {
			if (ferror(in)) {
				rc = 1;
				break;
			}
			if (len == 0) {
				break;
			}
		} else if (c == '\r' || c == '\n') {
			if (c == '\r') {
				next = fgetc(in);
				if (next != '\n' && next != EOF) {
					if (ungetc(next, in) == EOF) {
						rc = 1;
						break;
					}
				}
				if (next == EOF && ferror(in)) {
					rc = 1;
					break;
				}
			}
		} else {
			if (len == (size_t)-1 || grow(&buf, &cap, len + 1) != 0) {
				fprintf(stderr, "dns-type-gperf: out of memory\n");
				rc = 1;
				break;
			}
			buf[len++] = (char)c;
			continue;
		}

		while (len > 0 && trailing_ws((unsigned char)buf[len - 1])) {
			len--;
		}
		if (write_line(buf, len, prefix) != 0) {
			rc = 1;
			break;
		}
		len = 0;
		if (c == EOF) {
			break;
		}
	}

	free(buf);
	return rc;
}

int main(int argc, char **argv)
{
	FILE *in;
	int rc;

	if (argc != 4) {
		fprintf(stderr, "dns-type-gperf: expected name, prefix, and input\n");
		return 1;
	}

	if (write_header(argv[1]) != 0) {
		return 1;
	}

	in = fopen(argv[3], "r");
	if (in == NULL) {
		fprintf(stderr, "dns-type-gperf: cannot open input\n");
		return 1;
	}

	rc = write_input(in, argv[2]);
	if (fclose(in) != 0) {
		rc = 1;
	}
	if (rc != 0) {
		return 1;
	}
	if (fflush(stdout) != 0 || ferror(stdout)) {
		return 1;
	}
	return 0;
}
