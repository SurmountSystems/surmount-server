/* Writes the linker export list a compiler-rt generator expects. */
#include <stdio.h>
#include <string.h>

int main(int argc, char **argv)
{
	const char *out_path = NULL;
	int version_list = 0;
	int want_banner = 0;
	int i;
	FILE *out;

	for (i = 1; i < argc; i++) {
		if (strcmp(argv[i], "--version-list") == 0) {
			version_list = 1;
		} else if (strstr(argv[i], "version") != NULL) {
			want_banner = 1;
		}
		if (strcmp(argv[i], "-o") == 0 && i + 1 < argc) {
			out_path = argv[++i];
		}
	}
	if (want_banner) {
		fputs("Python 3.12.0\n", stdout);
	}
	if (out_path == NULL) {
		return 0;
	}
	out = fopen(out_path, "w");
	if (out == NULL) {
		return 1;
	}
	if (version_list) {
		fputs("{\nglobal:\n  *;\n};\n", out);
	} else {
		fputs("{\n  *;\n};\n", out);
	}
	if (fclose(out) != 0) {
		return 1;
	}
	return 0;
}
