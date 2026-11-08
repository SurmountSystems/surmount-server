/* argv is config.h, template, output. Any other argc exits 2.
   Substitutes {{ NAME }}, {{ 'a' if NAME else 'b' }}, {{ NAME + N }},
   and {{ MACRO('arg') }}. {% if NAME %} missing names are false.
   A missing {{ NAME }} exits 1. Block tags drop the following newline
   when the tag is the only text on its line. A minus on a tag strips
   adjacent whitespace.
*/
#include <ctype.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

#define OUT_MAX (8u * 1024u * 1024u)
#define STOP_ELSE 1
#define STOP_ENDIF 2

struct def {
	char *name;
	char *value;
};

struct macro {
	char *name;
	char *param;
	const char *body;
	size_t body_len;
};

struct env {
	struct def *defs;
	size_t ndefs;
	size_t def_cap;
	struct macro macros[8];
	size_t nmacros;
	char *out;
	size_t out_len;
	const char *param_name;
	const char *param_value;
};

struct tag {
	char kind;
	int open_minus;
	int close_minus;
	const char *body;
	size_t body_len;
	const char *after;
};

static int is_ws(unsigned char c)
{
	return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\v' ||
		c == '\f';
}

static int is_ident_start(unsigned char c)
{
	return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || c == '_';
}

static int is_ident(unsigned char c)
{
	return is_ident_start(c) || (c >= '0' && c <= '9');
}

static void die_oom(void)
{
	fprintf(stderr, "render-config-template: out of memory\n");
	exit(1);
}

static void *xmalloc(size_t n)
{
	void *p;

	p = malloc(n == 0 ? 1 : n);
	if (p == NULL) {
		die_oom();
	}
	return p;
}

static char *xdup_n(const char *s, size_t n)
{
	char *d;

	d = xmalloc(n + 1);
	memcpy(d, s, n);
	d[n] = '\0';
	return d;
}

static char *xdup(const char *s)
{
	return xdup_n(s, strlen(s));
}

static const char *skip_ws_ptr(const char *p, const char *end)
{
	while (p < end && is_ws((unsigned char)*p)) {
		p++;
	}
	return p;
}

static int emit_n(struct env *e, const char *s, size_t n)
{
	if (n > OUT_MAX - e->out_len) {
		fprintf(stderr, "render-config-template: output exceeded 8MB\n");
		return 1;
	}
	memcpy(e->out + e->out_len, s, n);
	e->out_len += n;
	return 0;
}

static int emit_str(struct env *e, const char *s)
{
	return emit_n(e, s, strlen(s));
}

static void lstrip_block(struct env *e)
{
	size_t i;

	i = e->out_len;
	while (i > 0 && (e->out[i - 1] == ' ' || e->out[i - 1] == '\t')) {
		i--;
	}
	if (i == 0 || e->out[i - 1] == '\n') {
		e->out_len = i;
	}
}

static void trim_out_ws(struct env *e)
{
	while (e->out_len > 0 && is_ws((unsigned char)e->out[e->out_len - 1])) {
		e->out_len--;
	}
}

static void close_ws(const char **pp, const char *end, int close_minus, int is_block)
{
	const char *p;
	const char *q;

	p = *pp;
	if (close_minus) {
		while (p < end && is_ws((unsigned char)*p)) {
			p++;
		}
	} else if (is_block) {
		q = p;
		while (q < end && (*q == ' ' || *q == '\t')) {
			q++;
		}
		if (q < end && *q == '\n') {
			p = q + 1;
		}
	}
	*pp = p;
}

