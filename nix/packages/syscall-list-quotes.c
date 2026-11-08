/* One argument is a name list, one name per line. Any other argc exits 2
   and writes nothing. Each line is stripped of surrounding ASCII
   whitespace and printed as a double-quoted string ending in \0.
*/
#include <stdio.h>

static int is_ws(unsigned char c)
{
	return c == ' ' || c == '\t' || c == '\n' || c == '\r'
		|| c == '\v' || c == '\f'
		|| c == 0x1c || c == 0x1d || c == 0x1e || c == 0x1f;
}

static int emit_quoted(const char *start, const char *end)
{
	size_t n;

	if (fputc('"', stdout) == EOF) {
		return 1;
	}
	n = (size_t)(end - start);
	if (n > 0 && fwrite(start, 1, n, stdout) != n) {
		return 1;
	}
	if (fputs("\\0\"\n", stdout) == EOF) {
		return 1;
	}
	return 0;
}

int main(int argc, char **argv)
{
	FILE *in;
	char buf[4096];

	if (argc != 2) {
		return 2;
	}

	in = fopen(argv[1], "r");
	if (in == NULL) {
		return 1;
	}

	while (fgets(buf, (int)sizeof buf, in) != NULL) {
		char *start;
		char *end;
		int saw_nl;

		start = buf;
		end = buf;
		saw_nl = 0;
		while (*end != '\0') {
			if (*end == '\n') {
				saw_nl = 1;
			}
			end++;
		}
		if (!saw_nl && (size_t)(end - buf) == sizeof buf - 1) {
			int extra;

			extra = fgetc(in);
			if (extra != '\n' && extra != EOF) {
				fclose(in);
				return 1;
			}
		}
		while (start < end && is_ws((unsigned char)*start)) {
			start++;
		}
		while (end > start && is_ws((unsigned char)end[-1])) {
			end--;
		}
		if (emit_quoted(start, end) != 0) {
			fclose(in);
			return 1;
		}
	}
	if (ferror(in)) {
		fclose(in);
		return 1;
	}
	if (fclose(in) != 0) {
		return 1;
	}
	if (fflush(stdout) != 0) {
		return 1;
	}
	return 0;
}
