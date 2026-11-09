autoPatchelf() {
    local norecurse=""
    while [ $# -gt 0 ]; do
        case "$1" in
            --no-recurse)
                norecurse=1
                shift
                ;;
            --)
                shift
                break
                ;;
            *)
                break
                ;;
        esac
    done
    if [ $# -eq 0 ]; then
        return 0
    fi
    if [ -n "$norecurse" ]; then
        auto-patchelf --no-recurse "$@"
    else
        auto-patchelf "$@"
    fi
}

postFixupHooks+=('
    if [ -z "${dontAutoPatchelf-}" ]; then
        autoPatchelf -- $(
            for output in $outputs; do
                [ -e "${!output}" ] || continue
                printf "%s\n" "${!output}"
            done
        )
    fi
')
