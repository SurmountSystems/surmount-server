/* ISO C11 port of libpsl psl-make-dafsa. cxx+ output feeds suffixes_dafsa.h. */
#define _DEFAULT_SOURCE
#define _POSIX_C_SOURCE 200809L

#include <errno.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

static void die(const char *msg)
{
	fprintf(stderr, "psl-make-dafsa-c: %s\n", msg);
	exit(1);
}

static void die_errno(const char *msg)
{
	fprintf(stderr, "psl-make-dafsa-c: %s: %s\n", msg, strerror(errno));
	exit(1);
}

static void die_fmt(const char *fmt, ...)
{
	va_list ap;

	fputs("psl-make-dafsa-c: ", stderr);
	va_start(ap, fmt);
	vfprintf(stderr, fmt, ap);
	va_end(ap);
	fputc('\n', stderr);
	exit(1);
}

static void *xmalloc(size_t n)
{
	void *p;

	if (n == 0)
		n = 1;
	p = malloc(n);
	if (p == NULL)
		die("out of memory");
	return p;
}

static void *xrealloc(void *p, size_t n)
{
	void *q;

	if (n == 0)
		n = 1;
	q = realloc(p, n);
	if (q == NULL)
		die("out of memory");
	return q;
}

typedef struct {
	uint8_t *p;
	size_t n;
} bstr;

typedef struct {
	bstr *v;
	size_t n, cap;
} bvec;

static void bvec_push(bvec *b, bstr s)
{
	if (b->n == b->cap) {
		b->cap = b->cap ? b->cap * 2 : 8;
		b->v = xrealloc(b->v, b->cap * sizeof *b->v);
	}
	b->v[b->n++] = s;
}

static bstr bstr_cat(const uint8_t *a, size_t an, const uint8_t *b, size_t bn)
{
	bstr s;

	s.n = an + bn;
	s.p = xmalloc(s.n ? s.n : 1);
	if (an)
		memcpy(s.p, a, an);
	if (bn)
		memcpy(s.p + an, b, bn);
	return s;
}

static int bstr_cmp(const bstr *a, const bstr *b)
{
	size_t n = a->n < b->n ? a->n : b->n;
	int c = 0;

	if (n)
		c = memcmp(a->p, b->p, n);
	if (c != 0)
		return c;
	if (a->n < b->n)
		return -1;
	if (a->n > b->n)
		return 1;
	return 0;
}

static int bstr_cmp_q(const void *a, const void *b)
{
	return bstr_cmp(a, b);
}

static void bvec_unique_sorted(bvec *b)
{
	size_t w = 0;
	size_t i;

	qsort(b->v, b->n, sizeof *b->v, bstr_cmp_q);
	for (i = 0; i < b->n; i++) {
		if (w > 0 && bstr_cmp(&b->v[w - 1], &b->v[i]) == 0)
			continue;
		b->v[w++] = b->v[i];
	}
	b->n = w;
}

typedef struct {
	uint8_t *p;
	size_t n, cap;
} uvec;

static void uvec_push(uvec *u, uint8_t c)
{
	if (u->n == u->cap) {
		u->cap = u->cap ? u->cap * 2 : 16;
		u->p = xrealloc(u->p, u->cap);
	}
	u->p[u->n++] = c;
}

static void uvec_extend(uvec *u, const uint8_t *p, size_t n)
{
	size_t i;

	for (i = 0; i < n; i++)
		uvec_push(u, p[i]);
}

static void uvec_reverse(uvec *u)
{
	size_t i;

	for (i = 0; i < u->n / 2; i++) {
		uint8_t t = u->p[i];

		u->p[i] = u->p[u->n - 1 - i];
		u->p[u->n - 1 - i] = t;
	}
}

typedef struct node node;
struct node {
	uint8_t *lab;
	size_t lab_len;
	node **ch;
	size_t nch, cap;
};

typedef struct {
	node **v;
	size_t n, cap;
} nvec;

static void nvec_push(nvec *a, node *n)
{
	if (a->n == a->cap) {
		a->cap = a->cap ? a->cap * 2 : 8;
		a->v = xrealloc(a->v, a->cap * sizeof *a->v);
	}
	a->v[a->n++] = n;
}

static node *node_new(const uint8_t *lab, size_t n)
{
	node *nd = xmalloc(sizeof *nd);

	nd->lab = xmalloc(n ? n : 1);
	if (n)
		memcpy(nd->lab, lab, n);
	nd->lab_len = n;
	nd->ch = NULL;
	nd->nch = 0;
	nd->cap = 0;
	return nd;
}

static void node_add(node *nd, node *child)
{
	if (nd->nch == nd->cap) {
		nd->cap = nd->cap ? nd->cap * 2 : 4;
		nd->ch = xrealloc(nd->ch, nd->cap * sizeof *nd->ch);
	}
	nd->ch[nd->nch++] = child;
}

typedef struct {
	const void *key;
	void *val;
	unsigned state;
} pslot;

typedef struct {
	pslot *s;
	size_t cap, n;
} pmap;

static uint64_t hash_ptr(const void *p)
{
	uint64_t x = (uint64_t)(uintptr_t)p;

	x ^= x >> 30;
	x *= 0xbf58476d1ce4e5b9ull;
	x ^= x >> 27;
	x *= 0x94d049bb133111ebull;
	x ^= x >> 31;
	return x ? x : 1;
}

static void pmap_grow(pmap *m)
{
	size_t nc = m->cap ? m->cap * 2 : 16;
	pslot *ns = xmalloc(nc * sizeof *ns);
	size_t i;

	memset(ns, 0, nc * sizeof *ns);
	for (i = 0; i < m->cap; i++) {
		size_t j, mask;

		if (!m->s[i].state)
			continue;
		mask = nc - 1;
		j = (size_t)hash_ptr(m->s[i].key) & mask;
		while (ns[j].state)
			j = (j + 1) & mask;
		ns[j] = m->s[i];
	}
	free(m->s);
	m->s = ns;
	m->cap = nc;
}

