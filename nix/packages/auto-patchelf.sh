#!/bin/sh
# Set ELF interpreters and RPATH with patchelf. Search -L paths from
# NIX_LDFLAGS, cc-wrapper libc nix-support, each target lib directory,
# and runtimeDependencies.
set -eu

norecurse=0
ignore_missing=0
if [ -n "${autoPatchelfIgnoreMissingDeps-}" ]; then
    ignore_missing=1
fi

targets=""
while [ $# -gt 0 ]; do
    case "$1" in
        --no-recurse)
            norecurse=1
            shift
            ;;
        --ignore-missing)
            ignore_missing=1
            shift
            ;;
        --)
            shift
            break
            ;;
        *)
            targets="$targets $1"
            shift
            ;;
    esac
done
if [ $# -gt 0 ]; then
    targets="$targets $*"
fi

if [ -z "$targets" ]; then
    echo "auto-patchelf: no paths" >&2
    exit 1
fi

interp=""
if [ -n "${NIX_CC-}" ] && [ -e "$NIX_CC/nix-support/dynamic-linker" ]; then
    interp=$(cat "$NIX_CC/nix-support/dynamic-linker")
fi

search=""

# -Lpath, or -L as its own word with the path in the next word.
add_dash_l() {
    prev_l=0
    for word in $1; do
        if [ "$prev_l" -eq 1 ]; then
            search="$search $word"
            prev_l=0
            continue
        fi
        case "$word" in
            -L)
                prev_l=1
                ;;
            -L*)
                search="$search ${word#-L}"
                ;;
        esac
    done
}

# One store path. Drop surrounding whitespace only.
trim_ws() {
    tw=$1
    while :; do
        case "$tw" in
            [[:space:]]*) tw=${tw#?} ;;
            *) break ;;
        esac
    done
    while :; do
        case "$tw" in
            *[[:space:]]) tw=${tw%?} ;;
            *) break ;;
        esac
    done
    printf '%s\n' "$tw"
}

add_dash_l "${NIX_LDFLAGS-}"

# cc-wrapper records libc outside NIX_LDFLAGS. Those sonames live in lib/.
if [ -n "${NIX_CC-}" ]; then
    if [ -f "$NIX_CC/nix-support/orig-libc" ]; then
        libc_store=$(cat "$NIX_CC/nix-support/orig-libc")
        libc_store=$(trim_ws "$libc_store")
        if [ -n "$libc_store" ] && [ -d "$libc_store/lib" ]; then
            search="$search $libc_store/lib"
        fi
    fi
    if [ -f "$NIX_CC/nix-support/libc-ldflags" ]; then
        add_dash_l "$(cat "$NIX_CC/nix-support/libc-ldflags")"
    fi
    if [ -f "$NIX_CC/nix-support/orig-libc-ldflags" ]; then
        add_dash_l "$(cat "$NIX_CC/nix-support/orig-libc-ldflags")"
    fi
fi

for dep in ${runtimeDependencies-}; do
    if [ -d "$dep/lib" ]; then
        search="$search $dep/lib"
    fi
done
for target in $targets; do
    if [ -d "$target/lib" ]; then
        search="$search $target/lib"
    fi
done

skipped_needed() {
    case "$1" in
        linux-vdso.so.* | ld-linux*.so.* | ld-linux*.so)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

find_lib() {
    lib="$1"
    for dir in $search; do
        if [ -e "$dir/$lib" ] || [ -L "$dir/$lib" ]; then
            printf '%s\n' "$dir"
            return 0
        fi
    done
    return 1
}

missing_any=0

patch_one() {
    file="$1"
    if ! patchelf --print-needed "$file" >/dev/null 2>&1; then
        return 0
    fi
    rpath=""
    missing=""
    needed=$(patchelf --print-needed "$file" || true)
    for lib in $needed; do
        if skipped_needed "$lib"; then
            continue
        fi
        if dir=$(find_lib "$lib"); then
            case " $rpath " in
                *" $dir "*) ;;
                *) rpath="$rpath $dir" ;;
            esac
        else
            missing="$missing $lib"
        fi
    done
    if [ -n "$missing" ]; then
        echo "auto-patchelf: $file missing:$missing" >&2
        if [ "$ignore_missing" -ne 1 ]; then
            missing_any=1
        fi
    fi
    rpath_colon=""
    for dir in $rpath; do
        if [ -z "$rpath_colon" ]; then
            rpath_colon="$dir"
        else
            rpath_colon="$rpath_colon:$dir"
        fi
    done
    if [ -n "$rpath_colon" ]; then
        patchelf --set-rpath "$rpath_colon" "$file"
    fi
    if [ -n "$interp" ] && patchelf --print-interpreter "$file" >/dev/null 2>&1; then
        patchelf --set-interpreter "$interp" "$file"
    fi
}

for target in $targets; do
    if [ -d "$target" ]; then
        if [ "$norecurse" -eq 1 ]; then
            files=$(find "$target" -maxdepth 1 -type f)
        else
            files=$(find "$target" -type f)
        fi
        # No pipe. A pipe would run patch_one in a subshell and drop
        # missing_any, so a missing library would look like success.
        for file in $files; do
            patch_one "$file"
        done
    elif [ -f "$target" ]; then
        patch_one "$target"
    fi
done

if [ "$missing_any" -ne 0 ]; then
    exit 1
fi
