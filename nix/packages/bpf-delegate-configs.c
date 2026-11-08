/* Two arguments: mode (code or doc) and a header path.
   Prints bpf delegate tables for bpf_cmd, bpf_map_type, bpf_prog_type,
   and bpf_attach_type. Any other argc exits 1 and prints usage.
*/
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct names {
	char **item;
	size_t len;
	size_t cap;
};

static int py_space(unsigned char c)
{
	return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\v' ||
		c == '\f' || (c >= 0x1c && c <= 0x1f);
}

static int word_char(unsigned char c)
{
	return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') ||
		(c >= '0' && c <= '9') || c == '_';
}

static char ascii_upper(char c)
{
	if (c >= 'a' && c <= 'z') {
		return (char)(c - 'a' + 'A');
	}
	return c;
}

static char ascii_lower(char c)
{
	if (c >= 'A' && c <= 'Z') {
		return (char)(c - 'A' + 'a');
	}
	return c;
}

static void print_usage(const char *argv0)
{
	printf("Usage: %s <code|doc> <header>\n", argv0 != NULL ? argv0 : "");
}

static void strip_line(char *s)
{
	char *start;
	size_t n;

	start = s;
	while (*start != '\0' && py_space((unsigned char)*start)) {
		start++;
	}
	if (start != s) {
		memmove(s, start, strlen(start) + 1);
	}
	n = strlen(s);
	while (n > 0 && py_space((unsigned char)s[n - 1])) {
		n--;
		s[n] = '\0';
	}
}

static char *read_line(FILE *fp, int *err)
{
	size_t cap;
	size_t n;
	char *buf;
	int c;

	*err = 0;
	cap = 128;
	n = 0;
	buf = malloc(cap);
	if (buf == NULL) {
		*err = 1;
		return NULL;
	}
	while ((c = fgetc(fp)) != EOF) {
		if (n + 1 >= cap) {
			size_t ncap;
			char *grown;

			if (cap > ((size_t)-1) / 2) {
				free(buf);
				*err = 1;
				return NULL;
			}
			ncap = cap * 2;
			grown = realloc(buf, ncap);
			if (grown == NULL) {
				free(buf);
				*err = 1;
				return NULL;
			}
			buf = grown;
			cap = ncap;
		}
		buf[n++] = (char)c;
		if (c == '\n') {
			break;
		}
	}
	if (c == EOF && ferror(fp)) {
		free(buf);
		*err = 1;
		return NULL;
	}
	if (n == 0) {
		free(buf);
		return NULL;
	}
	buf[n] = '\0';
	return buf;
}

static void names_init(struct names *list)
{
	list->item = NULL;
	list->len = 0;
	list->cap = 0;
}

static void names_clear(struct names *list)
{
	size_t i;

	for (i = 0; i < list->len; i++) {
		free(list->item[i]);
		list->item[i] = NULL;
	}
	list->len = 0;
}

static void names_free(struct names *list)
{
	names_clear(list);
	free(list->item);
	list->item = NULL;
	list->cap = 0;
}

static int names_push(struct names *list, char *value)
{
	if (list->len == list->cap) {
		size_t ncap;
		char **grown;

		if (list->cap == 0) {
			ncap = 32;
		} else if (list->cap > ((size_t)-1) / (2 * sizeof *grown)) {
			return -1;
		} else {
			ncap = list->cap * 2;
		}
		grown = realloc(list->item, ncap * sizeof *grown);
		if (grown == NULL) {
			return -1;
		}
		list->item = grown;
		list->cap = ncap;
	}
	list->item[list->len++] = value;
	return 0;
}

static int match_alt(const char *p, const char **alt, const char **end)
{
	static const char *alts[] = {
		"cmd",
		"map_type",
		"prog_type",
		"attach_type",
	};
	size_t i;

	for (i = 0; i < sizeof alts / sizeof alts[0]; i++) {
		size_t n;

		n = strlen(alts[i]);
		if (strncmp(p, alts[i], n) == 0) {
			*alt = alts[i];
			*end = p + n;
			return 1;
		}
	}
	return 0;
}

static int match_enum_start(const char *line, char *kind, size_t kindsz)
{
	const char *p;
	const char *alt;
	const char *end;
	const char *last;
	size_t n;

	p = line;
	if (strncmp(p, "enum", 4) != 0) {
		return 0;
	}
	p += 4;
	if (*p == '\0' || !py_space((unsigned char)*p)) {
		return 0;
	}
	while (py_space((unsigned char)*p)) {
		p++;
	}
	if (strncmp(p, "bpf_", 4) != 0) {
		return 0;
	}
	p += 4;
	if (!match_alt(p, &alt, &end)) {
		return 0;
	}
	last = alt;
	p = end;
	while (match_alt(p, &alt, &end)) {
		last = alt;
		p = end;
	}
	while (py_space((unsigned char)*p)) {
		p++;
	}
	if (*p != '{') {
		return 0;
	}
	n = strlen(last);
	if (n + 1 > kindsz) {
		return 0;
	}
	memcpy(kind, last, n + 1);
	return 1;
}

