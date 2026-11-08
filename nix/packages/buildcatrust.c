/* C replacement for the PyPI buildcatrust package.
 * nss-cacert runs this binary in buildPhase. The PyPI package is
 * Python and is not restored. This file is C only.
 *
 * Flags match pkgs/by-name/ca/cacert/package.nix at nixpkgs
 * 7dcdd87373d72e7a2173e4db02e20d3d7f495cfd:
 *   --certdata_input, --ca_bundle_input, --blocklist,
 *   --ca_bundle_output, --ca_standard_bundle_output,
 *   --ca_unpacked_output, --ca_hashed_unpacked_output,
 *   --p11kit_output.
 * --blocklist_input is the same flag as --blocklist.
 */
#define _GNU_SOURCE

#include <ctype.h>
#include <dirent.h>
#include <errno.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

enum {
  T_UNKNOWN = 0,
  T_TRUSTED_DELEGATOR,
  T_MUST_VERIFY,
  T_NOT_TRUSTED,
  T_TRUSTED
};

enum {
  TR_SERVER = 0,
  TR_CLIENT,
  TR_CODE,
  TR_DIGITAL,
  TR_NONREP,
  TR_KEYENC,
  TR_DATAENC,
  TR_KEYAGREE,
  TR_KEYCERT,
  TR_CRLSIGN,
  TR_EMAIL,
  TR_IPSEC_END,
  TR_IPSEC_TUN,
  TR_IPSEC_USER,
  TR_TIME,
  TR_COUNT
};

enum { C_BUILTIN = 1, C_CERT, C_TRUST };

enum { AV_BOOL, AV_TRUST, AV_CLASS, AV_STR, AV_BYTES };

typedef struct {
  unsigned char *p;
  size_t n, cap;
} Buf;

typedef struct {
  char *key;
  int kind;
  int num;
  char *str;
  unsigned char *bytes;
  size_t nbytes;
} Av;

typedef struct {
  Av *a;
  size_t n, cap;
} Obj;

typedef struct {
  unsigned comp[16];
  int ncomp;
  char *text;
} Ava;

typedef struct {
  Ava *a;
  int n, cap;
} Rdn;

typedef struct {
  Rdn *r;
  int n, cap;
} Dn;

typedef struct {
  const char *dotted;
  const char *name;
  unsigned comp[12];
  int ncomp;
  int trust_index;
} Purpose;

typedef struct {
  char *key;
  char *label;
  unsigned char *serial;
  size_t nserial;
  unsigned char *issuer;
  size_t nissuer;
  unsigned char *subject;
  size_t nsubject;
  unsigned char *cert;
  size_t ncert;
  int mozilla;
  unsigned char *server_after;
  size_t nserver_after;
  unsigned char *email_after;
  size_t nemail_after;
  int has_cert;
  int has_trust;
  int trust[TR_COUNT];
} Row;

static Purpose g_purposes[9];
static int g_npurposes;

static const unsigned OID_EKU[] = {2, 5, 29, 37};
static const unsigned OID_REJECT[] = {1, 3, 6, 1, 4, 1, 3319, 6, 10, 1};
static const unsigned OID_DUMMY[] = {1, 3, 6, 1, 4, 1, 3319, 6, 10, 16};

static void die(const char *fmt, ...) {
  va_list ap;
  fputs("buildcatrust: ", stderr);
  va_start(ap, fmt);
  vfprintf(stderr, fmt, ap);
  va_end(ap);
  fputc('\n', stderr);
  exit(1);
}

static void *xmalloc(size_t n) {
  void *p = malloc(n ? n : 1);
  if (!p) {
    die("out of memory");
  }
  return p;
}

static char *xstrdup(const char *s) {
  size_t n = strlen(s);
  char *d = xmalloc(n + 1);
  memcpy(d, s, n + 1);
  return d;
}

static unsigned char *memdup(const unsigned char *p, size_t n) {
  unsigned char *d = xmalloc(n + 1);
  if (n) {
    memcpy(d, p, n);
  }
  d[n] = 0;
  return d;
}

static void buf_need(Buf *b, size_t extra) {
  if (b->n + extra + 1 <= b->cap) {
    return;
  }
  size_t c = b->cap ? b->cap : 64;
  while (c < b->n + extra + 1) {
    c *= 2;
  }
  unsigned char *np = realloc(b->p, c);
  if (!np) {
    die("out of memory");
  }
  b->p = np;
  b->cap = c;
}

static void buf_add(Buf *b, const void *src, size_t n) {
  buf_need(b, n);
  if (n) {
    memcpy(b->p + b->n, src, n);
  }
  b->n += n;
  b->p[b->n] = 0;
}

static void buf_byte(Buf *b, unsigned char c) {
  buf_add(b, &c, 1);
}

static void buf_len(Buf *b, size_t n) {
  if (n < 0x80) {
    buf_byte(b, (unsigned char)n);
    return;
  }
  unsigned char tmp[8];
  int k = 0;
  size_t x = n;
  while (x) {
    tmp[k++] = (unsigned char)(x & 0xff);
    x >>= 8;
  }
  buf_byte(b, (unsigned char)(0x80 | k));
  while (k) {
    buf_byte(b, tmp[--k]);
  }
}

static void oid_base128(Buf *b, unsigned n) {
  unsigned char tmp[8];
  int k = 0;
  tmp[k++] = (unsigned char)(n & 0x7f);
  n >>= 7;
  while (n) {
    tmp[k++] = (unsigned char)(0x80 | (n & 0x7f));
    n >>= 7;
  }
  while (k) {
    buf_byte(b, tmp[--k]);
  }
}

static void der_oid(Buf *b, const unsigned *oid, int n) {
  if (n < 2) {
    die("oid shorter than two arcs");
  }
  Buf body = {0};
  buf_byte(&body, (unsigned char)(40 * oid[0] + oid[1]));
  for (int i = 2; i < n; i++) {
    oid_base128(&body, oid[i]);
  }
  buf_byte(b, 0x06);
  buf_len(b, body.n);
  buf_add(b, body.p, body.n);
  free(body.p);
}

static int der_read_len(const unsigned char *p, size_t n, size_t off, size_t *len, size_t *next) {
  if (off >= n) {
    return -1;
  }
  unsigned char b = p[off];
  if (b < 0x80) {
    *len = b;
    *next = off + 1;
    return 0;
  }
  if (b == 0x80) {
    return -1;
  }
  int nbytes = b & 0x7f;
  if (nbytes < 1 || nbytes > 4 || off + 1 + (size_t)nbytes > n) {
    return -1;
  }
  size_t L = 0;
  for (int i = 0; i < nbytes; i++) {
    L = (L << 8) | p[off + 1 + i];
  }
  *len = L;
  *next = off + 1 + (size_t)nbytes;
  return 0;
}

static int der_tlv(const unsigned char *p, size_t n, size_t off, unsigned *tag, size_t *c0,
                   size_t *clen, size_t *next) {
  if (off >= n) {
    return -1;
  }
  *tag = p[off];
  if ((*tag & 0x1f) == 0x1f) {
    return -1;
  }
  size_t len = 0;
  size_t after = 0;
  if (der_read_len(p, n, off + 1, &len, &after)) {
    return -1;
  }
  if (after + len < after || after + len > n) {
    return -1;
  }
  *c0 = after;
  *clen = len;
  *next = after + len;
  return 0;
}

static uint32_t rol32(uint32_t v, int n) {
  return (v << n) | (v >> (32 - n));
}

static uint32_t ror32(uint32_t v, int n) {
  return (v >> n) | (v << (32 - n));
}

static void sha1(const unsigned char *msg, size_t len, unsigned char out[20]) {
  uint32_t h0 = 0x67452301u;
  uint32_t h1 = 0xEFCDAB89u;
  uint32_t h2 = 0x98BADCFEu;
  uint32_t h3 = 0x10325476u;
  uint32_t h4 = 0xC3D2E1F0u;
  uint64_t bitlen = (uint64_t)len * 8u;
  size_t newlen = len + 1;
  while ((newlen % 64) != 56) {
    newlen++;
  }
  unsigned char *buf = xmalloc(newlen + 8);
  memset(buf, 0, newlen + 8);
  if (len) {
    memcpy(buf, msg ? msg : (const unsigned char *)"", len);
  }
  buf[len] = 0x80;
  for (int i = 0; i < 8; i++) {
    buf[newlen + i] = (unsigned char)(bitlen >> (56 - 8 * i));
  }
  for (size_t off = 0; off < newlen + 8; off += 64) {
    uint32_t w[80];
    for (int i = 0; i < 16; i++) {
      w[i] = ((uint32_t)buf[off + 4 * i] << 24) | ((uint32_t)buf[off + 4 * i + 1] << 16) |
             ((uint32_t)buf[off + 4 * i + 2] << 8) | (uint32_t)buf[off + 4 * i + 3];
    }
    for (int i = 16; i < 80; i++) {
      w[i] = rol32(w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16], 1);
    }
    uint32_t a = h0, b = h1, c = h2, d = h3, e = h4;
    for (int i = 0; i < 80; i++) {
      uint32_t f, k;
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
      uint32_t temp = rol32(a, 5) + f + e + k + w[i];
      e = d;
      d = c;
      c = rol32(b, 30);
      b = a;
      a = temp;
    }
    h0 += a;
    h1 += b;
    h2 += c;
    h3 += d;
    h4 += e;
  }
  free(buf);
  uint32_t hs[5] = {h0, h1, h2, h3, h4};
  for (int i = 0; i < 5; i++) {
    out[4 * i] = (unsigned char)(hs[i] >> 24);
    out[4 * i + 1] = (unsigned char)(hs[i] >> 16);
    out[4 * i + 2] = (unsigned char)(hs[i] >> 8);
    out[4 * i + 3] = (unsigned char)hs[i];
  }
}