static void **pmap_slot(pmap *m, const void *key, int create)
{
	size_t mask, j, steps;

	if (m->cap == 0 || (create && m->n * 10 >= m->cap * 7))
		pmap_grow(m);
	mask = m->cap - 1;
	j = (size_t)hash_ptr(key) & mask;
	for (steps = 0; steps <= m->cap; steps++) {
		if (!m->s[j].state) {
			if (!create)
				return NULL;
			m->s[j].state = 1;
			m->s[j].key = key;
			m->s[j].val = NULL;
			m->n++;
			return &m->s[j].val;
		}
		if (m->s[j].key == key)
			return &m->s[j].val;
		j = (j + 1) & mask;
	}
	die("pointer map full");
	return NULL;
}

static int pmap_get(pmap *m, const void *key, void **val)
{
	void **slot;

	if (m->cap == 0)
		return 0;
	slot = pmap_slot(m, key, 0);
	if (slot == NULL)
		return 0;
	*val = *slot;
	return 1;
}

static void pmap_set(pmap *m, const void *key, void *val)
{
	*pmap_slot(m, key, 1) = val;
}

typedef struct {
	uint8_t *k;
	size_t kn;
	void *val;
	unsigned state;
} bslot;

typedef struct {
	bslot *s;
	size_t cap, n;
} bmap;

static uint64_t hash_bytes(const uint8_t *p, size_t n)
{
	uint64_t h = 14695981039346656037ull;
	size_t i;

	for (i = 0; i < n; i++) {
		h ^= p[i];
		h *= 1099511628211ull;
	}
	return h ? h : 1;
}

static int bytes_eq(const uint8_t *a, size_t an, const uint8_t *b, size_t bn)
{
	if (an != bn)
		return 0;
	return an == 0 || memcmp(a, b, an) == 0;
}

static void bmap_grow(bmap *m)
{
	size_t nc = m->cap ? m->cap * 2 : 16;
	bslot *ns = xmalloc(nc * sizeof *ns);
	size_t i;

	memset(ns, 0, nc * sizeof *ns);
	for (i = 0; i < m->cap; i++) {
		size_t j, mask;

		if (!m->s[i].state)
			continue;
		mask = nc - 1;
		j = (size_t)hash_bytes(m->s[i].k, m->s[i].kn) & mask;
		while (ns[j].state)
			j = (j + 1) & mask;
		ns[j] = m->s[i];
	}
	free(m->s);
	m->s = ns;
	m->cap = nc;
}

static bslot *bmap_slot(bmap *m, const uint8_t *k, size_t kn, int create)
{
	size_t mask, j, steps;

	if (m->cap == 0 || (create && m->n * 10 >= m->cap * 7))
		bmap_grow(m);
	mask = m->cap - 1;
	j = (size_t)hash_bytes(k, kn) & mask;
	for (steps = 0; steps <= m->cap; steps++) {
		if (!m->s[j].state) {
			uint8_t *copy;

			if (!create)
				return NULL;
			copy = xmalloc(kn ? kn : 1);
			if (kn)
				memcpy(copy, k, kn);
			m->s[j].state = 1;
			m->s[j].k = copy;
			m->s[j].kn = kn;
			m->s[j].val = NULL;
			m->n++;
			return &m->s[j];
		}
		if (bytes_eq(m->s[j].k, m->s[j].kn, k, kn))
			return &m->s[j];
		j = (j + 1) & mask;
	}
	die("byte map full");
	return NULL;
}

static void *bmap_get(bmap *m, const uint8_t *k, size_t kn)
{
	bslot *s;

	if (m->cap == 0)
		return NULL;
	s = bmap_slot(m, k, kn, 0);
	if (s == NULL)
		return NULL;
	return s->val;
}

static void bmap_put(bmap *m, const uint8_t *k, size_t kn, void *val)
{
	bmap_slot(m, k, kn, 1)->val = val;
}

static uint32_t rol32(uint32_t v, int n)
{
	return (v << n) | (v >> (32 - n));
}

static void sha1_block(uint32_t h[5], const uint8_t blk[64])
{
	uint32_t w[80];
	uint32_t a, b, c, d, e;
	int i;

	for (i = 0; i < 16; i++) {
		w[i] = ((uint32_t)blk[i * 4] << 24) | ((uint32_t)blk[i * 4 + 1] << 16) |
		       ((uint32_t)blk[i * 4 + 2] << 8) | (uint32_t)blk[i * 4 + 3];
	}
	for (i = 16; i < 80; i++)
		w[i] = rol32(w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16], 1);
	a = h[0];
	b = h[1];
	c = h[2];
	d = h[3];
	e = h[4];
	for (i = 0; i < 80; i++) {
		uint32_t f, k, t;

		if (i < 20) {
			f = (b & c) | ((~b) & d);
			k = 0x5A827999u;
		} else if (i < 40) {
			f = b ^ c ^ d;
			k = 0x6ED9EBA1u;
		} else if (i < 60) {
			f = (b & c) | (b & d) | (c & d);
			k = 0x8F1BBCDCu;
		} else {
			f = b ^ c ^ d;
			k = 0xCA62C1D6u;
		}
		t = rol32(a, 5) + f + e + k + w[i];
		e = d;
		d = c;
		c = rol32(b, 30);
		b = a;
		a = t;
	}
	h[0] += a;
	h[1] += b;
	h[2] += c;
	h[3] += d;
	h[4] += e;
}

