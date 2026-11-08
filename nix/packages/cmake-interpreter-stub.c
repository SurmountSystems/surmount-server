/* Answers CMake's FindPython interpreter probe.
   The binary stays cmake-interpreter-stub. CMake compares
   the major version from -c against 3, so a "Python 3.12.0"
   banner is the wrong answer and the probe fails closed.
*/
#include <stdio.h>
#include <string.h>

static int has(const char *s, const char *n)
{
	return s != NULL && n != NULL && strstr(s, n) != NULL;
}

int main(int argc, char **argv)
{
	int i;

	for (i = 1; i < argc; i++) {
		if (strcmp(argv[i], "--version") == 0 || strcmp(argv[i], "-V") == 0) {
			fputs("Python 3.12.0\n", stdout);
			return 0;
		}
		if (strcmp(argv[i], "-c") == 0 && i + 1 < argc) {
			const char *code = argv[i + 1];
			if (has(code, "version_info[:3]")) {
				fputs("3.12.0\n", stdout);
				return 0;
			}
			if (has(code, "version_info[0]")) {
				fputs("3\n", stdout);
				return 0;
			}
			if (has(code, "get_path") || has(code, "sysconfig")) {
				fputs("/build/pylib;/build/pylib;/build/pylib;/build/pylib\n", stdout);
				return 0;
			}
			return 0;
		}
	}
	return 0;
}