static void sha256(const unsigned char *msg, size_t len, unsigned char out[32]) {
  static const uint32_t K[64] = {
      0x428a2f98u, 0x71374491u, 0xb5c0fbcfu, 0xe9b5dba5u, 0x3956c25bu, 0x59f111f1u, 0x923f82a4u,
      0xab1c5ed5u, 0xd807aa98u, 0x12835b01u, 0x243185beu, 0x550c7dc3u, 0x72be5d74u, 0x80deb1feu,
      0x9bdc06a7u, 0xc19bf174u, 0xe49b69c1u, 0xefbe4786u, 0x0fc19dc6u, 0x240ca1ccu, 0x2de92c6fu,
      0x4a7484aau, 0x5cb0a9dcu, 0x76f988dau, 0x983e5152u, 0xa831c66du, 0xb00327c8u, 0xbf597fc7u,
      0xc6e00bf3u, 0xd5a79147u, 0x06ca6351u, 0x14292967u, 0x27b70a85u, 0x2e1b2138u, 0x4d2c6dfcu,
      0x53380d13u, 0x650a7354u, 0x766a0abbu, 0x81c2c92eu, 0x92722c85u, 0xa2bfe8a1u, 0xa81a664bu,
      0xc24b8b70u, 0xc76c51a3u, 0xd192e819u, 0xd6990624u, 0xf40e3585u, 0x106aa070u, 0x19a4c116u,
      0x1e376c08u, 0x2748774cu, 0x34b0bcb5u, 0x391c0cb3u, 0x4ed8aa4au, 0x5b9cca4fu, 0x682e6ff3u,
      0x748f82eeu, 0x78a5636fu, 0x84c87814u, 0x8cc70208u, 0x90befffau, 0xa4506cebu, 0xbef9a3f7u,
      0xc67178f2u};
  uint32_t h[8] = {0x6a09e667u, 0xbb67ae85u, 0x3c6ef372u, 0xa54ff53au,
                   0x510e527fu, 0x9b05688cu, 0x1f83d9abu, 0x5be0cd19u};
  uint64_t bitlen = (uint64_t)len * 8u;
  size_t newlen = len + 1;
  while ((newlen % 64) != 56) {
    newlen++;
  }
  unsigned char *buf = xmalloc(newlen + 8);
  memset(buf, 0, newlen + 8);
  if (len) {
    memcpy(buf, msg, len);
  }
  buf[len] = 0x80;
  for (int i = 0; i < 8; i++) {
    buf[newlen + i] = (unsigned char)(bitlen >> (56 - 8 * i));
  }
  for (size_t off = 0; off < newlen + 8; off += 64) {
    uint32_t w[64];
    for (int i = 0; i < 16; i++) {
      w[i] = ((uint32_t)buf[off + 4 * i] << 24) | ((uint32_t)buf[off + 4 * i + 1] << 16) |
             ((uint32_t)buf[off + 4 * i + 2] << 8) | (uint32_t)buf[off + 4 * i + 3];
    }
    for (int i = 16; i < 64; i++) {
      uint32_t s0 = ror32(w[i - 15], 7) ^ ror32(w[i - 15], 18) ^ (w[i - 15] >> 3);
      uint32_t s1 = ror32(w[i - 2], 17) ^ ror32(w[i - 2], 19) ^ (w[i - 2] >> 10);
      w[i] = w[i - 16] + s0 + w[i - 7] + s1;
    }
    uint32_t a = h[0], b = h[1], c = h[2], d = h[3], e = h[4], f = h[5], g = h[6], hh = h[7];
    for (int i = 0; i < 64; i++) {
      uint32_t S1 = ror32(e, 6) ^ ror32(e, 11) ^ ror32(e, 25);
      uint32_t ch = (e & f) ^ ((~e) & g);
      uint32_t t1 = hh + S1 + ch + K[i] + w[i];
      uint32_t S0 = ror32(a, 2) ^ ror32(a, 13) ^ ror32(a, 22);
      uint32_t maj = (a & b) ^ (a & c) ^ (b & c);
      uint32_t t2 = S0 + maj;
      hh = g;
      g = f;
      f = e;
      e = d + t1;
      d = c;
      c = b;
      b = a;
      a = t1 + t2;
    }
    h[0] += a;
    h[1] += b;
    h[2] += c;
    h[3] += d;
    h[4] += e;
    h[5] += f;
    h[6] += g;
    h[7] += hh;
  }
  free(buf);
  for (int i = 0; i < 8; i++) {
    out[4 * i] = (unsigned char)(h[i] >> 24);
    out[4 * i + 1] = (unsigned char)(h[i] >> 16);
    out[4 * i + 2] = (unsigned char)(h[i] >> 8);
    out[4 * i + 3] = (unsigned char)h[i];
  }
}

static void hex_of(const unsigned char *dig, size_t n, char *out) {
  static const char *hexd = "0123456789abcdef";
  for (size_t i = 0; i < n; i++) {
    out[2 * i] = hexd[dig[i] >> 4];
    out[2 * i + 1] = hexd[dig[i] & 0xf];
  }
  out[2 * n] = 0;
}

static void serial_hex(const unsigned char *der, size_t n, char *out, size_t outcap) {
  unsigned tag = 0;
  size_t c0 = 0, clen = 0, next = 0;
  if (der_tlv(der, n, 0, &tag, &c0, &clen, &next) || tag != 0x02 || next != n) {
    die("serial number is not one DER integer");
  }
  const unsigned char *p = der + c0;
  size_t len = clen;
  while (len > 1 && p[0] == 0) {
    p++;
    len--;
  }
  if (len == 1 && p[0] == 0) {
    if (outcap < 2) {
      die("serial hex buffer");
    }
    out[0] = '0';
    out[1] = 0;
    return;
  }
  char tmp[160];
  if (len > 64) {
    die("serial number too long");
  }
  hex_of(p, len, tmp);
  const char *s = tmp;
  if (s[0] == '0') {
    s++;
  }
  if (strlen(s) + 1 > outcap) {
    die("serial hex buffer");
  }
  memcpy(out, s, strlen(s) + 1);
}

static char *clean_label(const char *label) {
  size_t n = strlen(label);
  char *d = xmalloc(n + 1);
  for (size_t i = 0; i < n; i++) {
    char c = label[i];
    if (c == '/' || c == ' ' || c == ',') {
      d[i] = '_';
    } else if (c == '(' || c == ')') {
      d[i] = '=';
    } else {
      d[i] = c;
    }
  }
  d[n] = 0;
  return d;
}

static char *make_key(const char *label, const unsigned char *serial, size_t nserial) {
  char hex[160];
  serial_hex(serial, nserial, hex, sizeof hex);
  char *clean = clean_label(label);
  size_t n = strlen(clean) + 1 + strlen(hex) + 1;
  char *key = xmalloc(n);
  snprintf(key, n, "%s:%s", clean, hex);
  free(clean);
  return key;
}

static int b64_val(int c) {
  if (c >= 'A' && c <= 'Z') {
    return c - 'A';
  }
  if (c >= 'a' && c <= 'z') {
    return c - 'a' + 26;
  }
  if (c >= '0' && c <= '9') {
    return c - '0' + 52;
  }
  if (c == '+') {
    return 62;
  }
  if (c == '/') {
    return 63;
  }
  return -1;
}

static void b64_emit(FILE *fp, int *col, char c) {
  fputc(c, fp);
  (*col)++;
  if (*col == 64) {
    fputc('\n', fp);
    *col = 0;
  }
}

static void write_pem(FILE *fp, const char *name, const unsigned char *der, size_t n) {
  static const char *b64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  fprintf(fp, "-----BEGIN %s-----\n", name);
  int col = 0;
  size_t i = 0;
  while (i + 3 <= n) {
    unsigned v = ((unsigned)der[i] << 16) | ((unsigned)der[i + 1] << 8) | (unsigned)der[i + 2];
    b64_emit(fp, &col, b64[(v >> 18) & 63]);
    b64_emit(fp, &col, b64[(v >> 12) & 63]);
    b64_emit(fp, &col, b64[(v >> 6) & 63]);
    b64_emit(fp, &col, b64[v & 63]);
    i += 3;
  }
  if (i < n) {
    unsigned a = der[i];
    unsigned b = (i + 1 < n) ? der[i + 1] : 0;
    unsigned v = (a << 16) | (b << 8);
    b64_emit(fp, &col, b64[(v >> 18) & 63]);
    b64_emit(fp, &col, b64[(v >> 12) & 63]);
    if (i + 1 < n) {
      b64_emit(fp, &col, b64[(v >> 6) & 63]);
    } else {
      b64_emit(fp, &col, '=');
    }
    b64_emit(fp, &col, '=');
  }
  if (col) {
    fputc('\n', fp);
  }
  fprintf(fp, "-----END %s-----\n", name);
}