static int copy_enum_value(const char *line, char **out)
{
	size_t n;
	char *value;

	*out = NULL;
	n = 0;
	while (word_char((unsigned char)line[n])) {
		n++;
	}
	if (n == 0 || line[n] != ',' || line[n + 1] != '\0') {
		return 0;
	}
	if (n >= 2 && line[0] == '_' && line[1] == '_') {
		return 0;
	}
	value = malloc(n + 1);
	if (value == NULL) {
		return -1;
	}
	memcpy(value, line, n);
	value[n] = '\0';
	*out = value;
	return 1;
}

static char *transform_ident(const char *ident)
{
	size_t n;
	size_t i;
	size_t o;
	int first;
	char *out;

	n = strlen(ident);
	out = malloc(n + 1);
	if (out == NULL) {
		return NULL;
	}
	i = 0;
	o = 0;
	first = 1;
	for (;;) {
		size_t start;
		size_t len;
		size_t k;

		start = i;
		while (ident[i] != '\0' && ident[i] != '_') {
			i++;
		}
		len = i - start;
		if (first) {
			memcpy(out + o, ident + start, len);
			o += len;
			first = 0;
		} else if (len > 0) {
			out[o++] = ascii_upper(ident[start]);
			for (k = 1; k < len; k++) {
				out[o++] = ascii_lower(ident[start + k]);
			}
		}
		if (ident[i] == '\0') {
			break;
		}
		i++;
	}
	out[o] = '\0';
	return out;
}

static int emit_enum(int doc, const char *enum_name, struct names *values)
{
	size_t i;

	if (!doc) {
		if (printf("static const char* const %s_table[] = {\n", enum_name) < 0) {
			return -1;
		}
	} else if (printf("<para id=\"%s\">\n", enum_name) < 0) {
		return -1;
	}
	for (i = 0; i < values->len; i++) {
		char *shown;
		int nwr;

		shown = transform_ident(values->item[i]);
		if (shown == NULL) {
			return -1;
		}
		if (!doc) {
			nwr = printf("\t\"%s\",\n", shown);
		} else {
			nwr = printf("<constant>%s</constant>\n", shown);
		}
		free(shown);
		if (nwr < 0) {
			return -1;
		}
	}
	if (!doc) {
		if (printf("};\n") < 0) {
			return -1;
		}
	} else if (printf("</para>\n") < 0) {
		return -1;
	}
	return 0;
}

int main(int argc, char **argv)
{
	const char *mode;
	FILE *fp;
	int doc;
	int in_enum;
	char kind[32];
	char enum_name[64];
	struct names values;
	int rc;

	if (argc != 3) {
		print_usage(argc > 0 ? argv[0] : NULL);
		return 1;
	}
	mode = argv[1];
	if (strcmp(mode, "code") != 0 && strcmp(mode, "doc") != 0) {
		printf("Invalid format: <built-in function format>\n");
		print_usage(argv[0]);
		return 1;
	}
	doc = strcmp(mode, "doc") == 0;
	fp = fopen(argv[2], "r");
	if (fp == NULL) {
		return 1;
	}
	if (doc) {
		if (printf(
			    "<?xml version=\"1.0\"?>\n"
			    "<!DOCTYPE bpf-delegates PUBLIC \"-//OASIS//DTD DocBook XML V4.5//EN\"\n"
			    "  \"http://www.oasis-open.org/docbook/xml/4.5/docbookx.dtd\">\n"
			    "<para>\n"
			    "\n") < 0) {
			fclose(fp);
			return 1;
		}
	}
	names_init(&values);
	in_enum = 0;
	enum_name[0] = '\0';
	rc = 0;
	for (;;) {
		int err;
		char *line;
		char *value;
		int took;

		line = read_line(fp, &err);
		if (line == NULL) {
			if (err) {
				rc = 1;
			}
			break;
		}
		strip_line(line);
		if (in_enum) {
			if (line[0] == '}') {
				in_enum = 0;
				if (emit_enum(doc, enum_name, &values) != 0) {
					free(line);
					rc = 1;
					break;
				}
				names_clear(&values);
			} else {
				took = copy_enum_value(line, &value);
				if (took < 0) {
					free(line);
					rc = 1;
					break;
				}
				if (took > 0 && names_push(&values, value) != 0) {
					free(value);
					free(line);
					rc = 1;
					break;
				}
			}
		} else if (match_enum_start(line, kind, sizeof kind)) {
			int nw;

			in_enum = 1;
			nw = snprintf(enum_name, sizeof enum_name, "bpf_delegate_%s", kind);
			if (nw < 0 || (size_t)nw >= sizeof enum_name) {
				free(line);
				rc = 1;
				break;
			}
		}
		free(line);
	}
	names_free(&values);
	if (fclose(fp) != 0) {
		rc = 1;
	}
	if (rc == 0 && doc && printf("</para>\n") < 0) {
		rc = 1;
	}
	if (rc == 0 && fflush(stdout) != 0) {
		rc = 1;
	}
	return rc;
}