static int read_file(const char *path, char **text, size_t *len)
{
	FILE *fp;
	long sz;
	char *buf;
	size_t n;

	fp = fopen(path, "r");
	if (fp == NULL) {
		fprintf(stderr, "render-config-template: open %s: %s\n", path,
			strerror(errno));
		return 1;
	}
	if (fseek(fp, 0, SEEK_END) != 0) {
		fclose(fp);
		return 1;
	}
	sz = ftell(fp);
	if (sz < 0) {
		fclose(fp);
		return 1;
	}
	if (fseek(fp, 0, SEEK_SET) != 0) {
		fclose(fp);
		return 1;
	}
	buf = xmalloc((size_t)sz + 1);
	n = fread(buf, 1, (size_t)sz, fp);
	if (n != (size_t)sz || ferror(fp)) {
		free(buf);
		fclose(fp);
		fprintf(stderr, "render-config-template: read %s failed\n", path);
		return 1;
	}
	buf[n] = '\0';
	fclose(fp);
	*text = buf;
	*len = n;
	return 0;
}

static char *normalize_value(const char *raw)
{
	int do_eval;
	char quote;
	size_t n;
	char *endptr;
	long long v;
	char buf[64];

	do_eval = 0;
	if (raw[0] >= '1' && raw[0] <= '9') {
		do_eval = 1;
	} else if (raw[0] == '"' || raw[0] == '\'') {
		do_eval = 1;
	} else if (strcmp(raw, "0") == 0) {
		do_eval = 1;
	}
	if (!do_eval) {
		return xdup(raw);
	}
	if (raw[0] == '"' || raw[0] == '\'') {
		quote = raw[0];
		n = strlen(raw);
		if (n < 2 || raw[n - 1] != quote) {
			fprintf(stderr, "render-config-template: bad quoted value\n");
			return NULL;
		}
		if (memchr(raw + 1, quote, n - 2) != NULL) {
			fprintf(stderr, "render-config-template: bad quoted value\n");
			return NULL;
		}
		return xdup_n(raw + 1, n - 2);
	}
	errno = 0;
	v = strtoll(raw, &endptr, 10);
	if (errno != 0 || endptr == raw || *endptr != '\0') {
		if (strchr(raw, '.') != NULL) {
			return xdup(raw);
		}
		fprintf(stderr, "render-config-template: bad numeric value\n");
		return NULL;
	}
	snprintf(buf, sizeof buf, "%lld", v);
	return xdup(buf);
}

static int add_def(struct env *e, const char *name, size_t name_len, char *value)
{
	size_t i;
	struct def *grown;

	for (i = 0; i < e->ndefs; i++) {
		if (strlen(e->defs[i].name) == name_len &&
		    memcmp(e->defs[i].name, name, name_len) == 0) {
			free(e->defs[i].value);
			e->defs[i].value = value;
			return 0;
		}
	}
	if (e->ndefs == e->def_cap) {
		e->def_cap = e->def_cap == 0 ? 256 : e->def_cap * 2;
		grown = realloc(e->defs, e->def_cap * sizeof *grown);
		if (grown == NULL) {
			die_oom();
		}
		e->defs = grown;
	}
	e->defs[e->ndefs].name = xdup_n(name, name_len);
	e->defs[e->ndefs].value = value;
	e->ndefs++;
	return 0;
}

static int load_config(struct env *e, const char *text, size_t len)
{
	size_t i;

	i = 0;
	while (i < len) {
		size_t line;
		size_t line_end;
		const char *s;
		size_t n;
		const char *name;
		size_t name_len;
		const char *val;
		size_t val_len;
		char *stored;

		line = i;
		while (i < len && text[i] != '\n') {
			i++;
		}
		line_end = i;
		if (i < len && text[i] == '\n') {
			i++;
		}
		if (line_end > line && text[line_end - 1] == '\r') {
			line_end--;
		}
		s = text + line;
		n = line_end - line;
		if (n < 8 || memcmp(s, "#define", 7) != 0 || !is_ws((unsigned char)s[7])) {
			continue;
		}
		s += 8;
		n -= 8;
		while (n > 0 && is_ws((unsigned char)*s)) {
			s++;
			n--;
		}
		if (n == 0 || !is_ident_start((unsigned char)*s)) {
			continue;
		}
		name = s;
		name_len = 0;
		while (name_len < n && is_ident((unsigned char)name[name_len])) {
			name_len++;
		}
		s += name_len;
		n -= name_len;
		if (n == 0 || !is_ws((unsigned char)*s)) {
			continue;
		}
		while (n > 0 && (s[0] == ' ' || s[0] == '\t')) {
			s++;
			n--;
		}
		while (n > 0 && is_ws((unsigned char)s[n - 1])) {
			n--;
		}
		val = s;
		val_len = n;
		{
			char *raw;

			raw = val_len == 0 ? xdup("") : xdup_n(val, val_len);
			stored = normalize_value(raw);
			free(raw);
		}
		if (stored == NULL) {
			return 1;
		}
		add_def(e, name, name_len, stored);
	}
	return 0;
}