static int quote_ok(unsigned char c) {
  if ((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9')) {
    return 1;
  }
  return c == '-' || c == '.' || c == '_' || c == '~' || c == '/' || c == ' ';
}

static void quote_bytes(FILE *fp, const unsigned char *p, size_t n) {
  fputc('"', fp);
  for (size_t i = 0; i < n; i++) {
    unsigned char c = p[i];
    if (quote_ok(c)) {
      fputc((int)c, fp);
    } else {
      fprintf(fp, "%%%02X", c);
    }
  }
  fputc('"', fp);
}

static int oid_eq(const unsigned *a, int na, const unsigned *b, int nb) {
  if (na != nb) {
    return 0;
  }
  for (int i = 0; i < na; i++) {
    if (a[i] != b[i]) {
      return 0;
    }
  }
  return 1;
}

static void purpose_add(const char *dotted, const char *name, const unsigned *comp, int ncomp,
                        int trust_index) {
  Purpose *p = &g_purposes[g_npurposes++];
  p->dotted = dotted;
  p->name = name;
  p->ncomp = ncomp;
  p->trust_index = trust_index;
  for (int i = 0; i < ncomp; i++) {
    p->comp[i] = comp[i];
  }
}

static void init_purposes(void) {
  const unsigned server[] = {1, 3, 6, 1, 5, 5, 7, 3, 1};
  const unsigned client[] = {1, 3, 6, 1, 5, 5, 7, 3, 2};
  const unsigned code[] = {1, 3, 6, 1, 5, 5, 7, 3, 3};
  const unsigned email[] = {1, 3, 6, 1, 5, 5, 7, 3, 4};
  const unsigned time[] = {1, 3, 6, 1, 5, 5, 7, 3, 8};
  const unsigned ipsec_end[] = {1, 3, 6, 1, 5, 5, 7, 3, 5};
  const unsigned ipsec_tun[] = {1, 3, 6, 1, 5, 5, 7, 3, 6};
  const unsigned ipsec_user[] = {1, 3, 6, 1, 5, 5, 7, 3, 7};
  purpose_add("1.3.6.1.5.5.7.3.1", "RFC5280: serverAuth key usage", server, 9, TR_SERVER);
  purpose_add("1.3.6.1.5.5.7.3.2", "RFC5280: clientAuth key usage", client, 9, TR_CLIENT);
  purpose_add("1.3.6.1.5.5.7.3.3", "RFC5280: codeSigning key usage", code, 9, TR_CODE);
  purpose_add("1.3.6.1.5.5.7.3.4", "RFC5280: emailProtection key usage", email, 9, TR_EMAIL);
  purpose_add("1.3.6.1.5.5.7.3.8", "RFC5280: timeStamping key usage", time, 9, TR_TIME);
  purpose_add("1.3.6.1.5.5.7.3.5", "RFC2459: ipsecEndSystem key usage", ipsec_end, 9, TR_IPSEC_END);
  purpose_add("1.3.6.1.5.5.7.3.6", "RFC2459: ipsecTunnel key usage", ipsec_tun, 9, TR_IPSEC_TUN);
  purpose_add("1.3.6.1.5.5.7.3.7", "RFC2459: ipsecUser key usage", ipsec_user, 9, TR_IPSEC_USER);
}

static const Purpose *purpose_by_oid(const unsigned *comp, int n) {
  for (int i = 0; i < g_npurposes; i++) {
    if (oid_eq(g_purposes[i].comp, g_purposes[i].ncomp, comp, n)) {
      return &g_purposes[i];
    }
  }
  return NULL;
}

typedef struct {
  const char *name;
  unsigned comp[12];
  int ncomp;
} ShortOid;

static ShortOid g_shorts[11];
static int g_nshorts;

static void short_add(const char *name, const unsigned *comp, int n) {
  ShortOid *s = &g_shorts[g_nshorts++];
  s->name = name;
  s->ncomp = n;
  for (int i = 0; i < n; i++) {
    s->comp[i] = comp[i];
  }
}

static void init_shorts(void) {
  const unsigned c[] = {2, 5, 4, 6};
  const unsigned l[] = {2, 5, 4, 7};
  const unsigned cn[] = {2, 5, 4, 3};
  const unsigned o[] = {2, 5, 4, 10};
  const unsigned ou[] = {2, 5, 4, 11};
  const unsigned orgid[] = {2, 5, 4, 97};
  const unsigned name[] = {2, 5, 4, 41};
  const unsigned given[] = {2, 5, 4, 42};
  const unsigned serial[] = {2, 5, 4, 5};
  const unsigned biz[] = {2, 5, 4, 15};
  const unsigned jur[] = {1, 3, 6, 1, 4, 1, 311, 60, 2, 1, 3};
  short_add("c", c, 4);
  short_add("l", l, 4);
  short_add("cn", cn, 4);
  short_add("o", o, 4);
  short_add("ou", ou, 4);
  short_add("organizationIdentifier", orgid, 4);
  short_add("name", name, 4);
  short_add("givenName", given, 4);
  short_add("serialNumber", serial, 4);
  short_add("businessCategory", biz, 4);
  short_add("jurisdictionOfIncorporationCountryName", jur, 11);
}

static const char *short_name(const unsigned *comp, int n) {
  for (int i = 0; i < g_nshorts; i++) {
    if (oid_eq(g_shorts[i].comp, g_shorts[i].ncomp, comp, n)) {
      return g_shorts[i].name;
    }
  }
  return NULL;
}

static char *oid_dotted(const unsigned *comp, int n) {
  char *s = xmalloc(n * 12 + 1);
  s[0] = 0;
  for (int i = 0; i < n; i++) {
    char piece[16];
    snprintf(piece, sizeof piece, "%s%u", i ? "." : "", comp[i]);
    strcat(s, piece);
  }
  return s;
}

static int decode_base128(const unsigned char *p, size_t n, size_t *pos, unsigned *val) {
  if (*pos >= n) {
    return -1;
  }
  if (p[*pos] < 0x80) {
    *val = p[*pos];
    (*pos)++;
    return 0;
  }
  unsigned v = 0;
  int guard = 0;
  while (*pos < n) {
    unsigned char b = p[*pos];
    (*pos)++;
    v = (v << 7) | (unsigned)(b & 0x7f);
    guard++;
    if (guard > 5) {
      return -1;
    }
    if ((b & 0x80) == 0) {
      *val = v;
      return 0;
    }
  }
  return -1;
}

static int parse_oid(const unsigned char *p, size_t n, size_t off, unsigned *comp, int *ncomp,
                     size_t *next) {
  unsigned tag = 0;
  size_t c0 = 0, clen = 0;
  if (der_tlv(p, n, off, &tag, &c0, &clen, next) || tag != 0x06 || clen < 1) {
    return -1;
  }
  unsigned first = p[c0];
  unsigned a0 = first / 40;
  if (a0 > 2) {
    a0 = 2;
  }
  comp[0] = a0;
  comp[1] = first - a0 * 40;
  int k = 2;
  size_t pos = 1;
  while (pos < clen) {
    unsigned arc = 0;
    if (decode_base128(p + c0, clen, &pos, &arc)) {
      return -1;
    }
    if (k >= 16) {
      return -1;
    }
    comp[k++] = arc;
  }
  *ncomp = k;
  return 0;
}

static int utf16be_to_utf8(const unsigned char *p, size_t n, char **out) {
  if (n % 2) {
    return -1;
  }
  Buf b = {0};
  for (size_t i = 0; i < n; i += 2) {
    unsigned u = ((unsigned)p[i] << 8) | (unsigned)p[i + 1];
    if (u >= 0xD800 && u <= 0xDFFF) {
      free(b.p);
      return -1;
    }
    if (u < 0x80) {
      buf_byte(&b, (unsigned char)u);
    } else if (u < 0x800) {
      buf_byte(&b, (unsigned char)(0xC0 | (u >> 6)));
      buf_byte(&b, (unsigned char)(0x80 | (u & 0x3f)));
    } else {
      buf_byte(&b, (unsigned char)(0xE0 | (u >> 12)));
      buf_byte(&b, (unsigned char)(0x80 | ((u >> 6) & 0x3f)));
      buf_byte(&b, (unsigned char)(0x80 | (u & 0x3f)));
    }
  }
  *out = (char *)(b.p ? b.p : (unsigned char *)xstrdup(""));
  if (!b.p) {
    /* xstrdup already allocated. The ternary above double-allocates if b.p is null.
     * Keep the empty string from xstrdup. */
  }
  return 0;
}

static int dn_add_ava(Rdn *rdn, const unsigned *comp, int ncomp, const char *text) {
  if (rdn->n == rdn->cap) {
    rdn->cap = rdn->cap ? rdn->cap * 2 : 4;
    rdn->a = realloc(rdn->a, (size_t)rdn->cap * sizeof(Ava));
    if (!rdn->a) {
      die("out of memory");
    }
  }
  Ava *a = &rdn->a[rdn->n++];
  a->ncomp = ncomp;
  for (int i = 0; i < ncomp; i++) {
    a->comp[i] = comp[i];
  }
  a->text = xstrdup(text);
  return 0;
}

static int parse_dn(const unsigned char *p, size_t n, Dn *dn) {
  memset(dn, 0, sizeof *dn);
  if (n == 0) {
    return -1;
  }
  unsigned tag = 0;
  size_t c0 = 0, clen = 0, next = 0;
  if (der_tlv(p, n, 0, &tag, &c0, &clen, &next) || tag != 0x30 || next != n) {
    return -1;
  }
  size_t off = c0;
  size_t end = c0 + clen;
  while (off < end) {
    size_t s0 = 0, slen = 0, snext = 0;
    if (der_tlv(p, n, off, &tag, &s0, &slen, &snext) || tag != 0x31) {
      return -1;
    }
    if (dn->n == dn->cap) {
      dn->cap = dn->cap ? dn->cap * 2 : 4;
      dn->r = realloc(dn->r, (size_t)dn->cap * sizeof(Rdn));
      if (!dn->r) {
        die("out of memory");
      }
    }
    Rdn *rdn = &dn->r[dn->n++];
    memset(rdn, 0, sizeof *rdn);
    size_t ioff = s0;
    size_t iend = s0 + slen;
    while (ioff < iend) {
      size_t a0 = 0, alen = 0, anext = 0;
      if (der_tlv(p, n, ioff, &tag, &a0, &alen, &anext) || tag != 0x30) {
        return -1;
      }
      unsigned comp[16];
      int ncomp = 0;
      size_t onext = 0;
      if (parse_oid(p, n, a0, comp, &ncomp, &onext)) {
        return -1;
      }
      if (onext >= a0 + alen) {
        return -1;
      }
      unsigned stag = 0;
      size_t v0 = 0, vlen = 0, vnext = 0;
      if (der_tlv(p, n, onext, &stag, &v0, &vlen, &vnext) || vnext != a0 + alen) {
        return -1;
      }
      char *text = NULL;
      if (stag == 0x0C || stag == 0x12 || stag == 0x13 || stag == 0x14 || stag == 0x16 ||
          stag == 0x1A) {
        text = xmalloc(vlen + 1);
        memcpy(text, p + v0, vlen);
        text[vlen] = 0;
        for (size_t k = 0; k < vlen; k++) {
          if (stag != 0x0C && p[v0 + k] >= 0x80) {
            free(text);
            return -1;
          }
        }
      } else if (stag == 0x1E) {
        if (utf16be_to_utf8(p + v0, vlen, &text)) {
          return -1;
        }
      } else {
        return -1;
      }
      dn_add_ava(rdn, comp, ncomp, text);
      free(text);
      ioff = anext;
    }
    off = snext;
  }
  return 0;
}

static void free_dn(Dn *dn) {
  for (int i = 0; i < dn->n; i++) {
    for (int j = 0; j < dn->r[i].n; j++) {
      free(dn->r[i].a[j].text);
    }
    free(dn->r[i].a);
  }
  free(dn->r);
}

static char *canon_text(const char *s) {
  size_t n = strlen(s);
  char *o = xmalloc(n + 1);
  size_t w = 0;
  int sp = 0;
  int started = 0;
  for (size_t i = 0; i < n; i++) {
    unsigned char c = (unsigned char)s[i];
    if (c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\v' || c == '\f') {
      if (started) {
        sp = 1;
      }
      continue;
    }
    if (sp) {
      o[w++] = ' ';
      sp = 0;
    }
    if (c >= 'A' && c <= 'Z') {
      c = (unsigned char)(c - 'A' + 'a');
    }
    o[w++] = (char)c;
    started = 1;
  }
  o[w] = 0;
  return o;
}

static void dn_canon(const Dn *dn, Buf *out) {
  memset(out, 0, sizeof *out);
  if (dn->n == 0) {
    return;
  }
  for (int i = 0; i < dn->n; i++) {
    Buf setb = {0};
    for (int j = 0; j < dn->r[i].n; j++) {
      const Ava *a = &dn->r[i].a[j];
      char *text = canon_text(a->text);
      Buf ava = {0};
      der_oid(&ava, a->comp, a->ncomp);
      buf_byte(&ava, 0x0c);
      size_t tn = strlen(text);
      buf_len(&ava, tn);
      buf_add(&ava, text, tn);
      free(text);
      buf_byte(&setb, 0x30);
      buf_len(&setb, ava.n);
      buf_add(&setb, ava.p, ava.n);
      free(ava.p);
    }
    buf_byte(out, 0x31);
    buf_len(out, setb.n);
    buf_add(out, setb.p, setb.n);
    free(setb.p);
  }
}

static char *ava_text(const Ava *a) {
  const char *name = short_name(a->comp, a->ncomp);
  char *oid = NULL;
  const char *left = name;
  if (!left) {
    oid = oid_dotted(a->comp, a->ncomp);
    left = oid;
  }
  size_t n = strlen(left) + 1 + strlen(a->text) + 1;
  char *s = xmalloc(n);
  snprintf(s, n, "%s=%s", left, a->text);
  free(oid);
  return s;
}

static char *dn_last_part(const Dn *dn) {
  if (dn->n == 0) {
    return xstrdup("");
  }
  const Rdn *rdn = &dn->r[dn->n - 1];
  if (rdn->n == 1) {
    return ava_text(&rdn->a[0]);
  }
  Buf b = {0};
  buf_byte(&b, '{');
  for (int i = 0; i < rdn->n; i++) {
    if (i) {
      buf_byte(&b, ',');
    }
    char *one = ava_text(&rdn->a[i]);
    buf_add(&b, one, strlen(one));
    free(one);
  }
  buf_byte(&b, '}');
  return (char *)b.p;
}

static void subject_hash(const unsigned char *dn, size_t n, char out[9]) {
  Dn parsed;
  if (parse_dn(dn, n, &parsed)) {
    die("could not parse a certificate subject");
  }
  Buf canon = {0};
  dn_canon(&parsed, &canon);
  unsigned char dig[20];
  sha1(canon.p, canon.n, dig);
  free(canon.p);
  free_dn(&parsed);
  snprintf(out, 9, "%02x%02x%02x%02x", dig[3], dig[2], dig[1], dig[0]);
}

typedef struct {
  size_t serial_off, serial_len;
  size_t issuer_off, issuer_len;
  size_t subject_off, subject_len;
  size_t spki_off, spki_len;
} CertParts;

static int cert_parts(const unsigned char *cert, size_t n, CertParts *part) {
  unsigned tag = 0;
  size_t c0 = 0, clen = 0, next = 0;
  if (der_tlv(cert, n, 0, &tag, &c0, &clen, &next) || tag != 0x30) {
    return -1;
  }
  size_t t0 = 0, tlen = 0, tnext = 0;
  if (der_tlv(cert, n, c0, &tag, &t0, &tlen, &tnext) || tag != 0x30) {
    return -1;
  }
  size_t p = t0;
  if (p < t0 + tlen && cert[p] == 0xA0) {
    if (der_tlv(cert, n, p, &tag, &c0, &clen, &next)) {
      return -1;
    }
    p = next;
  }
  size_t fields[5][2];
  unsigned expect[5] = {0x02, 0x30, 0x30, 0x30, 0x30};
  for (int i = 0; i < 5; i++) {
    if (der_tlv(cert, n, p, &tag, &c0, &clen, &next) || tag != expect[i]) {
      return -1;
    }
    fields[i][0] = p;
    fields[i][1] = next - p;
    p = next;
  }
  size_t spki = p;
  if (der_tlv(cert, n, p, &tag, &c0, &clen, &next) || tag != 0x30) {
    return -1;
  }
  part->serial_off = fields[0][0];
  part->serial_len = fields[0][1];
  part->issuer_off = fields[2][0];
  part->issuer_len = fields[2][1];
  part->subject_off = fields[4][0];
  part->subject_len = fields[4][1];
  part->spki_off = spki;
  part->spki_len = next - spki;
  return 0;
}

static void trust_clear(int trust[TR_COUNT], int value) {
  for (int i = 0; i < TR_COUNT; i++) {
    trust[i] = value;
  }
}

static int any_purpose(const int trust[TR_COUNT], int value, const Purpose **list, int *nlist) {
  int n = 0;
  for (int i = 0; i < g_npurposes; i++) {
    if (trust[g_purposes[i].trust_index] == value) {
      if (list) {
        list[n] = &g_purposes[i];
      }
      n++;
    }
  }
  if (nlist) {
    *nlist = n;
  }
  return n;
}

static void der_oid_list(Buf *out, unsigned tag, const Purpose **list, int n) {
  Buf body = {0};
  for (int i = 0; i < n; i++) {
    der_oid(&body, list[i]->comp, list[i]->ncomp);
  }
  buf_byte(out, (unsigned char)tag);
  buf_len(out, body.n);
  buf_add(out, body.p, body.n);
  free(body.p);
}

static void der_one_oid_list(Buf *out, unsigned tag, const unsigned *oid, int ncomp) {
  Buf body = {0};
  der_oid(&body, oid, ncomp);
  buf_byte(out, (unsigned char)tag);
  buf_len(out, body.n);
  buf_add(out, body.p, body.n);
  free(body.p);
}

static void openssl_aux(const int trust[TR_COUNT], Buf *out) {
  const Purpose *trusted[9];
  const Purpose *rejected[9];
  int nt = 0, nr = 0;
  any_purpose(trust, T_TRUSTED_DELEGATOR, trusted, &nt);
  any_purpose(trust, T_NOT_TRUSTED, rejected, &nr);
  Buf inner = {0};
  der_oid_list(&inner, 0x30, trusted, nt);
  if (nr) {
    der_oid_list(&inner, 0xA0, rejected, nr);
  }
  memset(out, 0, sizeof *out);
  buf_byte(out, 0x30);
  buf_len(out, inner.n);
  buf_add(out, inner.p, inner.n);
  free(inner.p);
}

static void write_trust_lines(FILE *fp, const int trust[TR_COUNT]) {
  const Purpose *trusted[9];
  const Purpose *rejected[9];
  int nt = 0, nr = 0;
  any_purpose(trust, T_TRUSTED_DELEGATOR, trusted, &nt);
  any_purpose(trust, T_NOT_TRUSTED, rejected, &nr);
  if (nt) {
    fputs("Trusted for:\n", fp);
    for (int i = 0; i < nt; i++) {
      fprintf(fp, "  - %s (%s)\n", trusted[i]->dotted, trusted[i]->name);
    }
  }
  if (nr) {
    fputs("Rejected for:\n", fp);
    for (int i = 0; i < nr; i++) {
      fprintf(fp, "  - %s (%s)\n", rejected[i]->dotted, rejected[i]->name);
    }
  }
}

static void write_plain_or_omit(FILE *fp, const Row *row) {
  fprintf(fp, "%s\n", row->label);
  if (row->has_cert && row->trust[TR_SERVER] == T_TRUSTED_DELEGATOR) {
    write_pem(fp, "CERTIFICATE", row->cert, row->ncert);
    fputc('\n', fp);
  } else if (row->has_cert) {
    fputs("Traditional PEM block omitted: this certificate is not trusted for "
          "authenticating servers.\n",
          fp);
  }
}

static void write_openssl_row(FILE *fp, const Row *row) {
  if (!row->has_cert) {
    return;
  }
  write_plain_or_omit(fp, row);
  write_trust_lines(fp, row->trust);
  Buf aux = {0};
  openssl_aux(row->trust, &aux);
  unsigned char *both = xmalloc(row->ncert + aux.n);
  memcpy(both, row->cert, row->ncert);
  if (aux.n) {
    memcpy(both + row->ncert, aux.p, aux.n);
  }
  write_pem(fp, "TRUSTED CERTIFICATE", both, row->ncert + aux.n);
  fputc('\n', fp);
  free(both);
  free(aux.p);
}

static void write_standard_row(FILE *fp, const Row *row) {
  if (!row->has_cert) {
    return;
  }
  write_plain_or_omit(fp, row);
}

static void der_cert_ext(Buf *out, const unsigned *ext_oid, int noid, const unsigned char *ext,
                         size_t next) {
  Buf body = {0};
  der_oid(&body, ext_oid, noid);
  unsigned char crit[] = {0x01, 0x01, 0xff};
  buf_add(&body, crit, 3);
  buf_byte(&body, 0x04);
  buf_len(&body, next);
  buf_add(&body, ext, next);
  memset(out, 0, sizeof *out);
  buf_byte(out, 0x30);
  buf_len(out, body.n);
  buf_add(out, body.p, body.n);
  free(body.p);
}

static void write_ku_ext(FILE *fp, const Row *row, const char *oid_name, const char *oid_dotted,
                         const unsigned *ext_oid, int noid, const Purpose **usages, int nusage,
                         const unsigned *single, int nsingle, const char *single_dotted,
                         const char *single_name) {
  Buf eku = {0};
  if (nusage >= 0 && usages) {
    Buf body = {0};
    for (int i = 0; i < nusage; i++) {
      der_oid(&body, usages[i]->comp, usages[i]->ncomp);
    }
    buf_byte(&eku, 0x30);
    buf_len(&eku, body.n);
    buf_add(&eku, body.p, body.n);
    free(body.p);
  } else {
    Buf body = {0};
    der_oid(&body, single, nsingle);
    buf_byte(&eku, 0x30);
    buf_len(&eku, body.n);
    buf_add(&eku, body.p, body.n);
    free(body.p);
  }
  Buf ext = {0};
  der_cert_ext(&ext, ext_oid, noid, eku.p, eku.n);
  fprintf(fp, "# %s\n", oid_name);
  fputs("# value = [", fp);
  if (usages) {
    for (int i = 0; i < nusage; i++) {
      if (i) {
        fputs(", ", fp);
      }
      fprintf(fp, "%s (%s)", usages[i]->dotted, usages[i]->name);
    }
  } else {
    fprintf(fp, "%s (%s)", single_dotted, single_name);
  }
  fputs("]\n", fp);
  fputs("[p11-kit-object-v1]\n", fp);
  fputs("label: ", fp);
  quote_bytes(fp, (const unsigned char *)row->label, strlen(row->label));
  fputc('\n', fp);
  fputs("modifiable: false\n", fp);
  fputs("class: x-certificate-extension\n", fp);
  fprintf(fp, "object-id: %s\n", oid_dotted);
  fputs("value: ", fp);
  quote_bytes(fp, ext.p, ext.n);
  fputc('\n', fp);
  CertParts part;
  if (cert_parts(row->cert, row->ncert, &part)) {
    die("could not read the subject public key of %s", row->label);
  }
  write_pem(fp, "PUBLIC KEY", row->cert + part.spki_off, part.spki_len);
  fputc('\n', fp);
  free(eku.p);
  free(ext.p);
}

static void write_p11_row(FILE *fp, const Row *row) {
  fprintf(fp, "# %s\n", row->key);
  if (!row->has_cert) {
    if (!any_purpose(row->trust, T_NOT_TRUSTED, NULL, NULL)) {
      die("trust %s has no certificate and is not distrusted", row->label);
    }
    fputs("[p11-kit-object-v1]\n", fp);
    fputs("label: ", fp);
    quote_bytes(fp, (const unsigned char *)row->label, strlen(row->label));
    fputc('\n', fp);
    fputs("class: certificate\n", fp);
    fputs("certificate-type: x-509\n", fp);
    fputs("modifiable: false\n", fp);
    fputs("issuer: ", fp);
    quote_bytes(fp, row->issuer, row->nissuer);
    fputc('\n', fp);
    fputs("serial-number: ", fp);
    quote_bytes(fp, row->serial, row->nserial);
    fputc('\n', fp);
    fputs("x-distrusted: true\n\n", fp);
    return;
  }
  fputs("[p11-kit-object-v1]\n", fp);
  fputs("label: ", fp);
  quote_bytes(fp, (const unsigned char *)row->label, strlen(row->label));
  fputc('\n', fp);
  fputs("modifiable: false\n", fp);
  if (any_purpose(row->trust, T_NOT_TRUSTED, NULL, NULL)) {
    fputs("x-distrusted: true\n\n", fp);
  } else if (any_purpose(row->trust, T_TRUSTED_DELEGATOR, NULL, NULL)) {
    fputs("trusted: true\n\n", fp);
  } else {
    fputs("trusted: false\n\n", fp);
  }
  int cert_attr = 0;
  if (row->mozilla) {
    fputs("nss-mozilla-ca-policy: true\n", fp);
    cert_attr = 1;
  }
  if (row->nserver_after) {
    fputs("nss-server-distrust-after: ", fp);
    quote_bytes(fp, row->server_after, row->nserver_after);
    fputc('\n', fp);
    cert_attr = 1;
  }
  if (row->nemail_after) {
    fputs("nss-email-distrust-after: ", fp);
    quote_bytes(fp, row->email_after, row->nemail_after);
    fputc('\n', fp);
    cert_attr = 1;
  }
  if (cert_attr) {
    fputc('\n', fp);
  }
  write_pem(fp, "CERTIFICATE", row->cert, row->ncert);
  fputc('\n', fp);

  const Purpose *trusted[9];
  const Purpose *rejected[9];
  int nt = 0, nr = 0;
  any_purpose(row->trust, T_TRUSTED_DELEGATOR, trusted, &nt);
  any_purpose(row->trust, T_NOT_TRUSTED, rejected, &nr);
  if (nr) {
    write_ku_ext(fp, row, "p11kit OpenSSL reject extension", "1.3.6.1.4.1.3319.6.10.1", OID_REJECT,
                 10, rejected, nr, NULL, 0, NULL, NULL);
  }
  if (nt) {
    write_ku_ext(fp, row, "RFC5280: Extended Key Usage X.509 extension", "2.5.29.37", OID_EKU, 4,
                 trusted, nt, NULL, 0, NULL, NULL);
  } else if (nr) {
    write_ku_ext(fp, row, "RFC5280: Extended Key Usage X.509 extension", "2.5.29.37", OID_EKU, 4,
                 NULL, -1, OID_DUMMY, 10, "1.3.6.1.4.1.3319.6.10.16",
                 "p11kit reserved key purpose");
  }
}

static int cmp_row(const void *a, const void *b) {
  const Row *const *ra = a;
  const Row *const *rb = b;
  return strcmp((*ra)->key, (*rb)->key);
}

static void write_bundle(const char *path, Row **rows, int n, int standard) {
  if (!path) {
    return;
  }
  FILE *fp = fopen(path, "wb");
  if (!fp) {
    die("cannot write %s: %s", path, strerror(errno));
  }
  for (int i = 0; i < n; i++) {
    if (standard) {
      write_standard_row(fp, rows[i]);
    } else {
      write_openssl_row(fp, rows[i]);
    }
  }
  if (fclose(fp)) {
    die("cannot close %s", path);
  }
}

static void write_p11(const char *path, Row **rows, int n) {
  if (!path) {
    return;
  }
  FILE *fp = fopen(path, "wb");
  if (!fp) {
    die("cannot write %s: %s", path, strerror(errno));
  }
  for (int i = 0; i < n; i++) {
    write_p11_row(fp, rows[i]);
  }
  if (fclose(fp)) {
    die("cannot close %s", path);
  }
}

static char *join_path(const char *dir, const char *name) {
  size_t n = strlen(dir) + 1 + strlen(name) + 1;
  char *s = xmalloc(n);
  snprintf(s, n, "%s/%s", dir, name);
  return s;
}

static void write_dir(const char *dir, Row **rows, int n, int hashed) {
  if (!dir) {
    return;
  }
  typedef struct {
    char hash[9];
    char *filename;
  } Link;
  Link *links = NULL;
  int nlinks = 0;
  int cap = 0;
  for (int i = 0; i < n; i++) {
    size_t fn = strlen(rows[i]->key) + 5;
    char *name = xmalloc(fn);
    snprintf(name, fn, "%s.crt", rows[i]->key);
    char *path = join_path(dir, name);
    FILE *fp = fopen(path, "wb");
    if (!fp) {
      die("cannot write %s: %s", path, strerror(errno));
    }
    write_openssl_row(fp, rows[i]);
    if (fclose(fp)) {
      die("cannot close %s", path);
    }
    free(path);
    if (hashed && rows[i]->has_cert) {
      if (nlinks == cap) {
        cap = cap ? cap * 2 : 16;
        links = realloc(links, (size_t)cap * sizeof(Link));
        if (!links) {
          die("out of memory");
        }
      }
      subject_hash(rows[i]->subject, rows[i]->nsubject, links[nlinks].hash);
      links[nlinks].filename = name;
      nlinks++;
    } else {
      free(name);
    }
  }
  if (!hashed) {
    free(links);
    return;
  }
  for (int i = 0; i < nlinks; i++) {
    for (int j = i + 1; j < nlinks; j++) {
      int c = strcmp(links[i].hash, links[j].hash);
      if (c == 0) {
        c = strcmp(links[i].filename, links[j].filename);
      }
      if (c > 0) {
        Link tmp = links[i];
        links[i] = links[j];
        links[j] = tmp;
      }
    }
  }
  int i = 0;
  while (i < nlinks) {
    int j = i + 1;
    while (j < nlinks && strcmp(links[i].hash, links[j].hash) == 0) {
      j++;
    }
    if (j - i > 256) {
      die("too many certificates share subject hash %s", links[i].hash);
    }
    for (int k = i; k < j; k++) {
      char linkname[32];
      snprintf(linkname, sizeof linkname, "%s.%d", links[i].hash, k - i);
      char *path = join_path(dir, linkname);
      if (symlink(links[k].filename, path)) {
        die("cannot symlink %s: %s", path, strerror(errno));
      }
      free(path);
    }
    i = j;
  }
  for (int k = 0; k < nlinks; k++) {
    free(links[k].filename);
  }
  free(links);
}

static Av *find_av(Obj *o, const char *key) {
  for (size_t i = 0; i < o->n; i++) {
    if (strcmp(o->a[i].key, key) == 0) {
      return &o->a[i];
    }
  }
  return NULL;
}

static Av *need_av(Obj *o, const char *key) {
  Av *a = find_av(o, key);
  if (!a) {
    die("certdata object missing %s", key);
  }
  return a;
}

static Row *row_new(void) {
  Row *r = xmalloc(sizeof *r);
  memset(r, 0, sizeof *r);
  trust_clear(r->trust, T_UNKNOWN);
  return r;
}

static Row *find_row(Row **rows, int n, const char *key) {
  for (int i = 0; i < n; i++) {
    if (strcmp(rows[i]->key, key) == 0) {
      return rows[i];
    }
  }
  return NULL;
}

static void rows_add(Row ***rows, int *n, int *cap, Row *row) {
  Row *old = find_row(*rows, *n, row->key);
  if (old) {
    if (row->has_cert) {
      free(old->subject);
      free(old->cert);
      free(old->server_after);
      free(old->email_after);
      old->subject = row->subject;
      old->nsubject = row->nsubject;
      old->cert = row->cert;
      old->ncert = row->ncert;
      old->mozilla = row->mozilla;
      old->server_after = row->server_after;
      old->nserver_after = row->nserver_after;
      old->email_after = row->email_after;
      old->nemail_after = row->nemail_after;
      old->has_cert = 1;
      row->subject = NULL;
      row->cert = NULL;
      row->server_after = NULL;
      row->email_after = NULL;
    }
    if (row->has_trust) {
      memcpy(old->trust, row->trust, sizeof old->trust);
      old->has_trust = 1;
    }
    if (!old->label) {
      old->label = row->label;
      row->label = NULL;
    }
    if (!old->serial) {
      old->serial = row->serial;
      old->nserial = row->nserial;
      old->issuer = row->issuer;
      old->nissuer = row->nissuer;
      row->serial = NULL;
      row->issuer = NULL;
    }
    free(row->key);
    free(row->label);
    free(row->serial);
    free(row->issuer);
    free(row->subject);
    free(row->cert);
    free(row->server_after);
    free(row->email_after);
    free(row);
    return;
  }
  if (*n == *cap) {
    *cap = *cap ? *cap * 2 : 32;
    *rows = realloc(*rows, (size_t)*cap * sizeof(Row *));
    if (!*rows) {
      die("out of memory");
    }
  }
  (*rows)[(*n)++] = row;
}

static int trust_from_token(const char *v) {
  if (strncmp(v, "CKT_NSS_", 8) != 0) {
    die("unknown trust value %s", v);
  }
  const char *s = v + 8;
  if (strcmp(s, "TRUSTED_DELEGATOR") == 0) {
    return T_TRUSTED_DELEGATOR;
  }
  if (strcmp(s, "MUST_VERIFY_TRUST") == 0) {
    return T_MUST_VERIFY;
  }
  if (strcmp(s, "NOT_TRUSTED") == 0) {
    return T_NOT_TRUSTED;
  }
  if (strcmp(s, "TRUSTED") == 0) {
    return T_TRUSTED;
  }
  if (strcmp(s, "UNKNOWN") == 0) {
    return T_UNKNOWN;
  }
  die("unknown trust value %s", v);
  return T_UNKNOWN;
}

static void add_obj(Obj *o, Row ***rows, int *n, int *cap) {
  Av *cls = need_av(o, "CKA_CLASS");
  if (cls->kind != AV_CLASS) {
    die("CKA_CLASS has the wrong type");
  }
  if (cls->num == C_BUILTIN) {
    return;
  }
  Av *label = need_av(o, "CKA_LABEL");
  Av *serial = need_av(o, "CKA_SERIAL_NUMBER");
  Av *issuer = need_av(o, "CKA_ISSUER");
  if (label->kind != AV_STR || serial->kind != AV_BYTES || issuer->kind != AV_BYTES) {
    die("certdata label, serial, or issuer has the wrong type");
  }
  Row *row = row_new();
  row->label = xstrdup(label->str);
  row->serial = memdup(serial->bytes, serial->nbytes);
  row->nserial = serial->nbytes;
  row->issuer = memdup(issuer->bytes, issuer->nbytes);
  row->nissuer = issuer->nbytes;
  row->key = make_key(row->label, row->serial, row->nserial);
  if (cls->num == C_CERT) {
    Av *subject = need_av(o, "CKA_SUBJECT");
    Av *value = need_av(o, "CKA_VALUE");
    if (subject->kind != AV_BYTES || value->kind != AV_BYTES) {
      die("certdata subject or value has the wrong type");
    }
    row->subject = memdup(subject->bytes, subject->nbytes);
    row->nsubject = subject->nbytes;
    row->cert = memdup(value->bytes, value->nbytes);
    row->ncert = value->nbytes;
    Av *pol = find_av(o, "CKA_NSS_MOZILLA_CA_POLICY");
    if (pol && pol->kind == AV_BOOL) {
      row->mozilla = pol->num;
    }
    Av *srv = find_av(o, "CKA_NSS_SERVER_DISTRUST_AFTER");
    if (srv && srv->kind == AV_BYTES && srv->nbytes) {
      row->server_after = memdup(srv->bytes, srv->nbytes);
      row->nserver_after = srv->nbytes;
    }
    Av *eml = find_av(o, "CKA_NSS_EMAIL_DISTRUST_AFTER");
    if (eml && eml->kind == AV_BYTES && eml->nbytes) {
      row->email_after = memdup(eml->bytes, eml->nbytes);
      row->nemail_after = eml->nbytes;
    }
    row->has_cert = 1;
    rows_add(rows, n, cap, row);
    return;
  }
  if (cls->num != C_TRUST) {
    die("unknown certdata object class");
  }
  trust_clear(row->trust, T_UNKNOWN);
  struct {
    const char *key;
    int idx;
    int required;
  } fields[] = {
      {"CKA_TRUST_SERVER_AUTH", TR_SERVER, 1},
      {"CKA_TRUST_CODE_SIGNING", TR_CODE, 1},
      {"CKA_TRUST_EMAIL_PROTECTION", TR_EMAIL, 1},
      {"CKA_TRUST_CLIENT_AUTH", TR_CLIENT, 0},
      {"CKA_TRUST_DIGITAL_SIGNATURE", TR_DIGITAL, 0},
      {"CKA_TRUST_NON_REPUDIATION", TR_NONREP, 0},
      {"CKA_TRUST_KEY_ENCIPHERMENT", TR_KEYENC, 0},
      {"CKA_TRUST_DATA_ENCIPHERMENT", TR_DATAENC, 0},
      {"CKA_TRUST_KEY_AGREEMENT", TR_KEYAGREE, 0},
      {"CKA_TRUST_KEY_CERT_SIGN", TR_KEYCERT, 0},
      {"CKA_TRUST_CRL_SIGN", TR_CRLSIGN, 0},
      {"CKA_TRUST_IPSEC_END_SYSTEM", TR_IPSEC_END, 0},
      {"CKA_TRUST_IPSEC_TUNNEL", TR_IPSEC_TUN, 0},
      {"CKA_TRUST_IPSEC_USER", TR_IPSEC_USER, 0},
      {"CKA_TRUST_TIME_STAMPING", TR_TIME, 0},
  };
  for (size_t i = 0; i < sizeof fields / sizeof fields[0]; i++) {
    Av *a = find_av(o, fields[i].key);
    if (!a) {
      if (fields[i].required) {
        die("trust object missing %s", fields[i].key);
      }
      continue;
    }
    if (a->kind != AV_TRUST) {
      die("%s has the wrong type", fields[i].key);
    }
    row->trust[fields[i].idx] = a->num;
  }
  if (!find_av(o, "CKA_TRUST_STEP_UP_APPROVED")) {
    die("trust object missing CKA_TRUST_STEP_UP_APPROVED");
  }
  row->has_trust = 1;
  rows_add(rows, n, cap, row);
}

static void decode_escapes(const char *s, unsigned char **out, size_t *outn) {
  Buf b = {0};
  const unsigned char *p = (const unsigned char *)s;
  while (*p) {
    if (*p != '\\') {
      buf_byte(&b, *p++);
      continue;
    }
    p++;
    if (*p == 0) {
      die("truncated escape in certdata");
    }
    if (*p >= '0' && *p <= '7') {
      int v = 0;
      int k = 0;
      while (k < 3 && *p >= '0' && *p <= '7') {
        v = (v << 3) | (*p - '0');
        p++;
        k++;
      }
      if (v > 255) {
        die("certdata escape does not fit in one byte");
      }
      buf_byte(&b, (unsigned char)v);
      continue;
    }
    if (*p == 'n') {
      buf_byte(&b, '\n');
      p++;
      continue;
    }
    if (*p == 'r') {
      buf_byte(&b, '\r');
      p++;
      continue;
    }
    if (*p == 't') {
      buf_byte(&b, '\t');
      p++;
      continue;
    }
    if (*p == '\\' || *p == '\'' || *p == '"') {
      buf_byte(&b, *p++);
      continue;
    }
    if (*p == 'x' && isxdigit((unsigned char)p[1]) && isxdigit((unsigned char)p[2])) {
      char tmp[3] = {(char)p[1], (char)p[2], 0};
      buf_byte(&b, (unsigned char)strtol(tmp, NULL, 16));
      p += 3;
      continue;
    }
    die("unknown escape in certdata");
  }
  if (!b.p) {
    b.p = xmalloc(1);
    b.p[0] = 0;
    b.n = 0;
  }
  *out = b.p;
  *outn = b.n;
}

static void obj_add_av(Obj *o, const char *key, const char *type, const char *value) {
  if (o->n == o->cap) {
    o->cap = o->cap ? o->cap * 2 : 8;
    o->a = realloc(o->a, o->cap * sizeof(Av));
    if (!o->a) {
      die("out of memory");
    }
  }
  Av *a = &o->a[o->n++];
  memset(a, 0, sizeof *a);
  a->key = xstrdup(key);
  if (strcmp(type, "CK_BBOOL") == 0) {
    a->kind = AV_BOOL;
    if (strcmp(value, "CK_TRUE") == 0) {
      a->num = 1;
    } else if (strcmp(value, "CK_FALSE") == 0) {
      a->num = 0;
    } else {
      die("unknown boolean %s", value);
    }
  } else if (strcmp(type, "CK_OBJECT_CLASS") == 0) {
    a->kind = AV_CLASS;
    if (strcmp(value, "CKO_CERTIFICATE") == 0) {
      a->num = C_CERT;
    } else if (strcmp(value, "CKO_NSS_TRUST") == 0) {
      a->num = C_TRUST;
    } else if (strcmp(value, "CKO_NSS_BUILTIN_ROOT_LIST") == 0) {
      a->num = C_BUILTIN;
    } else {
      die("unknown object class %s", value);
    }
  } else if (strcmp(type, "UTF8") == 0) {
    a->kind = AV_STR;
    a->str = xstrdup(value);
  } else if (strcmp(type, "CK_CERTIFICATE_TYPE") == 0) {
    a->kind = AV_STR;
    if (strcmp(value, "CKC_X_509") != 0) {
      die("unknown certificate type %s", value);
    }
    a->str = xstrdup(value);
  } else if (strcmp(type, "CK_TRUST") == 0) {
    a->kind = AV_TRUST;
    a->num = trust_from_token(value);
  } else if (strcmp(type, "MULTILINE_OCTAL") == 0) {
    a->kind = AV_BYTES;
    decode_escapes(value, &a->bytes, &a->nbytes);
  } else {
    die("unknown certdata type %s", type);
  }
}

static char *strip_line(char *s) {
  while (*s == ' ' || *s == '\t' || *s == '\r') {
    s++;
  }
  size_t n = strlen(s);
  while (n && (s[n - 1] == ' ' || s[n - 1] == '\t' || s[n - 1] == '\r')) {
    s[--n] = 0;
  }
  return s;
}

static void parse_certdata(const char *text, Row ***rows, int *n, int *cap) {
  enum { ST_WAIT, ST_ATTR, ST_TYPE, ST_VALUE, ST_MULTI };
  int state = ST_WAIT;
  Obj *cur = NULL;
  char *cur_attr = NULL;
  char *cur_type = NULL;
  Buf cur_val = {0};
  Obj **objs = NULL;
  int nobj = 0, capobj = 0;

  void flush_attr(void) {
    if (!cur_attr) {
      return;
    }
    if (!cur || !cur_type) {
      die("certdata attribute outside an object");
    }
    obj_add_av(cur, cur_attr, cur_type, (char *)(cur_val.p ? cur_val.p : (unsigned char *)""));
    free(cur_attr);
    free(cur_type);
    cur_attr = NULL;
    cur_type = NULL;
    free(cur_val.p);
    cur_val.p = NULL;
    cur_val.n = 0;
    cur_val.cap = 0;
  }

  void push_obj(void) {
    if (!cur || cur->n == 0) {
      if (cur) {
        free(cur->a);
        free(cur);
      }
      cur = NULL;
      return;
    }
    if (nobj == capobj) {
      capobj = capobj ? capobj * 2 : 16;
      objs = realloc(objs, (size_t)capobj * sizeof(Obj *));
      if (!objs) {
        die("out of memory");
      }
    }
    objs[nobj++] = cur;
    cur = NULL;
  }

  void start_object(void) {
    flush_attr();
    push_obj();
    cur = xmalloc(sizeof *cur);
    memset(cur, 0, sizeof *cur);
  }

  for (char *p = (char *)text; *p;) {
    char *nl = strchr(p, '\n');
    if (nl) {
      *nl = 0;
    }
    char *line = strip_line(p);
    if (line[0] != '#') {
      char *q = line;
      while (*q) {
        while (*q == ' ' || *q == '\t') {
          q++;
        }
        if (!*q) {
          break;
        }
        char *tok = NULL;
        if (*q == '"') {
          q++;
          Buf b = {0};
          while (*q && *q != '"') {
            if (*q == '\\' && q[1] == '"') {
              buf_byte(&b, '"');
              q += 2;
              continue;
            }
            buf_byte(&b, (unsigned char)*q++);
          }
          if (*q == '"') {
            q++;
          }
          tok = b.p ? (char *)b.p : xstrdup("");
          if (!b.p) {
            /* xstrdup result is tok. */
          }
        } else {
          char *start = q;
          while (*q && *q != ' ' && *q != '\t') {
            q++;
          }
          char saved = *q;
          *q = 0;
          tok = xstrdup(start);
          *q = saved;
        }
        if (state == ST_WAIT) {
          if (strcmp(tok, "BEGINDATA") == 0) {
            start_object();
            state = ST_ATTR;
          }
        } else if (state == ST_ATTR) {
          if (strcmp(tok, "CKA_CLASS") == 0) {
            start_object();
          }
          cur_attr = xstrdup(tok);
          state = ST_TYPE;
        } else if (state == ST_TYPE) {
          cur_type = xstrdup(tok);
          state = strcmp(tok, "MULTILINE_OCTAL") == 0 ? ST_MULTI : ST_VALUE;
        } else if (state == ST_VALUE) {
          buf_add(&cur_val, tok, strlen(tok));
          flush_attr();
          state = ST_ATTR;
        } else if (state == ST_MULTI) {
          if (strcmp(tok, "END") == 0) {
            flush_attr();
            state = ST_ATTR;
          } else {
            buf_add(&cur_val, tok, strlen(tok));
          }
        }
        free(tok);
      }
    }
    if (!nl) {
      break;
    }
    p = nl + 1;
  }
  flush_attr();
  push_obj();
  for (int i = 0; i < nobj; i++) {
    add_obj(objs[i], rows, n, cap);
    for (size_t k = 0; k < objs[i]->n; k++) {
      free(objs[i]->a[k].key);
      free(objs[i]->a[k].str);
      free(objs[i]->a[k].bytes);
    }
    free(objs[i]->a);
    free(objs[i]);
  }
  free(objs);
}

static char *slurp(const char *path, size_t *outn) {
  FILE *fp = fopen(path, "rb");
  if (!fp) {
    die("cannot read %s: %s", path, strerror(errno));
  }
  if (fseek(fp, 0, SEEK_END)) {
    die("cannot seek %s", path);
  }
  long sz = ftell(fp);
  if (sz < 0) {
    die("cannot size %s", path);
  }
  if (fseek(fp, 0, SEEK_SET)) {
    die("cannot rewind %s", path);
  }
  char *buf = xmalloc((size_t)sz + 1);
  size_t got = fread(buf, 1, (size_t)sz, fp);
  fclose(fp);
  buf[got] = 0;
  if (outn) {
    *outn = got;
  }
  return buf;
}

static unsigned char *b64_decode(const char *s, size_t n, size_t *outn) {
  Buf b = {0};
  int val = 0;
  int bits = 0;
  for (size_t i = 0; i < n; i++) {
    unsigned char c = (unsigned char)s[i];
    if (c == '=' || c == '\n' || c == '\r' || c == ' ' || c == '\t') {
      continue;
    }
    int d = b64_val(c);
    if (d < 0) {
      free(b.p);
      return NULL;
    }
    val = (val << 6) | d;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      buf_byte(&b, (unsigned char)((val >> bits) & 0xff));
    }
  }
  *outn = b.n;
  if (!b.p) {
    b.p = xmalloc(1);
    b.p[0] = 0;
    b.n = 0;
    *outn = 0;
  }
  return b.p;
}