static void sha1_file(const char *path, char out[41])
{
	FILE *f = fopen(path, "rb");
	uint32_t h[5] = {0x67452301u, 0xEFCDAB89u, 0x98BADCFEu, 0x10325476u, 0xC3D2E1F0u};
	uint8_t blk[64];
	uint64_t nbits = 0;
	size_t nbuf = 0;
	int eof = 0;

	if (f == NULL)
		die_errno(path);
	while (!eof) {
		size_t got = fread(blk + nbuf, 1, 64 - nbuf, f);

		if (got < 64 - nbuf) {
			if (ferror(f))
				die_errno(path);
			eof = 1;
		}
		nbuf += got;
		nbits += (uint64_t)got * 8;
		if (nbuf == 64) {
			sha1_block(h, blk);
			nbuf = 0;
		} else if (eof) {
			break;
		}
	}
	fclose(f);
	blk[nbuf++] = 0x80;
	if (nbuf > 56) {
		while (nbuf < 64)
			blk[nbuf++] = 0;
		sha1_block(h, blk);
		nbuf = 0;
	}
	while (nbuf < 56)
		blk[nbuf++] = 0;
	blk[56] = (uint8_t)(nbits >> 56);
	blk[57] = (uint8_t)(nbits >> 48);
	blk[58] = (uint8_t)(nbits >> 40);
	blk[59] = (uint8_t)(nbits >> 32);
	blk[60] = (uint8_t)(nbits >> 24);
	blk[61] = (uint8_t)(nbits >> 16);
	blk[62] = (uint8_t)(nbits >> 8);
	blk[63] = (uint8_t)nbits;
	sha1_block(h, blk);
	snprintf(out, 41, "%08x%08x%08x%08x%08x", h[0], h[1], h[2], h[3], h[4]);
}

static char puny_digit(uint32_t d)
{
	if (d < 26)
		return (char)('a' + d);
	if (d < 36)
		return (char)('0' + (d - 26));
	die("punycode digit");
	return 0;
}

static uint32_t puny_adapt(uint32_t delta, uint32_t numpoints, int first)
{
	uint32_t k = 0;

	delta = first ? delta / 700 : delta / 2;
	delta += delta / numpoints;
	while (delta > ((36 - 1) * 26) / 2) {
		delta /= 36 - 1;
		k += 36;
	}
	return k + ((36 - 1 + 1) * delta) / (delta + 38);
}

static int puny_encode(const uint32_t *cps, size_t ncp, char *out, size_t cap)
{
	size_t b = 0, h, o = 0, i;
	uint32_t n = 128, bias = 72, delta = 0;

	for (i = 0; i < ncp; i++) {
		if (cps[i] < 128) {
			if (o + 1 >= cap)
				return -1;
			out[o++] = (char)cps[i];
			b++;
		}
	}
	h = b;
	if (b > 0) {
		if (o + 1 >= cap)
			return -1;
		out[o++] = '-';
	}
	while (h < ncp) {
		uint32_t m = UINT32_MAX;

		for (i = 0; i < ncp; i++) {
			if (cps[i] >= n && cps[i] < m)
				m = cps[i];
		}
		if (m == UINT32_MAX)
			return -1;
		if ((uint64_t)(m - n) * (h + 1) + delta > UINT32_MAX)
			return -1;
		delta += (m - n) * (uint32_t)(h + 1);
		n = m;
		for (i = 0; i < ncp; i++) {
			uint32_t q, k;

			if (cps[i] < n) {
				if (delta == UINT32_MAX)
					return -1;
				delta++;
				continue;
			}
			if (cps[i] != n)
				continue;
			q = delta;
			for (k = 36;; k += 36) {
				uint32_t t;

				if (k <= bias + 1)
					t = 1;
				else if (k >= bias + 26)
					t = 26;
				else
					t = k - bias;
				if (q < t)
					break;
				if (o + 1 >= cap)
					return -1;
				out[o++] = puny_digit(t + (q - t) % (36 - t));
				q = (q - t) / (36 - t);
			}
			if (o + 1 >= cap)
				return -1;
			out[o++] = puny_digit(q);
			bias = puny_adapt(delta, (uint32_t)(h + 1), h == b);
			delta = 0;
			h++;
		}
		delta++;
		n++;
	}
	out[o] = 0;
	return (int)o;
}

static int utf8_decode(const uint8_t *s, size_t n, uint32_t *cps, size_t cap, size_t *nout)
{
	size_t i = 0, o = 0;

	while (i < n) {
		uint8_t c = s[i];
		uint32_t cp;
		size_t need, j;

		if (c < 0x80) {
			cp = c;
			need = 1;
		} else if ((c & 0xE0) == 0xC0 && c >= 0xC2) {
			cp = c & 0x1Fu;
			need = 2;
		} else if ((c & 0xF0) == 0xE0) {
			cp = c & 0x0Fu;
			need = 3;
		} else if ((c & 0xF8) == 0xF0 && c <= 0xF4) {
			cp = c & 0x07u;
			need = 4;
		} else {
			return -1;
		}
		if (i + need > n)
			return -1;
		for (j = 1; j < need; j++) {
			if ((s[i + j] & 0xC0) != 0x80)
				return -1;
			cp = (cp << 6) | (s[i + j] & 0x3Fu);
		}
		if (need == 3 && cp < 0x800)
			return -1;
		if (need == 4 && cp < 0x10000)
			return -1;
		if (cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF))
			return -1;
		if (o >= cap)
			return -1;
		cps[o++] = cp;
		i += need;
	}
	*nout = o;
	return 0;
}

static int label_idna(const uint8_t *s, size_t n, uint8_t *out, size_t cap, size_t *nout)
{
	size_t i;
	int hi = 0;

	for (i = 0; i < n; i++) {
		if (s[i] >= 128)
			hi = 1;
	}
	if (!hi) {
		if (n > cap)
			return -1;
		for (i = 0; i < n; i++) {
			uint8_t c = s[i];

			if (c >= 'A' && c <= 'Z')
				c = (uint8_t)(c - 'A' + 'a');
			out[i] = c;
		}
		*nout = n;
		return 0;
	}
	{
		uint32_t cps[256];
		size_t ncp;
		char puny[512];
		int pn;

		if (utf8_decode(s, n, cps, 256, &ncp) != 0)
			return -1;
		pn = puny_encode(cps, ncp, puny, sizeof puny);
		if (pn < 0)
			return -1;
		if ((size_t)pn + 4 > cap)
			return -1;
		memcpy(out, "xn--", 4);
		memcpy(out + 4, puny, (size_t)pn);
		*nout = (size_t)pn + 4;
	}
	return 0;
}

static int domain_idna(const uint8_t *s, size_t n, uint8_t *out, size_t cap, size_t *nout)
{
	size_t i = 0, o = 0;

	while (i <= n) {
		size_t start = i;
		size_t ln, wrote;

		while (i < n && s[i] != '.')
			i++;
		ln = i - start;
		if (label_idna(s + start, ln, out + o, cap - o, &wrote) != 0)
			return -1;
		o += wrote;
		if (i < n) {
			if (o >= cap)
				return -1;
			out[o++] = '.';
			i++;
			continue;
		}
		break;
	}
	*nout = o;
	return 0;
}