static const char *lookup(struct env *e, const char *name, size_t name_len, int *found)
{
	size_t i;

	*found = 0;
	if (e->param_name != NULL && strlen(e->param_name) == name_len &&
	    memcmp(e->param_name, name, name_len) == 0) {
		*found = 1;
		return e->param_value;
	}
	for (i = 0; i < e->ndefs; i++) {
		if (strlen(e->defs[i].name) == name_len &&
		    memcmp(e->defs[i].name, name, name_len) == 0) {
			*found = 1;
			return e->defs[i].value;
		}
	}
	return NULL;
}

static int truth_value(const char *value)
{
	if (value == NULL || value[0] == '\0' || strcmp(value, "0") == 0) {
		return 0;
	}
	return 1;
}

static int parse_ident(const char **pp, const char *end, const char **name, size_t *name_len)
{
	const char *p;

	p = skip_ws_ptr(*pp, end);
	if (p >= end || !is_ident_start((unsigned char)*p)) {
		return 1;
	}
	*name = p;
	while (p < end && is_ident((unsigned char)*p)) {
		p++;
	}
	*name_len = (size_t)(p - *name);
	*pp = p;
	return 0;
}

static int parse_string(const char **pp, const char *end, char **out)
{
	const char *p;
	char quote;
	const char *start;

	p = skip_ws_ptr(*pp, end);
	if (p >= end || (*p != '\'' && *p != '"')) {
		return 1;
	}
	quote = *p;
	p++;
	start = p;
	while (p < end && *p != quote) {
		p++;
	}
	if (p >= end) {
		return 1;
	}
	*out = xdup_n(start, (size_t)(p - start));
	*pp = p + 1;
	return 0;
}

static int parse_tag(const char *p, const char *end, struct tag *t)
{
	char kind;
	const char *body;
	const char *i;
	char quote;

	if (p + 1 >= end || p[0] != '{') {
		return 1;
	}
	if (p[1] != '{' && p[1] != '%' && p[1] != '#') {
		return 1;
	}
	kind = p[1];
	body = p + 2;
	t->open_minus = 0;
	if (body < end && *body == '-') {
		t->open_minus = 1;
		body++;
	}
	quote = 0;
	i = body;
	while (i < end) {
		if (quote) {
			if (*i == quote) {
				quote = 0;
			}
			i++;
			continue;
		}
		if (*i == '\'' || *i == '"') {
			quote = *i;
			i++;
			continue;
		}
		if (kind == '{' && i + 1 < end && i[0] == '}' && i[1] == '}') {
			break;
		}
		if (kind == '%' && i + 1 < end && i[0] == '%' && i[1] == '}') {
			break;
		}
		if (kind == '#' && i + 1 < end && i[0] == '#' && i[1] == '}') {
			break;
		}
		i++;
	}
	if (i >= end) {
		fprintf(stderr, "render-config-template: unclosed tag\n");
		return 2;
	}
	t->kind = kind;
	t->close_minus = 0;
	t->body = body;
	t->body_len = (size_t)(i - body);
	if (t->body_len > 0 && body[t->body_len - 1] == '-') {
		t->close_minus = 1;
		t->body_len--;
	}
	t->after = i + 2;
	return 0;
}