static int parse_oid_list(const unsigned char *p, size_t n, unsigned comp[][16], int *ncomp,
                          int maxn, int *count) {
  size_t off = 0;
  *count = 0;
  while (off < n) {
    if (*count >= maxn) {
      return -1;
    }
    size_t next = 0;
    if (parse_oid(p, n, off, comp[*count], &ncomp[*count], &next)) {
      return -1;
    }
    (*count)++;
    off = next;
  }
  return 0;
}

static void trust_from_aux(const unsigned char *aux, size_t n, int trust[TR_COUNT]) {
  trust_clear(trust, T_UNKNOWN);
  unsigned tag = 0;
  size_t c0 = 0, clen = 0, next = 0;
  if (der_tlv(aux, n, 0, &tag, &c0, &clen, &next) || tag != 0x30) {
    die("trusted certificate auxiliary is not a sequence");
  }
  size_t off = c0;
  size_t end = c0 + clen;
  if (off >= end) {
    return;
  }
  size_t t0 = 0, tlen = 0, tnext = 0;
  if (der_tlv(aux, n, off, &tag, &t0, &tlen, &tnext) || tag != 0x30) {
    die("trusted certificate is missing the trust list");
  }
  unsigned comps[16][16];
  int ncomps[16];
  int count = 0;
  if (parse_oid_list(aux + t0, tlen, comps, ncomps, 16, &count)) {
    die("could not parse trust oids");
  }
  for (int i = 0; i < count; i++) {
    const Purpose *p = purpose_by_oid(comps[i], ncomps[i]);
    if (p) {
      trust[p->trust_index] = T_TRUSTED_DELEGATOR;
    }
  }
  off = tnext;
  if (off < end && aux[off] == 0xA0) {
    size_t r0 = 0, rlen = 0, rnext = 0;
    if (der_tlv(aux, n, off, &tag, &r0, &rlen, &rnext) || tag != 0xA0) {
      die("could not parse reject oids");
    }
    count = 0;
    if (parse_oid_list(aux + r0, rlen, comps, ncomps, 16, &count)) {
      die("could not parse reject oids");
    }
    for (int i = 0; i < count; i++) {
      const Purpose *p = purpose_by_oid(comps[i], ncomps[i]);
      if (p) {
        trust[p->trust_index] = T_NOT_TRUSTED;
      }
    }
  }
}

