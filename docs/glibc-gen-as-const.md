# glibc gen-as-const

glibc 2.44 builds assembly constants with `scripts/gen-as-const.py`.
This repo replaces that program with `surmount-glibc-gen-as-const`.
The makefile calls the Rust binary. It does not call python3 on
`scripts/gen-as-const.py`.

The script quoted here is the file in the release tarball
`glibc-2.44.tar.xz` (120 lines), the same file as the glibc 2.44 tag.
Public copies:

1. [glibc 2.44 tarball](https://ftp.gnu.org/gnu/glibc/glibc-2.44.tar.xz)
   (accessed: 2026-11-25).
2. [scripts/gen-as-const.py at glibc-2.44](https://gitlab.com/gnutools/glibc/-/blob/glibc-2.44/scripts/gen-as-const.py)
   (accessed: 2026-11-25).
3. [scripts/glibcextract.py at glibc-2.44](https://gitlab.com/gnutools/glibc/-/blob/glibc-2.44/scripts/glibcextract.py)
   (accessed: 2026-11-25).

nixpkgs commit `2786a2a72088ecf32aea5b3ec52099e6a7cff78f` fetches that
tarball. `pkgs/development/libraries/glibc/common.nix` sets
`version = "2.44"`. The patch suffix on that expression does not change
the three `$(PYTHON)` recipes in `Makerules`.

## Who calls it

`Makerules` in the 2.44 tarball has three recipes.

1. Lines 251-253. `$(PYTHON) $(py-const-script) --python` with
   `--cc="$(CC) $(CFLAGS) $(CPPFLAGS)"` and the `.pysym` file. The
   recipe writes `$@.tmp`, then renames it to the `.py` file.
   `py-const-script` is `$(..)scripts/gen-as-const.py` (line 235).
2. Lines 269-274. The header rule depends on
   `$(..)scripts/gen-as-const.py` and the `.sym` file. The recipe is
   `$(PYTHON) $< --cc="$(CC) $(CFLAGS) $(CPPFLAGS) -DGEN_AS_CONST_HEADERS ..."`
   and the `.sym` file. It writes `$(@:.h.d=.h)T`, which the next lines
   rename to the `.h` file. `before-compile` adds those headers
   (line 281).
3. Lines 285-288. The internal test recipe runs
   `$(PYTHON) $< --test` on the `.sym` file and writes
   `test-as-const-%.c`. `$<` is the Python script because it is the
   first prerequisite.

Nix replaces those three `$(PYTHON)` invocations with
`surmount-glibc-gen-as-const`. The binary name is the Cargo package
name. `--test` still wins when `--python` is also set, matching
`main` lines 106-117.

## What the program writes

A `.sym` file is parsed by `main` lines 86-105. Blank lines are
dropped. A line that starts with `#` is copied through. `--` as the
first word is dropped. The first other line inserts the start marker.
A name alone is both the name and the expression.

`--test` prints the C test from `gen_test` (lines 31-67) plus the
newline from `print`. The start marker expands to the `stdint.h` /
`test-driver.c` block (lines 41-58). Each constant is one
`_Static_assert` line (lines 64-66).

The header and Python modes call `glibcextract.compute_c_consts`. The
probe is a temporary C file. `START` becomes `void` / `dummy (void)` /
`{` (glibcextract.py lines 42-43). Each constant is one `asm` line
(lines 49-50). The compiler command is
`{cc} -S -o test.s -x c - < test.c` (line 62), run with the shell.
Assembly is matched by the `@@@name@@@` / `@@@value@@@` / `@@@end@@@`
expression (lines 67-69). A repeated name with a different value raises
`duplicate constant {name}` (lines 73-74).

Header output is `#define NAME VALUE` lines, sorted by name, with no
extra trailing line (gen-as-const.py line 117). Python output is the
generated-file banner (lines 110-113) and `NAME = VALUE` lines, sorted
by name (line 114).

If `--cc` is omitted, Python passes `None`. Formatting that with `%s`
yields the word `None`.

## What still uses Python

Replacing `gen-as-const.py` does not remove Python from the glibc
build. `configure.ac` lines 590-596 require `python3` or `python` on
`PATH` and add `python` to the critical-program error when it is
missing:

```
# Check for python3 if available, or else python.
AC_CHECK_PROG_VER(PYTHON_PROG, python3 python, --version,
  [Python \([0-9][0-9.]*\)],
  [3.[4-9]*|3.[1-9][0-9]*|[4-9].*|[1-9][0-9]*],
  critic_missing="$critic_missing python")
PYTHON="$PYTHON_PROG -B"
AC_SUBST(PYTHON)
```

`locale/Makefile` puts `C-translit.h` in `before-compile` and runs
`$(PYTHON) gen-translit.py`. The recipe is:

```
$(objpfx)C-translit.h: C-translit.h.in gen-translit.py
	$(make-target-directory)
	$(PYTHON) gen-translit.py < $< > $@.tmp
	mv -f $@.tmp $@
```

That script is `locale/gen-translit.py`. It is not replaced. The glibc
derivation still has `python3Minimal` from a nixpkgs import that is
not the top-level throw. Top-level `python3`, `python`, and
`python3Minimal` stay throws.

`sysdeps/unix/sysv/linux/Makefile` still calls Python for tests and for
the manual `update-syscall-lists` target. Those paths are
`sysdeps/unix/sysv/linux/update-syscall-lists.py`,
`tst-glibcsyscalls.py`, `tst-mman-consts.py`, `tst-pidfd-consts.py`,
`tst-mount-consts.py`, `tst-mount-compile.py`, `tst-sched-consts.py`,
`tst-signal-numbers.py`, `tst-socket-consts.py`, and
`tst-openat2-consts.py`. The tests import `scripts/glibcextract.py` and
`sysdeps/unix/sysv/linux/glibcsyscalls.py`. The nixpkgs glibc expression
sets `doCheck = false`, so those tests are not the library compile.
They are still Python callers in the makefile, so the interpreter stays.