static int find_next_tag(const char *p, const char *end, const char **text_end, struct tag *t)
{
	const char *s;

	s = p;
	while (s < end) {
		int rc;

		if (*s != '{') {
			s++;
			continue;
		}
		rc = parse_tag(s, end, t);
		if (rc == 1) {
			s++;
			continue;
		}
		if (rc != 0) {
			return rc;
		}
		*text_end = s;
		return 0;
	}
	*text_end = end;
	return 0;
}

static struct macro *find_macro(struct env *e, const char *name, size_t name_len)
{
	size_t i;

	for (i = 0; i < e->nmacros; i++) {
		if (strlen(e->macros[i].name) == name_len &&
		    memcmp(e->macros[i].name, name, name_len) == 0) {
			return &e->macros[i];
		}
	}
	return NULL;
}

static int block_keyword(const char *body, size_t len, const char **rest)
{
	const char *p;
	const char *end;
	const char *word;

	p = body;
	end = body + len;
	p = skip_ws_ptr(p, end);
	word = p;
	while (p < end && is_ident((unsigned char)*p)) {
		p++;
	}
	*rest = p;
	if ((size_t)(p - word) == 2 && memcmp(word, "if", 2) == 0) {
		return 1;
	}
	if ((size_t)(p - word) == 4 && memcmp(word, "else", 4) == 0) {
		return 2;
	}
	if ((size_t)(p - word) == 5 && memcmp(word, "endif", 5) == 0) {
		return 3;
	}
	if ((size_t)(p - word) == 5 && memcmp(word, "macro", 5) == 0) {
		return 4;
	}
	if ((size_t)(p - word) == 8 && memcmp(word, "endmacro", 8) == 0) {
		return 5;
	}
	return 0;
}

static int integer_text(const char *s, long long *out)
{
	char *endptr;
	long long v;

	errno = 0;
	v = strtoll(s, &endptr, 10);
	if (errno != 0 || endptr == s || *endptr != '\0') {
		return 1;
	}
	*out = v;
	return 0;
}

static int render_range(struct env *e, const char *begin, const char *end, int stop_mask,
			 int *why, const char **stopped);
static int skip_range(struct env *e, const char *begin, const char *end, int stop_mask,
		       int *why, const char **stopped);

static int expand_macro(struct env *e, struct macro *m, const char *arg)
{
	const char *saved_name;
	const char *saved_value;
	int rc;
	int why;
	const char *stopped;

	saved_name = e->param_name;
	saved_value = e->param_value;
	e->param_name = m->param;
	e->param_value = arg;
	why = 0;
	stopped = m->body + m->body_len;
	rc = render_range(e, m->body, m->body + m->body_len, 0, &why, &stopped);
	e->param_name = saved_name;
	e->param_value = saved_value;
	return rc;
}