static void add_pem_der(const unsigned char *der, size_t nder, const int trust_in[TR_COUNT],
                        Row ***rows, int *n, int *cap) {
  CertParts part;
  if (cert_parts(der, nder, &part)) {
    die("could not parse an extra certificate");
  }
  Dn dn;
  if (parse_dn(der + part.subject_off, part.subject_len, &dn)) {
    die("could not parse an extra certificate subject");
  }
  unsigned char dig[32];
  sha256(der, nder, dig);
  char hex[65];
  hex_of(dig, 32, hex);
  char *label = NULL;
  if (dn.n == 0) {
    label = xstrdup(hex);
  } else {
    char *last = dn_last_part(&dn);
    size_t ln = strlen(last) + 1 + 8 + 1;
    label = xmalloc(ln);
    snprintf(label, ln, "%s:%.8s", last, hex);
    free(last);
  }
  free_dn(&dn);
  Row *row = row_new();
  row->label = label;
  row->serial = memdup(der + part.serial_off, part.serial_len);
  row->nserial = part.serial_len;
  row->issuer = memdup(der + part.issuer_off, part.issuer_len);
  row->nissuer = part.issuer_len;
  row->subject = memdup(der + part.subject_off, part.subject_len);
  row->nsubject = part.subject_len;
  row->cert = memdup(der, nder);
  row->ncert = nder;
  row->key = make_key(row->label, row->serial, row->nserial);
  memcpy(row->trust, trust_in, sizeof row->trust);
  row->has_cert = 1;
  row->has_trust = 1;
  rows_add(rows, n, cap, row);
}

