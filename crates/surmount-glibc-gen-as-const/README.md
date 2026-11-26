# surmount-glibc-gen-as-const

This program replaces glibc 2.44 `scripts/gen-as-const.py`. The glibc
makefile calls it where that script used to run. It does not run
python3.

The behavior follows the script in the glibc 2.44 tarball
(`glibc-2.44/scripts/gen-as-const.py`) and `compute_c_consts` in
`glibc-2.44/scripts/glibcextract.py`. Public notes:
[docs/glibc-gen-as-const.md](../../docs/glibc-gen-as-const.md).

## .sym format

The input is a `.sym` file. `main` in `gen-as-const.py` reads it as
follows.

1. Blank lines are dropped.
2. A line whose first non-whitespace character is `#` is a preprocessor
   directive. It is copied through. The script does not treat `//` as a
   comment.
3. `--` as the first word is a separator. That line is dropped,
   including any text after `--`.
4. The first line that is not a directive inserts a start marker before
   that line is classified. A file with no such line still gets one
   start marker at the end.
5. A line with only a name uses that name as the C expression. A line
   with a name and more text uses the rest of the line as the expression.

## Command line

```
surmount-glibc-gen-as-const [-h] [--cc CC] [--test] [--python] sym_file
```

1. The default writes `#define NAME VALUE` lines, sorted by name.
   Values come from the compiler probe, not from the expression text.
2. `--test` writes a C compilation test. It does not run the compiler.
   If `--python` is also set, `--test` wins.
3. `--python` writes Python assignments, sorted by name, under the
   banner the script prints.
4. `--cc` is the C compiler command, including options. The makefile
   passes one string. If `--cc` is omitted, the probe command starts
   with the word `None`, which is what Python's `%s` does with `None`.
5. `-h` and `--help` exit 0. A usage error exits 2. A missing file or a
   failed compiler exits 1.

## Compiler probe

For a header or a Python file, the program writes a temporary C file
and compiles it the way `glibcextract.py` does:

```
{cc} -S -o {temp}/test.s -x c - < {temp}/test.c
```

The shell is `sh -c`. The compiler reads the probe on stdin so the
temporary path is not written into generated dependencies. Assembly
lines carry `@@@name@@@`, `@@@value@@@`, and `@@@end@@@`. The value is
the hex, `x`, `X`, digit, and minus run after any other characters
(a `$` prefix is skipped). The same name with two different values is
`duplicate constant {name}`.

## How the glibc makefile calls it

glibc 2.44 `Makerules` has three recipes. Nix `substituteInPlace`
replaces `$(PYTHON)` plus the script with this binary. The recipe
lines are:

1. Line 251. `$(PYTHON) $(py-const-script) --python` with
   `--cc="$(CC) $(CFLAGS) $(CPPFLAGS)"`. `py-const-script` is
   `$(..)scripts/gen-as-const.py` (line 235).
2. Line 271. `$(PYTHON) $< --cc="$(CC) $(CFLAGS) $(CPPFLAGS) -DGEN_AS_CONST_HEADERS ..."`
   on the `.sym` file. This is the `before-compile` header rule.
3. Line 288. `$(PYTHON) $< --test` on the `.sym` file, after an awk
   prefix. That recipe is `tests-internal`.

The `.py` file stays in the glibc source tree. The recipes do not
execute it.
