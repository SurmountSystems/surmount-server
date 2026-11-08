/* argv is name, prefix, file, then optional include strings.
   stdout is the gperf text. Fewer than those three arguments exits 1.
*/
#include <stdio.h>

static size_t cstr_len(const char *s)
{
	size_t n = 0;

	while (s[n] != '\0') {
		n++;
	}
	return n;
}

static int is_trail_ws(char c)
{
	return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\v' ||
		c == '\f';
}

int main(int argc, char **argv)
{
	const char *name;
	const char *prefix;
	FILE *in;
	char buf[4096];
	int i;

	if (argc < 4) {
		fprintf(stderr, "Usage: %s name prefix file [includes...]\n",
			argc > 0 && argv[0] != NULL ? argv[0] :
						       "gperf-protocol-names");
		return 1;
	}

	name = argv[1];
	prefix = argv[2];

	if (fputs("%{\n"
		  "_Pragma(\"GCC diagnostic ignored \\\"-Wimplicit-fallthrough\\\"\")\n"
		  "#if __GNUC__ >= 15\n"
		  "_Pragma(\"GCC diagnostic ignored \\\"-Wzero-as-null-pointer-constant\\\"\")\n"
		  "#endif\n",
		  stdout) == EOF) {
		return 1;
	}

	for (i = 4; i < argc; i++) {
		if (printf("#include %s\n", argv[i]) < 0) {
			return 1;
		}
	}

	if (printf("%%}\n"
		   "struct %s_name { const char* name; int id; };\n"
		   "%%null-strings\n"
		   "%%%%\n",
		   name) < 0) {
		return 1;
	}

	in = fopen(argv[3], "r");
	if (in == NULL) {
		return 1;
	}

	while (fgets(buf, (int)sizeof buf, in) != NULL) {
		size_t n = cstr_len(buf);

		if (n == sizeof buf - 1 && buf[n - 1] != '\n' && !feof(in)) {
			fclose(in);
			return 1;
		}
		while (n > 0 && is_trail_ws(buf[n - 1])) {
			n--;
		}
		buf[n] = '\0';
		if (printf("%s, %s%s\n", buf, prefix, buf) < 0) {
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
	return 0;
}