enum {
	PSL_FLAG_EXCEPTION = 1 << 0,
	PSL_FLAG_WILDCARD = 1 << 1,
	PSL_FLAG_ICANN = 1 << 2,
	PSL_FLAG_PRIVATE = 1 << 3,
	PSL_FLAG_PLAIN = 1 << 4
};

static int psl_nsuffixes;
static int psl_nexceptions;
static int psl_nwildcards;
static const char *psl_input_file;

typedef struct {
	uint8_t *p;
	size_t n;
	int flags;
} entry;

typedef struct {
	entry *v;
	size_t n, cap;
} evec;

static int contains_bytes(const uint8_t *s, size_t n, const char *lit)
{
	size_t m = strlen(lit);
	size_t i;

	if (m > n)
		return 0;
	for (i = 0; i + m <= n; i++) {
		if (memcmp(s + i, lit, m) == 0)
			return 1;
	}
	return 0;
}

static char hex_upper(unsigned v)
{
	v &= 0x0fu;
	return (char)(v < 10 ? '0' + v : 'A' + (v - 10));
}

static int entry_cmp(const void *a, const void *b)
{
	const entry *ea = a;
	const entry *eb = b;
	bstr ba, bb;

	ba.p = ea->p;
	ba.n = ea->n;
	bb.p = eb->p;
	bb.n = eb->n;
	return bstr_cmp(&ba, &bb);
}

static void *entry_token(size_t idx)
{
	return (void *)(uintptr_t)(idx + 1);
}

static entry *entry_from_token(evec *entries, void *tok)
{
	size_t idx = (size_t)(uintptr_t)tok - 1;

	return &entries->v[idx];
}

static bvec parse_psl(const uint8_t *data, size_t n, int utf_mode)
{
	size_t i = 0;
	int section = 0;
	bmap map = {0};
	evec entries = {0};
	bvec words = {0};

	psl_nsuffixes = 0;
	psl_nexceptions = 0;
	psl_nwildcards = 0;
	while (i < n) {
		size_t start = i;
		size_t len, a, b;
		const uint8_t *line;
		int flags;
		uint8_t puny[1024];
		size_t puny_n;
		void *have;

		while (i < n && data[i] != '\n')
			i++;
		len = i - start;
		if (i < n)
			i++;
		a = 0;
		b = len;
		while (a < b && (data[start + a] == ' ' || data[start + a] == '\t' ||
				 data[start + a] == '\r' || data[start + a] == '\v' ||
				 data[start + a] == '\f'))
			a++;
		while (b > a && (data[start + b - 1] == ' ' || data[start + b - 1] == '\t' ||
				 data[start + b - 1] == '\r' || data[start + b - 1] == '\v' ||
				 data[start + b - 1] == '\f'))
			b--;
		if (a == b)
			continue;
		line = data + start + a;
		len = b - a;
		if (len >= 2 && line[0] == '/' && line[1] == '/') {
			if (section == 0) {
				if (contains_bytes(line, len, "===BEGIN ICANN DOMAINS==="))
					section = PSL_FLAG_ICANN;
				else if (contains_bytes(line, len, "===BEGIN PRIVATE DOMAINS==="))
					section = PSL_FLAG_PRIVATE;
			} else if (section == PSL_FLAG_ICANN &&
				   contains_bytes(line, len, "===END ICANN DOMAINS===")) {
				section = 0;
			} else if (section == PSL_FLAG_PRIVATE &&
				   contains_bytes(line, len, "===END PRIVATE DOMAINS===")) {
				section = 0;
			}
			continue;
		}
		if (line[0] == '!') {
			psl_nexceptions++;
			flags = PSL_FLAG_EXCEPTION | section;
			line++;
			len--;
		} else if (line[0] == '*') {
			if (len < 2 || line[1] != '.') {
				fprintf(stderr, "psl-make-dafsa-c: unsupported rule ignored\n");
				continue;
			}
			psl_nwildcards++;
			psl_nsuffixes++;
			flags = PSL_FLAG_WILDCARD | PSL_FLAG_PLAIN | section;
			line += 2;
			len -= 2;
		} else {
			psl_nsuffixes++;
			flags = PSL_FLAG_PLAIN | section;
		}
		if (domain_idna(line, len, puny, sizeof puny, &puny_n) != 0)
			die("idna conversion failed");
		if (bmap_get(&map, puny, puny_n) != NULL) {
			fprintf(stderr, "psl-make-dafsa-c: duplicate rule skipped\n");
			continue;
		}
		if (utf_mode) {
			have = bmap_get(&map, line, len);
			if (have != NULL) {
				entry_from_token(&entries, have)->flags = flags;
			} else {
				entry *e = xmalloc(sizeof *e);

				e->p = xmalloc(len ? len : 1);
				if (len)
					memcpy(e->p, line, len);
				e->n = len;
				e->flags = flags;
				if (entries.n == entries.cap) {
					entries.cap = entries.cap ? entries.cap * 2 : 16;
					entries.v = xrealloc(entries.v, entries.cap * sizeof *entries.v);
				}
				entries.v[entries.n++] = *e;
				bmap_put(&map, line, len, entry_token(entries.n - 1));
				free(e);
			}
		}
		have = bmap_get(&map, puny, puny_n);
		if (have != NULL) {
			entry_from_token(&entries, have)->flags = flags;
		} else {
			entry *e = xmalloc(sizeof *e);

			e->p = xmalloc(puny_n ? puny_n : 1);
			if (puny_n)
				memcpy(e->p, puny, puny_n);
			e->n = puny_n;
			e->flags = flags;
			if (entries.n == entries.cap) {
				entries.cap = entries.cap ? entries.cap * 2 : 16;
				entries.v = xrealloc(entries.v, entries.cap * sizeof *entries.v);
			}
			entries.v[entries.n++] = *e;
			bmap_put(&map, puny, puny_n, entry_token(entries.n - 1));
			free(e);
		}
	}
	qsort(entries.v, entries.n, sizeof *entries.v, entry_cmp);
	for (i = 0; i < entries.n; i++) {
		bstr w;
		entry *e = &entries.v[i];

		w.n = e->n + 1;
		w.p = xmalloc(w.n);
		if (e->n)
			memcpy(w.p, e->p, e->n);
		w.p[e->n] = (uint8_t)hex_upper((unsigned)e->flags);
		bvec_push(&words, w);
	}
	return words;
}