static void add_pem_text(const char *text, Row ***rows, int *n, int *cap) {
  const char *p = text;
  while ((p = strstr(p, "-----BEGIN ")) != NULL) {
    const char *name = p + 11;
    const char *endname = strstr(name, "-----");
    if (!endname) {
      die("pem block is missing its end marker");
    }
    size_t namelen = (size_t)(endname - name);
    char namebuf[64];
    if (namelen == 0 || namelen >= sizeof namebuf) {
      die("pem label is too long");
    }
    memcpy(namebuf, name, namelen);
    namebuf[namelen] = 0;
    const char *body = endname + 5;
    if (*body == '\r') {
      body++;
    }
    if (*body == '\n') {
      body++;
    }
    char endmark[80];
    snprintf(endmark, sizeof endmark, "-----END %s-----", namebuf);
    const char *end = strstr(body, endmark);
    if (!end) {
      die("pem block %s is not closed", namebuf);
    }
    size_t der_n = 0;
    unsigned char *der = b64_decode(body, (size_t)(end - body), &der_n);
    if (!der) {
      die("pem block %s is not base64", namebuf);
    }
    if (strcmp(namebuf, "CERTIFICATE") == 0) {
      int trust[TR_COUNT];
      trust_clear(trust, T_UNKNOWN);
      trust[TR_SERVER] = T_TRUSTED_DELEGATOR;
      trust[TR_CLIENT] = T_TRUSTED_DELEGATOR;
      trust[TR_CODE] = T_TRUSTED_DELEGATOR;
      add_pem_der(der, der_n, trust, rows, n, cap);
    } else if (strcmp(namebuf, "TRUSTED CERTIFICATE") == 0) {
      CertParts part;
      if (cert_parts(der, der_n, &part)) {
        die("could not parse a trusted certificate");
      }
      /* The auxiliary follows the certificate sequence. */
      unsigned tag = 0;
      size_t c0 = 0, clen = 0, next = 0;
      if (der_tlv(der, der_n, 0, &tag, &c0, &clen, &next) || tag != 0x30) {
        die("trusted certificate is not a sequence");
      }
      int trust[TR_COUNT];
      trust_from_aux(der + next, der_n - next, trust);
      add_pem_der(der, next, trust, rows, n, cap);
    }
    free(der);
    p = end + strlen(endmark);
  }
}

