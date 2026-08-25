# Private-data detector fixtures (synthetic only)

Copies of `script/testdata/private-data/` so crane tests stay hermetic.
Every sample is obviously fake. Never replace these with real keys, tokens,
host IPs, or screenshot text.

`--tree` and `--staged` exclude this directory and the script testdata path.
Tests use `--paths` (and the library `Mode::Paths` API).