static int char_length(unsigned byte)
{
	if (byte < 0x20 || (byte >= 0x80 && byte < 0xC0) || byte >= 0xF8)
		return 0;
	if (byte < 0x80)
		return 1;
	if (byte < 0xE0)
		return 2;
	if (byte < 0xF0)
		return 3;
	return 4;
}

static int hex_val(unsigned c)
{
	if (c >= '0' && c <= '9')
		return (int)(c - '0');
	if (c >= 'A' && c <= 'F')
		return (int)(c - 'A' + 10);
	if (c >= 'a' && c <= 'f')
		return (int)(c - 'a' + 10);
	return -1;
}

static node *to_nodes(const uint8_t *w, size_t len, int multi, int utf_mode)
{
	unsigned byte;
	int cl;

	if (len == 0)
		die("empty word");
	byte = w[0];
	if (multi) {
		uint8_t lab;
		node *nd;

		if ((byte & 0xC0) != 0x80)
			die("Invalid UTF-8 multibyte sequence");
		lab = (uint8_t)(byte ^ 0xC0);
		nd = node_new(&lab, 1);
		node_add(nd, to_nodes(w + 1, len - 1, multi - 1, utf_mode));
		return nd;
	}
	cl = char_length(byte);
	if (cl == 1) {
		node *nd;

		if (len == 1) {
			int v = hex_val(byte);
			uint8_t lab;

			if (v < 0)
				die("terminal is not a hex digit");
			lab = (uint8_t)(v & 0x0F);
			nd = node_new(&lab, 1);
			node_add(nd, NULL);
			return nd;
		}
		nd = node_new(w, 1);
		node_add(nd, to_nodes(w + 1, len - 1, 0, utf_mode));
		return nd;
	}
	if (cl > 1) {
		uint8_t lead = 0x1F;
		uint8_t mid;
		node *nd, *midn;

		if (!utf_mode)
			die("UTF-8 encoded characters are not allowed in ASCII mode");
		if (len <= (size_t)cl)
			die("Unterminated UTF-8 multibyte sequence");
		nd = node_new(&lead, 1);
		mid = (uint8_t)(byte ^ 0x80);
		midn = node_new(&mid, 1);
		node_add(midn, to_nodes(w + 1, len - 1, cl - 1, utf_mode));
		node_add(nd, midn);
		return nd;
	}
	die("Domain names must be printable ASCII or UTF-8");
	return NULL;
}

static nvec to_dafsa(const bvec *words, int utf_mode)
{
	nvec roots = {0};
	size_t i;

	if (words->n == 0)
		die("The domain list must not be empty");
	for (i = 0; i < words->n; i++)
		nvec_push(&roots, to_nodes(words->v[i].p, words->v[i].n, 0, utf_mode));
	return roots;
}

static void rev_dfs(node *n, node *parent, nvec *sink, pmap *map)
{
	node *rn;
	void *val;
	size_t i;

	if (n == NULL) {
		nvec_push(sink, parent);
		return;
	}
	if (pmap_get(map, n, &val)) {
		node_add(val, parent);
		return;
	}
	{
		uint8_t *r = xmalloc(n->lab_len ? n->lab_len : 1);

		for (i = 0; i < n->lab_len; i++)
			r[i] = n->lab[n->lab_len - 1 - i];
		rn = node_new(r, n->lab_len);
		free(r);
	}
	node_add(rn, parent);
	pmap_set(map, n, rn);
	for (i = 0; i < n->nch; i++)
		rev_dfs(n->ch[i], rn, sink, map);
}

static nvec reverse_g(nvec in)
{
	nvec sink = {0};
	pmap map = {0};
	size_t i;

	for (i = 0; i < in.n; i++)
		rev_dfs(in.v[i], NULL, &sink, &map);
	return sink;
}

static void to_words(node *n, bvec *out, pmap *cache)
{
	void *val;
	bvec *mine;
	size_t i, j;

	if (n == NULL) {
		bstr e;

		e.p = NULL;
		e.n = 0;
		bvec_push(out, e);
		return;
	}
	if (pmap_get(cache, n, &val)) {
		bvec *cached = val;

		for (i = 0; i < cached->n; i++)
			bvec_push(out, cached->v[i]);
		return;
	}
	mine = xmalloc(sizeof *mine);
	*mine = (bvec){0};
	for (i = 0; i < n->nch; i++) {
		bvec child = {0};

		to_words(n->ch[i], &child, cache);
		for (j = 0; j < child.n; j++)
			bvec_push(mine, bstr_cat(n->lab, n->lab_len, child.v[j].p, child.v[j].n));
	}
	pmap_set(cache, n, mine);
	for (i = 0; i < mine->n; i++)
		bvec_push(out, mine->v[i]);
}

static bstr canon_words(bvec *words)
{
	bstr s;
	size_t bytes = 0, i;
	uint8_t *w;

	bvec_unique_sorted(words);
	for (i = 0; i < words->n; i++)
		bytes += 4 + words->v[i].n;
	s.n = bytes;
	s.p = xmalloc(bytes ? bytes : 1);
	w = s.p;
	for (i = 0; i < words->n; i++) {
		uint32_t ln = (uint32_t)words->v[i].n;

		w[0] = (uint8_t)(ln >> 24);
		w[1] = (uint8_t)(ln >> 16);
		w[2] = (uint8_t)(ln >> 8);
		w[3] = (uint8_t)ln;
		if (ln)
			memcpy(w + 4, words->v[i].p, ln);
		w += 4 + ln;
	}
	return s;
}