static int eval_expr(struct env *e, const char *body, size_t len)
{
	const char *p;
	const char *end;
	const char *name;
	size_t name_len;

	p = body;
	end = body + len;
	p = skip_ws_ptr(p, end);
	if (p < end && (*p == '\'' || *p == '"')) {
		char *a;
		char *b;
		const char *cond_name;
		size_t cond_len;
		int found;
		const char *cond;
		const char *after;

		if (parse_string(&p, end, &a) != 0) {
			fprintf(stderr, "render-config-template: bad string\n");
			return 1;
		}
		after = skip_ws_ptr(p, end);
		if (after + 2 <= end && memcmp(after, "if", 2) == 0 &&
		    (after + 2 == end || !is_ident((unsigned char)after[2]))) {
			p = after + 2;
			if (parse_ident(&p, end, &cond_name, &cond_len) != 0) {
				free(a);
				fprintf(stderr, "render-config-template: bad ternary\n");
				return 1;
			}
			p = skip_ws_ptr(p, end);
			if (p + 4 > end || memcmp(p, "else", 4) != 0) {
				free(a);
				fprintf(stderr, "render-config-template: ternary missing else\n");
				return 1;
			}
			p += 4;
			if (parse_string(&p, end, &b) != 0) {
				free(a);
				fprintf(stderr, "render-config-template: bad ternary string\n");
				return 1;
			}
			p = skip_ws_ptr(p, end);
			if (p != end) {
				free(a);
				free(b);
				fprintf(stderr, "render-config-template: trailing ternary text\n");
				return 1;
			}
			cond = lookup(e, cond_name, cond_len, &found);
			if (!found) {
				free(a);
				free(b);
				fprintf(stderr, "render-config-template: undefined name %.*s\n",
					(int)cond_len, cond_name);
				return 1;
			}
			if (truth_value(cond)) {
				if (emit_str(e, a) != 0) {
					free(a);
					free(b);
					return 1;
				}
			} else if (emit_str(e, b) != 0) {
				free(a);
				free(b);
				return 1;
			}
			free(a);
			free(b);
			return 0;
		}
		p = skip_ws_ptr(p, end);
		if (p != end) {
			free(a);
			fprintf(stderr, "render-config-template: trailing expression\n");
			return 1;
		}
		if (emit_str(e, a) != 0) {
			free(a);
			return 1;
		}
		free(a);
		return 0;
	}
	if (parse_ident(&p, end, &name, &name_len) != 0) {
		fprintf(stderr, "render-config-template: bad expression\n");
		return 1;
	}
	p = skip_ws_ptr(p, end);
	if (p < end && *p == '(') {
		struct macro *m;
		char *arg;

		p++;
		if (parse_string(&p, end, &arg) != 0) {
			fprintf(stderr, "render-config-template: bad call\n");
			return 1;
		}
		p = skip_ws_ptr(p, end);
		if (p >= end || *p != ')') {
			free(arg);
			fprintf(stderr, "render-config-template: bad call\n");
			return 1;
		}
		p = skip_ws_ptr(p + 1, end);
		if (p != end) {
			free(arg);
			fprintf(stderr, "render-config-template: trailing call\n");
			return 1;
		}
		m = find_macro(e, name, name_len);
		if (m == NULL) {
			fprintf(stderr, "render-config-template: unknown macro %.*s\n",
				(int)name_len, name);
			free(arg);
			return 1;
		}
		if (expand_macro(e, m, arg) != 0) {
			free(arg);
			return 1;
		}
		free(arg);
		return 0;
	}
	if (p < end && *p == '+') {
		const char *right_name;
		size_t right_len;
		int found;
		const char *left;
		long long lv;
		long long rv;
		char num[64];

		left = lookup(e, name, name_len, &found);
		if (!found || left == NULL || integer_text(left, &lv) != 0) {
			fprintf(stderr, "render-config-template: bad addition\n");
			return 1;
		}
		p++;
		p = skip_ws_ptr(p, end);
		if (p < end && is_ident_start((unsigned char)*p)) {
			if (parse_ident(&p, end, &right_name, &right_len) != 0) {
				return 1;
			}
			left = lookup(e, right_name, right_len, &found);
			if (!found || left == NULL || integer_text(left, &rv) != 0) {
				fprintf(stderr, "render-config-template: bad addition\n");
				return 1;
			}
		} else {
			char *endptr;

			errno = 0;
			rv = strtoll(p, &endptr, 10);
			if (errno != 0 || endptr == p) {
				fprintf(stderr, "render-config-template: bad addition\n");
				return 1;
			}
			p = endptr;
		}
		p = skip_ws_ptr(p, end);
		if (p != end) {
			fprintf(stderr, "render-config-template: trailing addition\n");
			return 1;
		}
		snprintf(num, sizeof num, "%lld", lv + rv);
		return emit_str(e, num);
	}
	if (p != end) {
		fprintf(stderr, "render-config-template: trailing expression\n");
		return 1;
	}
	{
		int found;
		const char *value;

		value = lookup(e, name, name_len, &found);
		if (!found || value == NULL) {
			fprintf(stderr, "render-config-template: undefined name %.*s\n",
				(int)name_len, name);
			return 1;
		}
		return emit_str(e, value);
	}
}

