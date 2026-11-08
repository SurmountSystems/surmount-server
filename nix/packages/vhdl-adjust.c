/* Adjusts javacc VHDL sources the same way the upstream text pass does. */
#define _POSIX_C_SOURCE 200809L

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void write_adjusted(const char *line, FILE *out)
{
	const char *p = line;

	while (*p != '\0') {
		if (strncmp(p, "assert(false);", 14) == 0) {
			fputs("assert(false);return QCString();", out);
			p += 14;
			continue;
		}
		if (strncmp(p, "message", 7) == 0) {
			const char *q = p + 7;

			while (*q == ' ' || *q == '\t') {
				q++;
			}
			if (q[0] == '+' && q[1] == '=') {
				const char *end;

				q += 2;
				while (*q == ' ' || *q == '\t') {
					q++;
				}
				if (*q == '"') {
					end = q + 1;
					while (*end != '\0' && *end != '"') {
						if (*end == '\\' && end[1] != '\0') {
							end += 2;
						} else {
							end++;
						}
					}
					if (*end == '"') {
						fputs("message += reinterpret_cast<const JJChar*>(", out);
						fwrite(q, 1, (size_t)(end - q + 1), out);
						fputc(')', out);
						p = end + 1;
						continue;
					}
				}
			}
		}
		fputc(*p, out);
		p++;
	}
}

int main(int argc, char **argv)
{
	const char *in_path;
	const char *out_path;
	FILE *in;
	FILE *out;
	char *line = NULL;
	size_t cap = 0;

	if (argc < 3) {
		return 0;
	}
	in_path = argv[argc - 2];
	out_path = argv[argc - 1];
	in = fopen(in_path, "r");
	if (in == NULL) {
		return 1;
	}
	out = fopen(out_path, "w");
	if (out == NULL) {
		fclose(in);
		return 1;
	}
	while (getline(&line, &cap, in) != -1) {
		write_adjusted(line, out);
	}
	free(line);
	fclose(in);
	if (fclose(out) != 0) {
		return 1;
	}
	return 0;
}
