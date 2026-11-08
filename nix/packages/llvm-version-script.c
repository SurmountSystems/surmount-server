/* One argument is the output path. Any other argc exits 2 and writes nothing.
   The bytes are a linker version script binutils 2.46 ld.bfd accepts.
*/
#include <stdio.h>

int main(int argc, char **argv)
{
	static const char script[] =
		"{\n"
		"  global:\n"
		"    *;\n"
		"};\n";
	FILE *out;
	size_t n;
	size_t wrote;

	if (argc != 2) {
		return 2;
	}

	out = fopen(argv[1], "w");
	if (out == NULL) {
		return 1;
	}

	n = sizeof script - 1;
	wrote = fwrite(script, 1, n, out);
	if (wrote != n || ferror(out)) {
		fclose(out);
		return 1;
	}
	if (fclose(out) != 0) {
		return 1;
	}
	return 0;
}