static int apply_open(struct env *e, const struct tag *t)
{
	if (t->kind == '%') {
		lstrip_block(e);
	}
	if (t->open_minus) {
		trim_out_ws(e);
	}
	return 0;
}

static int handle_if(struct env *e, int truth, const char **pp, const char *end)
{
	int why;
	const char *p;

	why = 0;
	p = *pp;
	if (truth) {
		if (render_range(e, p, end, STOP_ELSE | STOP_ENDIF, &why, &p) != 0) {
			return 1;
		}
		if (why == STOP_ELSE) {
			if (skip_range(e, p, end, STOP_ENDIF, &why, &p) != 0) {
				return 1;
			}
		}
	} else {
		if (skip_range(e, p, end, STOP_ELSE | STOP_ENDIF, &why, &p) != 0) {
			return 1;
		}
		if (why == STOP_ELSE) {
			if (render_range(e, p, end, STOP_ENDIF, &why, &p) != 0) {
				return 1;
			}
		}
	}
	if (why != STOP_ENDIF && why != STOP_ELSE) {
		fprintf(stderr, "render-config-template: unclosed if\n");
		return 1;
	}
	if (why == STOP_ELSE) {
		fprintf(stderr, "render-config-template: unclosed if\n");
		return 1;
	}
	*pp = p;
	return 0;
}

static int define_macro(struct env *e, const struct tag *open, const char **pp, const char *end)
{
	const char *rest;
	const char *name;
	size_t name_len;
	const char *param;
	size_t param_len;
	const char *p;
	const char *body;
	const char *body_end;
	struct tag t;
	int kw;
	struct macro *slot;

	rest = open->body;
	if (block_keyword(open->body, open->body_len, &rest) != 4) {
		return 1;
	}
	if (parse_ident(&rest, open->body + open->body_len, &name, &name_len) != 0) {
		fprintf(stderr, "render-config-template: bad macro name\n");
		return 1;
	}
	rest = skip_ws_ptr(rest, open->body + open->body_len);
	if (rest >= open->body + open->body_len || *rest != '(') {
		fprintf(stderr, "render-config-template: macro missing parameter\n");
		return 1;
	}
	rest++;
	if (parse_ident(&rest, open->body + open->body_len, &param, &param_len) != 0) {
		fprintf(stderr, "render-config-template: bad macro parameter\n");
		return 1;
	}
	rest = skip_ws_ptr(rest, open->body + open->body_len);
	if (rest >= open->body + open->body_len || *rest != ')') {
		fprintf(stderr, "render-config-template: bad macro parameter\n");
		return 1;
	}
	p = open->after;
	close_ws(&p, end, open->close_minus, 1);
	body = p;
	body_end = NULL;
	while (p < end) {
		const char *text_end;
		int rc;

		rc = find_next_tag(p, end, &text_end, &t);
		if (rc != 0) {
			return 1;
		}
		if (text_end == end) {
			fprintf(stderr, "render-config-template: unclosed macro\n");
			return 1;
		}
		if (t.kind == '%') {
			kw = block_keyword(t.body, t.body_len, &rest);
			if (kw == 5) {
				body_end = text_end;
				if (t.open_minus) {
					while (body_end > body &&
					       is_ws((unsigned char)body_end[-1])) {
						body_end--;
					}
				}
				p = t.after;
				close_ws(&p, end, t.close_minus, 1);
				break;
			}
			if (kw == 4) {
				fprintf(stderr, "render-config-template: nested macro\n");
				return 1;
			}
		}
		p = t.after;
	}
	if (body_end == NULL) {
		fprintf(stderr, "render-config-template: unclosed macro\n");
		return 1;
	}
	slot = find_macro(e, name, name_len);
	if (slot == NULL) {
		if (e->nmacros == 8) {
			fprintf(stderr, "render-config-template: too many macros\n");
			return 1;
		}
		slot = &e->macros[e->nmacros];
		e->nmacros++;
		slot->name = xdup_n(name, name_len);
	}
	free(slot->param);
	slot->param = xdup_n(param, param_len);
	slot->body = body;
	slot->body_len = (size_t)(body_end - body);
	*pp = p;
	return 0;
}