static node *join_suffix_one(node *n, bmap *nodemap, pmap *cache)
{
	bvec words = {0};
	bstr key;
	node *found;
	node *created;
	size_t i;

	if (n == NULL)
		return NULL;
	to_words(n, &words, cache);
	key = canon_words(&words);
	found = bmap_get(nodemap, key.p, key.n);
	if (found != NULL)
		return found;
	created = node_new(n->lab, n->lab_len);
	bmap_put(nodemap, key.p, key.n, created);
	for (i = 0; i < n->nch; i++)
		node_add(created, join_suffix_one(n->ch[i], nodemap, cache));
	return created;
}

static nvec join_suffixes(nvec in)
{
	bmap nodemap = {0};
	pmap cache = {0};
	nvec out = {0};
	size_t i;

	for (i = 0; i < in.n; i++)
		nvec_push(&out, join_suffix_one(in.v[i], &nodemap, &cache));
	return out;
}

static void count_parents(node *n, pmap *pc, size_t *nulls)
{
	void *val;
	size_t i;

	if (n == NULL) {
		(*nulls)++;
		return;
	}
	if (pmap_get(pc, n, &val)) {
		pmap_set(pc, n, (void *)(uintptr_t)((size_t)(uintptr_t)val + 1));
		return;
	}
	pmap_set(pc, n, (void *)(uintptr_t)1);
	for (i = 0; i < n->nch; i++)
		count_parents(n->ch[i], pc, nulls);
}

static size_t parent_of(pmap *pc, node *n, size_t nulls)
{
	void *val;

	if (n == NULL)
		return nulls;
	if (!pmap_get(pc, n, &val))
		die("missing parent count");
	return (size_t)(uintptr_t)val;
}

static node *join_label_one(node *n, pmap *nodemap, pmap *pc, size_t nulls)
{
	void *val;
	node *created;
	node **kids;
	size_t i;

	if (n == NULL)
		return NULL;
	if (pmap_get(nodemap, n, &val))
		return val;
	kids = xmalloc((n->nch ? n->nch : 1) * sizeof *kids);
	for (i = 0; i < n->nch; i++)
		kids[i] = join_label_one(n->ch[i], nodemap, pc, nulls);
	if (n->nch == 1 && parent_of(pc, n->ch[0], nulls) == 1 && n->ch[0] != NULL) {
		node *child = kids[0];
		uint8_t *lab = xmalloc(n->lab_len + child->lab_len + 1);

		if (n->lab_len)
			memcpy(lab, n->lab, n->lab_len);
		if (child->lab_len)
			memcpy(lab + n->lab_len, child->lab, child->lab_len);
		created = node_new(lab, n->lab_len + child->lab_len);
		free(lab);
		for (i = 0; i < child->nch; i++)
			node_add(created, child->ch[i]);
	} else {
		created = node_new(n->lab, n->lab_len);
		for (i = 0; i < n->nch; i++)
			node_add(created, kids[i]);
	}
	free(kids);
	pmap_set(nodemap, n, created);
	return created;
}

static nvec join_labels(nvec in)
{
	pmap pc = {0};
	pmap nodemap = {0};
	nvec out = {0};
	size_t nulls = 2;
	size_t i;

	for (i = 0; i < in.n; i++)
		count_parents(in.v[i], &pc, &nulls);
	for (i = 0; i < in.n; i++)
		nvec_push(&out, join_label_one(in.v[i], &nodemap, &pc, nulls));
	return out;
}

static void count_incoming(node *n, pmap *inc)
{
	void *val;
	size_t i;

	if (n == NULL)
		return;
	if (pmap_get(inc, n, &val)) {
		pmap_set(inc, n, (void *)(uintptr_t)((size_t)(uintptr_t)val + 1));
		return;
	}
	pmap_set(inc, n, (void *)(uintptr_t)1);
	for (i = 0; i < n->nch; i++)
		count_incoming(n->ch[i], inc);
}

static size_t incoming_of(pmap *inc, node *n)
{
	void *val;

	if (!pmap_get(inc, n, &val))
		die("missing incoming count");
	return (size_t)(uintptr_t)val;
}

static void count_graph(node *n, pmap *seen, size_t *c)
{
	void *ignore;
	size_t i;

	if (n == NULL)
		return;
	if (pmap_get(seen, n, &ignore))
		return;
	pmap_set(seen, n, n);
	(*c)++;
	for (i = 0; i < n->nch; i++)
		count_graph(n->ch[i], seen, c);
}

static nvec top_sort(nvec in)
{
	pmap inc = {0};
	pmap seen = {0};
	nvec order = {0};
	nvec wait = {0};
	size_t total = 0;
	size_t i;

	for (i = 0; i < in.n; i++)
		count_incoming(in.v[i], &inc);
	for (i = 0; i < in.n; i++) {
		size_t c = incoming_of(&inc, in.v[i]);

		if (c == 0)
			die("incoming underflow");
		pmap_set(&inc, in.v[i], (void *)(uintptr_t)(c - 1));
	}
	for (i = 0; i < in.n; i++) {
		if (incoming_of(&inc, in.v[i]) == 0)
			nvec_push(&wait, in.v[i]);
	}
	while (wait.n > 0) {
		node *nd = wait.v[--wait.n];

		if (incoming_of(&inc, nd) != 0)
			die("top_sort invariant");
		nvec_push(&order, nd);
		for (i = 0; i < nd->nch; i++) {
			node *child = nd->ch[i];
			size_t c;

			if (child == NULL)
				continue;
			c = incoming_of(&inc, child);
			if (c == 0)
				die("incoming underflow");
			c--;
			pmap_set(&inc, child, (void *)(uintptr_t)c);
			if (c == 0)
				nvec_push(&wait, child);
		}
	}
	for (i = 0; i < in.n; i++)
		count_graph(in.v[i], &seen, &total);
	if (order.n != total)
		die("top_sort did not reach every node");
	return order;
}

static pmap *g_off;

static size_t offset_of(pmap *off, node *n)
{
	void *val;

	if (!pmap_get(off, n, &val))
		die("missing dafsa offset");
	return (size_t)(uintptr_t)val;
}