static void add_bundle_path(const char *path, Row ***rows, int *n, int *cap) {
  struct stat st;
  if (stat(path, &st)) {
    die("bundle not found: %s", path);
  }
  if (S_ISDIR(st.st_mode)) {
    DIR *dir = opendir(path);
    if (!dir) {
      die("cannot open %s", path);
    }
    char **names = NULL;
    int nn = 0, c = 0;
    struct dirent *de;
    while ((de = readdir(dir)) != NULL) {
      if (strcmp(de->d_name, ".") == 0 || strcmp(de->d_name, "..") == 0) {
        continue;
      }
      if (nn == c) {
        c = c ? c * 2 : 8;
        names = realloc(names, (size_t)c * sizeof(char *));
        if (!names) {
          die("out of memory");
        }
      }
      names[nn++] = xstrdup(de->d_name);
    }
    closedir(dir);
    for (int i = 0; i < nn; i++) {
      for (int j = i + 1; j < nn; j++) {
        if (strcmp(names[i], names[j]) > 0) {
          char *tmp = names[i];
          names[i] = names[j];
          names[j] = tmp;
        }
      }
    }
    for (int i = 0; i < nn; i++) {
      char *full = join_path(path, names[i]);
      struct stat one;
      if (!stat(full, &one) && S_ISREG(one.st_mode)) {
        char *text = slurp(full, NULL);
        add_pem_text(text, rows, n, cap);
        free(text);
      }
      free(full);
      free(names[i]);
    }
    free(names);
    return;
  }
  char *text = slurp(path, NULL);
  add_pem_text(text, rows, n, cap);
  free(text);
}