static int render_range(struct env *e, const char *begin, const char *end, int stop_mask,
			 int *why, const char **stopped)
{
	const char *p;

	p = begin;
	*why = 0;
	*stopped = end;
	while (p < end) {
		const char *text_end;
		struct tag t;
		int rc;
		const char *rest;
		int kw;
		const char *after;

		rc = find_next_tag(p, end, &text_end, &t);
		if (rc != 0) {
			return 1;
		}
		if (text_end > p) {
			if (emit_n(e, p, (size_t)(text_end - p)) != 0) {
				return 1;
			}
		}
		if (text_end == end) {
			*stopped = end;
			return 0;
		}
		if (t.kind == '#') {
			if (apply_open(e, &t) != 0) {
				return 1;
			}
			p = t.after;
			close_ws(&p, end, t.close_minus, 0);
			continue;
		}
		if (t.kind == '{') {
			if (apply_open(e, &t) != 0) {
				return 1;
			}
			if (eval_expr(e, t.body, t.body_len) != 0) {
				return 1;
			}
			p = t.after;
			close_ws(&p, end, t.close_minus, 0);
			continue;
		}
		kw = block_keyword(t.body, t.body_len, &rest);
		if (kw == 2) {
			if ((stop_mask & STOP_ELSE) == 0) {
				fprintf(stderr, "render-config-template: else without if\n");
				return 1;
			}
			if (apply_open(e, &t) != 0) {
				return 1;
			}
			p = t.after;
			close_ws(&p, end, t.close_minus, 1);
			*why = STOP_ELSE;
			*stopped = p;
			return 0;
		}
		if (kw == 3) {
			if ((stop_mask & STOP_ENDIF) == 0) {
				fprintf(stderr, "render-config-template: endif without if\n");
				return 1;
			}
			if (apply_open(e, &t) != 0) {
				return 1;
			}
			p = t.after;
			close_ws(&p, end, t.close_minus, 1);
			*why = STOP_ENDIF;
			*stopped = p;
			return 0;
		}
		if (kw == 5) {
			fprintf(stderr, "render-config-template: endmacro without macro\n");
			return 1;
		}
		if (apply_open(e, &t) != 0) {
			return 1;
		}
		if (kw == 1) {
			const char *cond_name;
			size_t cond_len;
			int found;
			const char *cond;
			const char *ip;

			ip = rest;
			if (parse_ident(&ip, t.body + t.body_len, &cond_name, &cond_len) != 0) {
				fprintf(stderr, "render-config-template: bad if\n");
				return 1;
			}
			cond = lookup(e, cond_name, cond_len, &found);
			after = t.after;
			close_ws(&after, end, t.close_minus, 1);
			if (handle_if(e, found && truth_value(cond), &after, end) != 0) {
				return 1;
			}
			p = after;
			continue;
		}
		if (kw == 4) {
			after = p;
			if (define_macro(e, &t, &after, end) != 0) {
				return 1;
			}
			p = after;
			continue;
		}
		fprintf(stderr, "render-config-template: unsupported tag\n");
		return 1;
	}
	*stopped = end;
	return 0;
}