static int cmp_off_desc(const void *a, const void *b)
{
	node *na = *(node *const *)a;
	node *nb = *(node *const *)b;
	size_t oa = offset_of(g_off, na);
	size_t ob = offset_of(g_off, nb);

	if (oa > ob)
		return -1;
	if (oa < ob)
		return 1;
	return 0;
}

static uvec encode_prefix(const node *nd)
{
	uvec u = {0};
	size_t i;

	if (nd->lab_len == 0)
		die("empty label");
	for (i = nd->lab_len; i-- > 0;)
		uvec_push(&u, nd->lab[i]);
	return u;
}

static uvec encode_label(const node *nd)
{
	uvec u = encode_prefix(nd);

	u.p[0] = (uint8_t)(u.p[0] | 0x80);
	return u;
}

static uvec encode_links(node **children, size_t nch, pmap *offsets, size_t current)
{
	uvec empty = {0};
	node **ch;
	size_t guess, iter;

	if (nch == 0)
		die("encode_links: no children");
	if (children[0] == NULL) {
		if (nch != 1)
			die("end label has extra links");
		return empty;
	}
	ch = xmalloc(nch * sizeof *ch);
	memcpy(ch, children, nch * sizeof *ch);
	g_off = offsets;
	qsort(ch, nch, sizeof *ch, cmp_off_desc);
	guess = 3 * nch;
	for (iter = 0; iter < 8; iter++) {
		uvec buf = {0};
		size_t offset = current + guess;
		size_t last = 0;
		size_t i;

		for (i = 0; i < nch; i++) {
			size_t child_off = offset_of(offsets, ch[i]);
			size_t distance;

			last = buf.n;
			if (offset <= child_off)
				die("bad dafsa distance");
			distance = offset - child_off;
			if (distance == 0 || distance >= (1u << 21))
				die("dafsa distance out of range");
			if (distance < (1u << 6)) {
				uvec_push(&buf, (uint8_t)distance);
			} else if (distance < (1u << 13)) {
				uvec_push(&buf, (uint8_t)(0x40 | (distance >> 8)));
				uvec_push(&buf, (uint8_t)(distance & 0xFF));
			} else {
				uvec_push(&buf, (uint8_t)(0x60 | (distance >> 16)));
				uvec_push(&buf, (uint8_t)((distance >> 8) & 0xFF));
				uvec_push(&buf, (uint8_t)(distance & 0xFF));
			}
			offset -= distance;
		}
		if (buf.n == guess) {
			buf.p[last] = (uint8_t)(buf.p[last] | 0x80);
			uvec_reverse(&buf);
			free(ch);
			return buf;
		}
		guess = buf.n;
		free(buf.p);
	}
	die("encode_links did not converge");
	return empty;
}

static uvec encode(nvec roots, int utf_mode)
{
	nvec order = top_sort(roots);
	pmap offsets = {0};
	uvec output = {0};
	size_t i;

	for (i = order.n; i-- > 0;) {
		node *nd = order.v[i];
		void *have = NULL;

		if (nd->nch == 1 && nd->ch[0] != NULL &&
		    pmap_get(&offsets, nd->ch[0], &have) &&
		    offset_of(&offsets, nd->ch[0]) == output.n) {
			uvec pref = encode_prefix(nd);

			uvec_extend(&output, pref.p, pref.n);
			free(pref.p);
		} else {
			uvec links = encode_links(nd->ch, nd->nch, &offsets, output.n);
			uvec lab = encode_label(nd);

			uvec_extend(&output, links.p, links.n);
			uvec_extend(&output, lab.p, lab.n);
			free(links.p);
			free(lab.p);
		}
		pmap_set(&offsets, nd, (void *)(uintptr_t)output.n);
	}
	{
		uvec links = encode_links(roots.v, roots.n, &offsets, output.n);

		uvec_extend(&output, links.p, links.n);
		free(links.p);
	}
	uvec_reverse(&output);
	if (utf_mode)
		uvec_push(&output, 0x01);
	return output;
}

static uvec to_cxx(const uvec *data)
{
	uvec t = {0};
	char num[32];
	size_t i;
	const char *head =
		"/* This file has been generated by psl-make-dafsa. DO NOT EDIT!\n\n"
		"The byte array encodes effective tld names. See psl-make-dafsa source for"
		" documentation."
		"*/\n\n"
		"static const unsigned char kDafsa[";

	uvec_extend(&t, (const uint8_t *)head, strlen(head));
	snprintf(num, sizeof num, "%zu", data->n);
	uvec_extend(&t, (const uint8_t *)num, strlen(num));
	uvec_extend(&t, (const uint8_t *)"] = {\n", 6);
	for (i = 0; i < data->n; i++) {
		char byte[8];

		if (i % 12 == 0)
			uvec_extend(&t, (const uint8_t *)"  ", 2);
		snprintf(byte, sizeof byte, "0x%02x", data->p[i]);
		uvec_extend(&t, (const uint8_t *)byte, strlen(byte));
		uvec_push(&t, (uint8_t)',');
		if (i % 12 == 11 || i + 1 == data->n)
			uvec_push(&t, (uint8_t)'\n');
		else
			uvec_push(&t, (uint8_t)' ');
	}
	uvec_extend(&t, (const uint8_t *)"};\n", 3);
	return t;
}

static uvec to_cxx_plus(const uvec *data)
{
	uvec t = to_cxx(data);
	struct stat st;
	char line[1024];
	char sum[41];
	int k;

	if (stat(psl_input_file, &st) != 0)
		die_errno(psl_input_file);
	sha1_file(psl_input_file, sum);
	k = snprintf(line, sizeof line, "static time_t _psl_file_time = %d;\n", (int)st.st_mtime);
	if (k < 0 || (size_t)k >= sizeof line)
		die("time line truncated");
	uvec_extend(&t, (const uint8_t *)line, (size_t)k);
	k = snprintf(line, sizeof line, "static int _psl_nsuffixes = %d;\n", psl_nsuffixes);
	uvec_extend(&t, (const uint8_t *)line, (size_t)k);
	k = snprintf(line, sizeof line, "static int _psl_nexceptions = %d;\n", psl_nexceptions);
	uvec_extend(&t, (const uint8_t *)line, (size_t)k);
	k = snprintf(line, sizeof line, "static int _psl_nwildcards = %d;\n", psl_nwildcards);
	uvec_extend(&t, (const uint8_t *)line, (size_t)k);
	k = snprintf(line, sizeof line, "static const char _psl_sha1_checksum[] = \"%s\";\n", sum);
	if (k < 0 || (size_t)k >= sizeof line)
		die("sha1 line truncated");
	uvec_extend(&t, (const uint8_t *)line, (size_t)k);
	k = snprintf(line, sizeof line, "static const char _psl_filename[] = \"%s\";\n", psl_input_file);
	if (k < 0 || (size_t)k >= sizeof line)
		die("filename line truncated");
	uvec_extend(&t, (const uint8_t *)line, (size_t)k);
	return t;
}