static int is_opt(const char *s) {
  return s[0] == '-' && s[1] == '-';
}

static void push_str(char ***list, int *n, const char *s) {
  *list = realloc(*list, (size_t)(*n + 1) * sizeof(char *));
  if (!*list) {
    die("out of memory");
  }
  (*list)[(*n)++] = xstrdup(s);
}

int main(int argc, char **argv) {
  init_purposes();
  init_shorts();
  char **certdata = NULL;
  int ncert = 0;
  char **bundles = NULL;
  int nbundles = 0;
  const char *blocklist = NULL;
  const char *p11 = NULL;
  const char *bundle_out = NULL;
  const char *standard_out = NULL;
  const char *unpacked = NULL;
  const char *hashed = NULL;

  for (int i = 1; i < argc; i++) {
    const char *a = argv[i];
    if (!is_opt(a)) {
      die("unexpected argument %s", a);
    }
    if (strcmp(a, "--certdata_input") == 0 || strcmp(a, "--ca_bundle_input") == 0) {
      int bundle = strcmp(a, "--ca_bundle_input") == 0;
      while (i + 1 < argc && !is_opt(argv[i + 1])) {
        i++;
        if (bundle) {
          push_str(&bundles, &nbundles, argv[i]);
        } else {
          push_str(&certdata, &ncert, argv[i]);
        }
      }
      continue;
    }
    if (i + 1 >= argc || is_opt(argv[i + 1])) {
      die("%s needs a path", a);
    }
    const char *val = argv[++i];
    if (strcmp(a, "--blocklist") == 0 || strcmp(a, "--blocklist_input") == 0) {
      blocklist = val;
    } else if (strcmp(a, "--p11kit_output") == 0) {
      p11 = val;
    } else if (strcmp(a, "--ca_bundle_output") == 0) {
      bundle_out = val;
    } else if (strcmp(a, "--ca_standard_bundle_output") == 0) {
      standard_out = val;
    } else if (strcmp(a, "--ca_unpacked_output") == 0) {
      unpacked = val;
    } else if (strcmp(a, "--ca_hashed_unpacked_output") == 0) {
      hashed = val;
    } else {
      die("unknown option %s", a);
    }
  }
  if (ncert == 0 && nbundles == 0) {
    fputs("buildcatrust: pass --certdata_input or --ca_bundle_input\n", stderr);
    return 1;
  }
  if (!p11 && !bundle_out && !standard_out && !unpacked && !hashed) {
    fputs("buildcatrust: pass an output path\n", stderr);
    return 1;
  }

  Row **rows = NULL;
  int nrows = 0, cap = 0;
  for (int i = 0; i < ncert; i++) {
    char *text = slurp(certdata[i], NULL);
    parse_certdata(text, &rows, &nrows, &cap);
    free(text);
  }
  for (int i = 0; i < nbundles; i++) {
    add_bundle_path(bundles[i], &rows, &nrows, &cap);
  }

  int missing = 0;
  for (int i = 0; i < nrows; i++) {
    if (rows[i]->has_cert && !rows[i]->has_trust) {
      if (!missing) {
        fputs("Certs without trusts:", stderr);
      }
      fprintf(stderr, " %s", rows[i]->key);
      missing = 1;
    }
  }
  if (missing) {
    fputc('\n', stderr);
    return 2;
  }

  char **blocked = NULL;
  int nblocked = 0;
  if (blocklist) {
    char *text = slurp(blocklist, NULL);
    for (char *p = text; *p;) {
      char *nl = strchr(p, '\n');
      if (nl) {
        *nl = 0;
      }
      char *line = strip_line(p);
      if (line[0] && line[0] != '#') {
        int seen = 0;
        for (int i = 0; i < nblocked; i++) {
          if (strcmp(blocked[i], line) == 0) {
            seen = 1;
          }
        }
        if (!seen) {
          push_str(&blocked, &nblocked, line);
        }
      }
      if (!nl) {
        break;
      }
      p = nl + 1;
    }
    free(text);
  }
  char **saw = NULL;
  int nsaw = 0;
  for (int i = 0; i < nrows; i++) {
    if (!rows[i]->has_trust) {
      continue;
    }
    for (int b = 0; b < nblocked; b++) {
      if (strcmp(blocked[b], rows[i]->key) == 0 || strcmp(blocked[b], rows[i]->label) == 0) {
        trust_clear(rows[i]->trust, T_NOT_TRUSTED);
        int already = 0;
        for (int s = 0; s < nsaw; s++) {
          if (strcmp(saw[s], blocked[b]) == 0) {
            already = 1;
          }
        }
        if (!already) {
          push_str(&saw, &nsaw, blocked[b]);
        }
      }
    }
  }
  int unseen = 0;
  for (int b = 0; b < nblocked; b++) {
    int ok = 0;
    for (int s = 0; s < nsaw; s++) {
      if (strcmp(saw[s], blocked[b]) == 0) {
        ok = 1;
      }
    }
    if (!ok) {
      if (!unseen) {
        fputs("Certs in blocklist but not in cert store:", stderr);
      }
      fprintf(stderr, " %s", blocked[b]);
      unseen = 1;
    }
  }
  if (unseen) {
    fputc('\n', stderr);
    return 3;
  }

  Row **out = xmalloc((size_t)nrows * sizeof(Row *));
  int nout = 0;
  for (int i = 0; i < nrows; i++) {
    if (rows[i]->has_trust) {
      out[nout++] = rows[i];
    }
  }
  qsort(out, (size_t)nout, sizeof(Row *), cmp_row);

  write_bundle(bundle_out, out, nout, 0);
  write_bundle(standard_out, out, nout, 1);
  write_dir(unpacked, out, nout, 0);
  write_dir(hashed, out, nout, 1);
  write_p11(p11, out, nout);
  return 0;
}