static int skip_range(struct env *e, const char *begin, const char *end, int stop_mask,
		       int *why, const char **stopped)
{
	const char *p;
	int depth;

	(void)e;
	p = begin;
	depth = 0;
	*why = 0;
	while (p < end) {
		const char *text_end;
		struct tag t;
		int rc;
		const char *rest;
		int kw;

		rc = find_next_tag(p, end, &text_end, &t);
		if (rc != 0) {
			return 1;
		}
		if (text_end == end) {
			fprintf(stderr, "render-config-template: unclosed if\n");
			return 1;
		}
		if (t.kind != '%') {
			p = t.after;
			continue;
		}
		kw = block_keyword(t.body, t.body_len, &rest);
		if (kw == 1) {
			depth++;
			p = t.after;
			continue;
		}
		if (kw == 2 && depth == 0 && (stop_mask & STOP_ELSE) != 0) {
			p = t.after;
			close_ws(&p, end, t.close_minus, 1);
			*why = STOP_ELSE;
			*stopped = p;
			return 0;
		}
		if (kw == 3) {
			if (depth == 0) {
				p = t.after;
				close_ws(&p, end, t.close_minus, 1);
				*why = STOP_ENDIF;
				*stopped = p;
				return 0;
			}
			depth--;
			p = t.after;
			continue;
		}
		p = t.after;
	}
	fprintf(stderr, "render-config-template: unclosed if\n");
	return 1;
}

static void free_env(struct env *e)
{
	size_t i;

	for (i = 0; i < e->ndefs; i++) {
		free(e->defs[i].name);
		free(e->defs[i].value);
	}
	free(e->defs);
	for (i = 0; i < e->nmacros; i++) {
		free(e->macros[i].name);
		free(e->macros[i].param);
	}
	free(e->out);
}

int main(int argc, char **argv)
{
	struct env e;
	char *config;
	size_t config_len;
	char *tmpl;
	size_t tmpl_len;
	int why;
	const char *stopped;
	FILE *out;
	struct stat st;

	if (argc != 4) {
		fprintf(stderr, "usage: render-config-template config.h template output\n");
		return 2;
	}
	memset(&e, 0, sizeof e);
	e.out = xmalloc(OUT_MAX);
	if (read_file(argv[1], &config, &config_len) != 0) {
		free_env(&e);
		return 1;
	}
	if (load_config(&e, config, config_len) != 0) {
		free(config);
		free_env(&e);
		return 1;
	}
	free(config);
	if (read_file(argv[2], &tmpl, &tmpl_len) != 0) {
		free_env(&e);
		return 1;
	}
	if (stat(argv[2], &st) != 0) {
		fprintf(stderr, "render-config-template: stat %s: %s\n", argv[2],
			strerror(errno));
		free(tmpl);
		free_env(&e);
		return 1;
	}
	why = 0;
	stopped = NULL;
	if (render_range(&e, tmpl, tmpl + tmpl_len, 0, &why, &stopped) != 0) {
		free(tmpl);
		free_env(&e);
		return 1;
	}
	out = fopen(argv[3], "w");
	if (out == NULL) {
		fprintf(stderr, "render-config-template: open %s: %s\n", argv[3],
			strerror(errno));
		free(tmpl);
		free_env(&e);
		return 1;
	}
	if (e.out_len > 0 && fwrite(e.out, 1, e.out_len, out) != e.out_len) {
		fclose(out);
		free(tmpl);
		free_env(&e);
		return 1;
	}
	if (fclose(out) != 0) {
		free(tmpl);
		free_env(&e);
		return 1;
	}
	if (chmod(argv[3], st.st_mode & 07777) != 0) {
		fprintf(stderr, "render-config-template: chmod %s: %s\n", argv[3],
			strerror(errno));
		free(tmpl);
		free_env(&e);
		return 1;
	}
	free(tmpl);
	free_env(&e);
	return 0;
}