static uvec dafsa_bytes(bvec words, int utf_mode)
{
	nvec g = to_dafsa(&words, utf_mode);

	g = reverse_g(g);
	g = join_suffixes(g);
	g = reverse_g(g);
	g = join_suffixes(g);
	g = join_labels(g);
	return encode(g, utf_mode);
}

static uvec words_to_output(bvec words, int utf_mode, int plus)
{
	uvec bytes = dafsa_bytes(words, utf_mode);

	if (plus)
		return to_cxx_plus(&bytes);
	return to_cxx(&bytes);
}

static uint8_t *read_all(const char *path, size_t *n)
{
	FILE *f = fopen(path, "rb");
	long sz;
	uint8_t *p;
	size_t got;

	if (f == NULL)
		die_errno(path);
	if (fseek(f, 0, SEEK_END) != 0)
		die_errno(path);
	sz = ftell(f);
	if (sz < 0)
		die_errno(path);
	if (fseek(f, 0, SEEK_SET) != 0)
		die_errno(path);
	p = xmalloc((size_t)sz + 1);
	got = fread(p, 1, (size_t)sz, f);
	fclose(f);
	if (got != (size_t)sz)
		die("short read");
	*n = got;
	return p;
}

static int selftest(void)
{
	static const uint8_t books[] = {0x62, 0xc3, 0xbc, 0x63, 0x68, 0x65, 0x72};
	static const uint8_t mun[] = {0x6d, 0xc3, 0xbc, 0x6e, 0x63, 0x68, 0x65, 0x6e};
	uint32_t cps[8];
	size_t ncp;
	char puny[64];
	const char *fix =
		"// ===BEGIN ICANN DOMAINS===\n"
		"com\n"
		"*.ck\n"
		"!www.ck\n"
		"// ===END ICANN DOMAINS===\n";
	bvec words;

	if (utf8_decode(books, sizeof books, cps, 8, &ncp) != 0 || ncp != 6)
		die("utf8 books");
	if (puny_encode(cps, ncp, puny, sizeof puny) < 0 || strcmp(puny, "bcher-kva") != 0)
		die_fmt("puny books got %s", puny);
	if (utf8_decode(mun, sizeof mun, cps, 8, &ncp) != 0)
		die("utf8 mun");
	if (puny_encode(cps, ncp, puny, sizeof puny) < 0 || strcmp(puny, "mnchen-3ya") != 0)
		die_fmt("puny mun got %s", puny);
	words = parse_psl((const uint8_t *)fix, strlen(fix), 1);
	if (psl_nsuffixes != 2 || psl_nexceptions != 1 || psl_nwildcards != 1)
		die_fmt("fixture counts %d %d %d", psl_nsuffixes, psl_nexceptions, psl_nwildcards);
	if (words.n != 3)
		die_fmt("fixture words %zu", words.n);
	if (words.v[0].n != 3 || memcmp(words.v[0].p, "ck6", 3) != 0 ||
	    words.v[1].n != 4 || memcmp(words.v[1].p, "com4", 4) != 0 ||
	    words.v[2].n != 7 || memcmp(words.v[2].p, "www.ck5", 7) != 0)
		die("fixture word bytes");
	{
		uvec out = dafsa_bytes(words, 1);

		if (out.n < 1 || out.p[out.n - 1] != 0x01)
			die("fixture dafsa missing utf marker");
	}
	return 0;
}

static void usage(void)
{
	fprintf(stderr, "usage: psl-make-dafsa-c [--output-format=cxx+] infile outfile\n");
	exit(1);
}

int main(int argc, char **argv)
{
	int utf_mode = 1;
	int plus = 0;
	int i;
	const char *in_path;
	const char *out_path;
	uint8_t *data;
	size_t n;
	bvec words;
	uvec text;
	FILE *out;

	if (argc == 2 && strcmp(argv[1], "--selftest") == 0)
		return selftest();
	if (argc < 3)
		usage();
	for (i = 1; i < argc - 2; i++) {
		if (strncmp(argv[i], "--output-format=", 16) == 0) {
			const char *v = argv[i] + 16;

			if (strcmp(v, "cxx+") == 0)
				plus = 1;
			else if (strcmp(v, "cxx") == 0)
				plus = 0;
			else
				die_fmt("Unknown output format '%s'", v);
		} else if (strncmp(argv[i], "--encoding=", 11) == 0) {
			const char *v = argv[i] + 11;

			if (strcmp(v, "ascii") == 0)
				utf_mode = 0;
			else if (strcmp(v, "utf-8") == 0)
				utf_mode = 1;
			else
				die_fmt("Unknown encoding '%s'", v);
		} else if (strncmp(argv[i], "--input-format=", 15) == 0) {
			if (strcmp(argv[i] + 15, "psl") != 0)
				die_fmt("Unknown input format '%s'", argv[i] + 15);
		} else {
			usage();
		}
	}
	in_path = argv[argc - 2];
	out_path = argv[argc - 1];
	psl_input_file = in_path;
	data = read_all(in_path, &n);
	words = parse_psl(data, n, utf_mode);
	text = words_to_output(words, utf_mode, plus);
	out = fopen(out_path, "wb");
	if (out == NULL)
		die_errno(out_path);
	if (fwrite(text.p, 1, text.n, out) != text.n)
		die("short write");
	if (fclose(out) != 0)
		die_errno(out_path);
	return 0;
}
