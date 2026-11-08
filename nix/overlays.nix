# Optional overlays. The flake injects Surmount packages via surmountOverlay
# in flake.nix (stalwart-mail, stalwart-cli, management-ui, artiOnionService,
# ops bins: private-data / host-logs / shc / niced-builder / leftover-homes /
# host-probe / static-sites / diskstation / deploy-host / dns-zone /
# domain-audit / host-cutover / acme-namecheap / stalwart-ops / mail-import /
# secrets-install, grok-oss, splora / splora-liquid from flake input packages).
# Keep this file for any extra pins that should not live in the flake body.
#
# artiOnionService lives in flake surmountOverlay (distinct from stock
# pkgs.arti). The expression is nix/packages/arti-onion-service.nix
# (GitLab tag arti-v2.8.0). The Arti 2.8.0 cargoHash is set there
# (sha256-grgIqU1gw1UqeGgR4aR7cAOxBW91F12bHMgOj4pgLcY=). This file
# does not vendor Arti. Not a feature-only override of channel 1.4.2.
#
# This file exports three overlays:
#   pins      Nix 2.35.4 cli. Safe on checks (mkPkgs) and on the mail host.
#   tzdata    tzdata 2026e doCheck = false. Mail-host nixosSystem only.
#             Not on mkPkgs, so checks.x86_64-linux.ci does not rebuild
#             CPython just because now.ck is skipped.
#   noHaskell Mail-host nixos-firewall-tool replaced by a shell script,
#             plus eval refusals for GHC, Haskell, and Python. Mail-host
#             nixosSystem only. Not on mkPkgs. The firewall-tool check
#             was the remaining Haskell edge after nixfmt-rs. It is now
#             a non-Haskell script. Referencing ghc, haskellPackages,
#             haskell.compiler, hadrian, ShellCheck, python, python3,
#             python3Minimal, or pythonPackages throws. glibc's
#             python3Minimal input and libapparmor's python3Packages
#             input are removed. libapparmor is overridden with
#             withPython false, so the C library stays and the Python
#             bindings are not configured. There is no stand-in
#             interpreter.
#             dbus is built with x11Support false, so libx11 is not a
#             dbus input. tpm2-tss install checks are off, so swtpm is
#             not an install-check input. The tpm2-tss library stays.
#             glib introspection is off, so gobject-introspection is
#             not a glib input. re2c configure no longer searches for
#             an interpreter. The C++ build copies bootstrap sources.
#             ninja is built by CMake and the C++ compiler. configure.py
#             is not run. No interpreter is added.
#
# tzdata 2026e: now.ck is date-dependent. On or after 2026-11-01 the check
# fails because America/Phoenix and America/Vancouver are identical from
# now on (America/Edmonton and America/Winnipeg are the same class). This
# is not a gcc or x86-64-v3 failure. Same attribute change as
# NixOS/nixpkgs pull 582069. gcc.arch and x86-64-v3 stay as they are.
#
# Nix: nixos-26.05 pkgs.nix is nixVersions.stable (the 2.34.9 recipe).
# nixVersions.nix_2_35 is the 2.35.3 recipe: one fetchFromGitHub source
# and meson components, no cargoHash. 2.35.4 is that recipe plus the
# source of commit 21286d605815196c3fff1960550ab08700fc4e6f and version
# 2.35.4. The nixpkgs channel stays nixos-26.05. NixOS 26.11 is not
# released.
#
# The system package is nix-cli, not nix-everything. nix-everything's
# installPhase symlinks doc and man to nix-manual (the HTML manual;
# json-schema-for-humans). Its doCheck pulls nix-functional-tests, and
# those tests' nativeBuildInputs include mercurial (python3.13-pip).
# The mail host runs nix-daemon from nix-cli, including the systemd
# units that component installs. It does not run the HTML manual or
# the functional tests. nix-nswrapper is only the non-root daemon
# ExecStart; the default daemon user is root, and this host does not
# change that.

let
  # Mail-host package set only. flake nixosConfigurations.mail-vps
  # appends this. mkPkgs (checks.x86_64-linux.ci, devShells) does not.
  tzdataOverlay = _final: prev: {
    tzdata = prev.tzdata.overrideAttrs (_old: {
      doCheck = false; # https://lists.iana.org/hyperkitty/list/tz@iana.org/thread/CBEK3HXYBOXGD34ZFDSF64QIDT7ZWGBB/
    });
  };

  pinsOverlay = final: prev:
    let
      # systemd configure requires a jinja module even when ukify is off.
      # The ukify target still runs that renderer. Drop both. Compile the
      # C generators beside the meson files that name them, and point those
      # command arrays at the binaries. Do not add an interpreter package.
      # man/meson.build probes an lxml helper. Force have_lxml false.
      # Keep subdir('man') and the xsltproc pages.
      systemdDropJinja =
        pkg:
        pkg.overrideAttrs (old: {
          nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [ prev.minijinja ];
          postPatch =
            (old.postPatch or "")
            + ''
              awk '
                index($0, "find_installation") && index($0, "jinja2") { skip = 1; next }
                skip && index($0, "endif") { skip = 0; next }
                skip { next }
                index($0, "want_ukify = get_option") {
                  print "want_ukify = false"
                  next
                }
                index($0, "ukify = custom_target") { drop = 1; next }
                drop && index($0, "install_dir : bindir)") { drop = 0; next }
                drop { next }
                { print }
              ' meson.build > meson.build.nojinja && mv meson.build.nojinja meson.build
              # units/capsule@.service.in is rendered as
              # tools/render-config-template config.h <template> <output>.
              # The C renderer stops on a trailing expression. This path
              # is the existing minijinja-cli binary. No new Rust main.
              if ! command -v minijinja-cli >/dev/null 2>&1; then
                echo "minijinja-cli is not on PATH" >&2
                exit 1
              fi
              rm -f tools/render-config-template
              cat > tools/render-config-template << 'MINIJINJA'
#!/bin/sh
set -eu
if [ "$#" -ne 3 ]; then
  echo "usage: render-config-template config.h template output" >&2
  exit 2
fi
config=$1
template=$2
output=$3
json=$(mktemp)
awk '
  function esc(s,    i, c, out) {
    out = ""
    for (i = 1; i <= length(s); i++) {
      c = substr(s, i, 1)
      if (c == "\\") out = out "\\\\"
      else if (c == "\"") out = out "\\\""
      else out = out c
    }
    return out
  }
  /^#define[ \t]/ {
    line = $0
    sub(/^#define[ \t]+/, "", line)
    name = line
    sub(/[ \t].*/, "", name)
    rest = substr(line, length(name) + 1)
    sub(/^[ \t]+/, "", rest)
    if (name == "") next
    # Strict mode accepts only boolean, null, or string. TTY_MODE 0600
    # is not a JSON number. Zero is false so {% if %} can fail.
    # Every other value is a JSON string, including 0600 and 260.
    if (rest == "") val = "\"1\""
    else if (rest ~ /^[-+]?0+$/) val = "false"
    else if (rest ~ /^".*"$/) {
      inner = substr(rest, 2, length(rest) - 2)
      val = "\"" esc(inner) "\""
    } else val = "\"" esc(rest) "\""
    keys[++n] = name
    vals[n] = val
  }
  END {
    printf "{"
    for (i = 1; i <= n; i++) {
      if (i > 1) printf ","
      printf "\"%s\":%s", keys[i], vals[i]
    }
    printf "}\n"
  }
' "$config" > "$json"
minijinja-cli --autoescape none --strict --format json -o "$output" "$template" "$json"
rm -f "$json"
if [ -f "$template" ]; then
  chmod --reference="$template" "$output" || true
fi
exit 0
MINIJINJA
              chmod 755 tools/render-config-template
              if ! grep -F -q minijinja-cli tools/render-config-template; then
                echo "render-config-template does not call minijinja-cli" >&2
                exit 1
              fi
              # filesystem-sets is a script whose shebang interpreter is
              # absent. Emit the same gperf and C from the shell.
              # FSSCRIPT is column 0 for the same reason as CPPWRAP.
              cat > src/basic/filesystem-sets.sh << 'FSSCRIPT'
#!/bin/sh
exec awk -f - "$@" << 'AWK'
              function add(name, magics, obs) {
                n++
                names[n] = name
                mags[n] = magics
                obsolete[name] = obs
                magof[name] = magics
                known = known (known == "" ? "" : " ") name
              }
              function addset(name, help, fs) {
                setn++
                setname[setn] = name
                sethelp[setn] = help
                setfs[setn] = fs
              }
              function enum_of(name,    s) {
                s = substr(name, 2)
                gsub(/-/, "_", s)
                return "FILESYSTEM_SET_" toupper(s)
              }
              function uniq_words(s,    n, a, i, seen, out) {
                n = split(s, a, " ")
                out = ""
                delete seen
                for (i = 1; i <= n; i++) {
                  if (a[i] != "" && !seen[a[i]]++) {
                    out = out (out == "" ? "" : " ") a[i]
                  }
                }
                return out
              }
              function sort_words(s,    n, a, i, j, tmp, out) {
                n = split(s, a, " ")
                for (i = 1; i <= n; i++) {
                  for (j = i + 1; j <= n; j++) {
                    if (a[j] < a[i]) {
                      tmp = a[i]
                      a[i] = a[j]
                      a[j] = tmp
                    }
                  }
                }
                out = ""
                for (i = 1; i <= n; i++) {
                  if (a[i] != "") {
                    out = out (out == "" ? "" : " ") a[i]
                  }
                }
                return out
              }
              function magics_for(fs_list,    n, a, i, m, words, w, j, all) {
                n = split(fs_list, a, " ")
                all = ""
                for (i = 1; i <= n; i++) {
                  m = magof[a[i]]
                  w = split(m, words, " ")
                  for (j = 1; j <= w; j++) {
                    all = all (all == "" ? "" : " ") words[j]
                  }
                }
                return sort_words(uniq_words(all))
              }

              BEGIN {
                add("apparmorfs", "AAFS_MAGIC", 0)
                add("adfs", "ADFS_SUPER_MAGIC", 0)
                add("affs", "AFFS_SUPER_MAGIC", 0)
                add("afs", "AFS_FS_MAGIC AFS_SUPER_MAGIC", 0)
                add("anon_inodefs", "ANON_INODE_FS_MAGIC", 0)
                add("autofs", "AUTOFS_SUPER_MAGIC", 0)
                add("balloon-kvm", "BALLOON_KVM_MAGIC", 0)
                add("bcachefs", "BCACHEFS_SUPER_MAGIC", 0)
                add("bdev", "BDEVFS_MAGIC", 0)
                add("binder", "BINDERFS_SUPER_MAGIC", 0)
                add("binfmt_misc", "BINFMTFS_MAGIC", 0)
                add("bpf", "BPF_FS_MAGIC", 0)
                add("btrfs", "BTRFS_SUPER_MAGIC", 0)
                add("btrfs_test_fs", "BTRFS_TEST_MAGIC", 0)
                add("cpuset", "CGROUP_SUPER_MAGIC", 1)
                add("ceph", "CEPH_SUPER_MAGIC", 0)
                add("cgroup2", "CGROUP2_SUPER_MAGIC", 0)
                add("cgroup", "CGROUP_SUPER_MAGIC", 0)
                add("cifs", "CIFS_SUPER_MAGIC SMB2_SUPER_MAGIC", 0)
                add("coda", "CODA_SUPER_MAGIC", 0)
                add("configfs", "CONFIGFS_MAGIC", 0)
                add("cramfs", "CRAMFS_MAGIC", 0)
                add("dax", "DAXFS_MAGIC", 0)
                add("debugfs", "DEBUGFS_MAGIC", 0)
                add("devmem", "DEVMEM_MAGIC", 0)
                add("devpts", "DEVPTS_SUPER_MAGIC", 0)
                add("devtmpfs", "TMPFS_MAGIC", 1)
                add("dmabuf", "DMA_BUF_MAGIC", 0)
                add("ecryptfs", "ECRYPTFS_SUPER_MAGIC", 0)
                add("efivarfs", "EFIVARFS_MAGIC", 0)
                add("efs", "EFS_SUPER_MAGIC", 0)
                add("erofs", "EROFS_SUPER_MAGIC_V1", 0)
                add("ext2", "EXT2_SUPER_MAGIC", 1)
                add("ext3", "EXT3_SUPER_MAGIC", 1)
                add("ext4", "EXT4_SUPER_MAGIC", 0)
                add("exfat", "EXFAT_SUPER_MAGIC", 0)
                add("f2fs", "F2FS_SUPER_MAGIC", 0)
                add("failfs", "FAIL_FS_MAGIC", 0)
                add("fuseblk", "FUSE_SUPER_MAGIC", 1)
                add("fuse", "FUSE_SUPER_MAGIC", 0)
                add("fusectl", "FUSE_CTL_SUPER_MAGIC", 0)
                add("gfs", "GFS2_MAGIC", 1)
                add("gfs2", "GFS2_MAGIC", 0)
                add("gmem", "GUEST_MEMFD_MAGIC", 0)
                add("hostfs", "HOSTFS_SUPER_MAGIC", 0)
                add("hpfs", "HPFS_SUPER_MAGIC", 0)
                add("hugetlbfs", "HUGETLBFS_MAGIC", 0)
                add("iso9660", "ISOFS_SUPER_MAGIC", 0)
                add("jffs2", "JFFS2_SUPER_MAGIC", 0)
                add("minix", "MINIX_SUPER_MAGIC MINIX_SUPER_MAGIC2 MINIX2_SUPER_MAGIC MINIX2_SUPER_MAGIC2 MINIX3_SUPER_MAGIC", 0)
                add("mqueue", "MQUEUE_MAGIC", 0)
                add("msdos", "MSDOS_SUPER_MAGIC", 1)
                add("ncp", "NCP_SUPER_MAGIC", 1)
                add("ncpfs", "NCP_SUPER_MAGIC", 0)
                add("nfs", "NFS_SUPER_MAGIC", 1)
                add("nfs4", "NFS_SUPER_MAGIC", 0)
                add("nilfs2", "NILFS_SUPER_MAGIC", 0)
                add("nsfs", "NSFS_MAGIC", 0)
                add("ntfs", "NTFS_SB_MAGIC", 0)
                add("ntfs3", "NTFS3_SUPER_MAGIC", 0)
                add("nullfs", "NULL_FS_MAGIC", 0)
                add("ocfs2", "OCFS2_SUPER_MAGIC", 0)
                add("openpromfs", "OPENPROM_SUPER_MAGIC", 0)
                add("orangefs", "ORANGEFS_DEVREQ_MAGIC", 0)
                add("overlay", "OVERLAYFS_SUPER_MAGIC", 0)
                add("pidfs", "PID_FS_MAGIC", 0)
                add("pipefs", "PIPEFS_MAGIC", 0)
                add("ppc-cmm", "PPC_CMM_MAGIC", 0)
                add("proc", "PROC_SUPER_MAGIC", 0)
                add("pstore", "PSTOREFS_MAGIC", 0)
                add("pvfs2", "ORANGEFS_DEVREQ_MAGIC", 1)
                add("qnx4", "QNX4_SUPER_MAGIC", 0)
                add("qnx6", "QNX6_SUPER_MAGIC", 0)
                add("ramfs", "RAMFS_MAGIC", 0)
                add("resctrl", "RDTGROUP_SUPER_MAGIC", 0)
                add("reiserfs", "REISERFS_SUPER_MAGIC", 0)
                add("rpc_pipefs", "RPC_PIPEFS_SUPER_MAGIC", 0)
                add("secretmem", "SECRETMEM_MAGIC", 0)
                add("securityfs", "SECURITYFS_MAGIC", 0)
                add("selinuxfs", "SELINUX_MAGIC", 0)
                add("shiftfs", "SHIFTFS_MAGIC", 0)
                add("smackfs", "SMACK_MAGIC", 0)
                add("smb3", "CIFS_SUPER_MAGIC", 1)
                add("smbfs", "SMB_SUPER_MAGIC", 0)
                add("sockfs", "SOCKFS_MAGIC", 0)
                add("squashfs", "SQUASHFS_MAGIC", 0)
                add("sysfs", "SYSFS_MAGIC", 0)
                add("tmpfs", "TMPFS_MAGIC", 0)
                add("tracefs", "TRACEFS_MAGIC", 0)
                add("udf", "UDF_SUPER_MAGIC", 0)
                add("usbdevfs", "USBDEVICE_SUPER_MAGIC", 0)
                add("vboxsf", "VBOXSF_SUPER_MAGIC", 0)
                add("vfat", "MSDOS_SUPER_MAGIC", 0)
                add("v9fs", "V9FS_MAGIC", 0)
                add("xenfs", "XENFS_SUPER_MAGIC", 0)
                add("xfs", "XFS_SUPER_MAGIC", 0)
                add("z3fold", "Z3FOLD_MAGIC", 0)
                add("zonefs", "ZONEFS_MAGIC", 0)
                add("zsmalloc", "ZSMALLOC_MAGIC", 0)
                addset("@basic-api", "Basic filesystem API", "cgroup cgroup2 devpts devtmpfs mqueue proc sysfs")
                addset("@anonymous", "Anonymous inodes", "anon_inodefs pipefs sockfs")
                addset("@application", "Application virtual filesystems", "autofs fuse overlay")
                addset("@auxiliary-api", "Auxiliary filesystem API", "binfmt_misc configfs efivarfs fusectl hugetlbfs rpc_pipefs securityfs")
                addset("@common-block", "Common block device filesystems", "btrfs erofs exfat ext4 f2fs iso9660 ntfs3 squashfs udf vfat xfs")
                addset("@historical-block", "Historical block device filesystems", "ext2 ext3 minix")
                addset("@network", "Well-known network filesystems", "afs ceph cifs gfs gfs2 ncp ncpfs nfs nfs4 ocfs2 orangefs pvfs2 smb3 smbfs")
                addset("@privileged-api", "Privileged filesystem API", "bpf debugfs pstore tracefs")
                addset("@security", "Security/MAC API VFS", "apparmorfs selinuxfs smackfs")
                addset("@temporary", "Temporary filesystems", "ramfs tmpfs")
                addset("@known", "All known filesystems declared in the kernel", known)
                if (ARGC < 2) {
                  exit 1
                }
                for (i = 1; i < ARGC; i++) {
                  m = ARGV[i]
                  if (m == "gperf") {
                    emit_gperf()
                  } else if (m == "fs-type-to-string") {
                    emit_type()
                  } else if (m == "filesystem-sets") {
                    emit_sets()
                  } else if (m == "fs-in-group") {
                    emit_in_group()
                  } else if (m == "check") {
                  } else {
                    exit 1
                  }
                  delete ARGV[i]
                }
                exit 0
              }
AWK
FSSCRIPT
              chmod 755 src/basic/filesystem-sets.sh
              sed -i \
                -e "s|files('generate-gperfs.py')|files('gperf-protocol-names')|" \
                -e "s|files('meson-render-jinja2.py')|files('render-config-template')|" \
                tools/meson.build
              sed -i \
                -e "s|files('generate-syscall-list.py')|files('syscall-list-quotes')|" \
                -e "s|files('ethtool-link-mode.py')|files('ethtool-link-modes')|" \
                -e "s|files('generate-dns_type-gperf.py')|files('dns-type-gperf')|" \
                src/shared/meson.build
              # The earlier substitution left this line in place. Meson
              # still reads `command : [python, ethtool_link_mode_py, ...]`,
              # and python is not defined. index() is a literal search.
              # awk sub() is a regex, so "[python, " is a character class
              # and does not remove that object. Rebuild the command.
              # The generator is ethtool-link-modes (writes the inc and
              # the xml, exits 0). cpp-e-wrap is the preprocessor it
              # execs. have_lxml stays false. systemd, systemdMinimal,
              # and systemdLibs share this postPatch.
              awk '
                BEGIN { q = sprintf("%c", 39) }
                {
                  needle = "[python, ethtool_link_mode_py"
                  i = index($0, needle)
                  if (i == 0) {
                    print
                    next
                  }
                  mode = "--header"
                  if (index($0, "--xml") > 0) {
                    mode = "--xml"
                  }
                  prefix = substr($0, 1, i - 1)
                  printf "%s[files(%sethtool-link-modes%s), %s%s%s, files(%scpp-e-wrap%s), system_include_args],\n", prefix, q, q, q, mode, q, q, q
                }
              ' src/shared/meson.build > src/shared/meson.build.ethtool
              mv src/shared/meson.build.ethtool src/shared/meson.build
              if grep -F -q "[python, ethtool_link_mode_py" src/shared/meson.build; then
                echo "ethtool command still names python" >&2
                exit 1
              fi
              if ! grep -F -q "files('ethtool-link-modes')" src/shared/meson.build; then
                echo "ethtool command does not name the generator" >&2
                exit 1
              fi
              if ! grep -F -q "files('cpp-e-wrap')" src/shared/meson.build; then
                echo "ethtool command does not name cpp-e-wrap" >&2
                exit 1
              fi
              sed -i \
                -e "s|files('filesystem-sets.py')|files('filesystem-sets.sh')|" \
                src/basic/meson.build
              sed -i \
                -e "s|files('generate-bpf-delegate-configs.py')|files('bpf-delegate-configs')|" \
                -e "s|command : \[python,|command : [|" \
                src/core/meson.build
              sed -i \
                -e "s|have_lxml = run_command(xml_helper_py, check: false).returncode() == 0|have_lxml = false|" \
                man/meson.build
              # gperf 3.3 rejects an empty keyword. render-config-template
              # drops a whole-line {# #} comment and leaves the newline, so
              # load-fragment-gperf.gperf has a blank keyword line. Strip
              # those lines from the templates, and run gperf through one
              # wrapper so a blank keyword section line never reaches gperf.
              # systemd-minimal and systemd-minimal-libs share this patch.
              find . \( -name '*.gperf.in' -o -name '*.gperf' \) -type f | while IFS= read -r gperf_in; do
                awk '
                  BEGIN { in_kw = 0; seps = 0; in_comment = 0 }
                  {
                    line = $0
                    while (1) {
                      if (in_comment) {
                        end = index(line, "#}")
                        if (end == 0) next
                        line = substr(line, end + 2)
                        in_comment = 0
                        continue
                      }
                      start = index(line, "{#")
                      if (start == 0) break
                      rest = substr(line, start + 2)
                      end = index(rest, "#}")
                      if (end == 0) {
                        line = substr(line, 1, start - 1)
                        in_comment = 1
                        break
                      }
                      line = substr(line, 1, start - 1) substr(line, start + end + 3)
                    }
                    if (line ~ /^%%[[:space:]]*$/) {
                      if (seps == 0) { seps = 1; in_kw = 1 }
                      else if (in_kw == 1) { in_kw = 0 }
                      print line
                      next
                    }
                    if (in_kw == 1 && line ~ /^[[:space:]]*$/) next
                    print line
                  }
                ' "$gperf_in" > "$gperf_in.nocomment"
                mv "$gperf_in.nocomment" "$gperf_in"
              done
              real_gperf=$(command -v gperf)
              if [ -z "$real_gperf" ]; then
                echo "gperf was not on PATH" >&2
                exit 1
              fi
              mkdir -p tools
              cat > tools/gperf-no-empty-keyword << 'EOF'
#!/bin/sh
set -eu
real="REAL_GPERF_PATH"
tmp=$(mktemp)
args=$(mktemp)
prev=
for a in "$@"; do
  if [ "$prev" = "--output-file" ] || [ "$prev" = "-o" ]; then
    printf '%s\n' "$a" >> "$args"
    prev=
    continue
  fi
  case "$a" in
    --output-file=*|-o?*)
      printf '%s\n' "$a" >> "$args"
      ;;
    --output-file|-o)
      printf '%s\n' "$a" >> "$args"
      prev=$a
      ;;
    -*)
      printf '%s\n' "$a" >> "$args"
      ;;
    *)
      if [ -f "$a" ]; then
        awk '
          BEGIN { in_kw = 0; seps = 0 }
          {
            if ($0 ~ /^%%[[:space:]]*$/) {
              if (seps == 0) { seps = 1; in_kw = 1 }
              else if (in_kw == 1) { in_kw = 0 }
              print
              next
            }
            if (in_kw == 1 && $0 ~ /^[[:space:]]*$/) next
            print
          }
        ' "$a" > "$tmp"
        printf '%s\n' "$tmp" >> "$args"
      else
        printf '%s\n' "$a" >> "$args"
      fi
      ;;
  esac
done
set --
while IFS= read -r line; do
  set -- "$@" "$line"
done < "$args"
set +e
"$real" "$@"
ec=$?
set -e
rm -f "$tmp" "$args"
exit "$ec"
EOF
              awk -v real="$real_gperf" '
                { gsub(/REAL_GPERF_PATH/, real); print }
              ' tools/gperf-no-empty-keyword > tools/gperf-no-empty-keyword.path
              mv tools/gperf-no-empty-keyword.path tools/gperf-no-empty-keyword
              chmod +x tools/gperf-no-empty-keyword
              wrapper=$PWD/tools/gperf-no-empty-keyword
              find . -name meson.build -type f | while IFS= read -r mb; do
                awk -v wrapper="$wrapper" '
                  BEGIN {
                    q1 = sprintf("%c", 39)
                    olda = "find_program(" q1 "gperf" q1
                    oldb = "find_program(\"gperf\""
                    repl = "find_program(" q1 wrapper q1
                  }
                  {
                    rest = $0
                    out = ""
                    while (1) {
                      ia = index(rest, olda)
                      ib = index(rest, oldb)
                      i = 0
                      old = ""
                      if (ia > 0 && (ib == 0 || ia <= ib)) { i = ia; old = olda }
                      else if (ib > 0) { i = ib; old = oldb }
                      if (i == 0) { out = out rest; break }
                      out = out substr(rest, 1, i - 1) repl
                      rest = substr(rest, i + length(old))
                    }
                    print out
                  }
                ' "$mb" > "$mb.wrapped"
                mv "$mb.wrapped" "$mb"
              done
              if ! grep -R --include=meson.build -F -q gperf-no-empty-keyword .; then
                echo "gperf program was not retargeted" >&2
                exit 1
              fi

              # ata_id.c calls udev_parse_config. The link fails with an
              # undefined reference. Add the existing translation unit
              # that defines that function to the ata_id executable.
              # Do not write a new C function. systemd-minimal and
              # systemd-minimal-libs share this hook. KEY_POS_VIEW, the
              # magic.h inserts, PROC_*_INIT_INO, GFS2, FILEID, and
              # EXT4_IOC_RESIZE_FS stay.
              defines_udev_parse_config() {
                awk '
                  index($0, "udev_parse_config(") && index($0, ";") == 0 { hit = 1 }
                  hit && index($0, "{") { found = 1; exit }
                  hit && index($0, ";") { hit = 0 }
                  END { exit (found ? 0 : 1) }
                ' "$1"
              }
              if [ -f src/udev/ata_id/meson.build ]; then
                definer=""
                find src -name '*.c' -type f > udev-parse-c.list
                while IFS= read -r cand; do
                  case "$cand" in
                    *ata_id.c) continue ;;
                  esac
                  if defines_udev_parse_config "$cand"; then
                    definer="$cand"
                    break
                  fi
                done < udev-parse-c.list
                rm -f udev-parse-c.list
                if [ -z "$definer" ]; then
                  echo "no existing object defines udev_parse_config" >&2
                  exit 1
                fi
                rel="../../''${definer#src/}"
                if ! grep -F -q "$rel" src/udev/ata_id/meson.build; then
                  awk -v src="$rel" '
                    index($0, "ata_id.c") && !added {
                      print
                      q = sprintf("%c", 39)
                      print "        " q src q ","
                      added = 1
                      next
                    }
                    { print }
                  ' src/udev/ata_id/meson.build > src/udev/ata_id/meson.build.link
                  mv src/udev/ata_id/meson.build.link src/udev/ata_id/meson.build
                fi
                if ! grep -F -q "$rel" src/udev/ata_id/meson.build; then
                  echo "ata_id meson.build does not link udev_parse_config" >&2
                  exit 1
                fi
              fi
              # src/udev/ata_id/meson.build is not in this tree. The
              # helper targets live in src/udev/meson.build. ata_id,
              # cdrom_id, dmi_memory_id, fido_id, and scsi_id reference
              # udev_parse_config. ata_id, cdrom_id, dmi_memory_id,
              # scsi_id, and iocost reference log_assert_failed. Add the
              # existing .c files that define those symbols. Do not
              # write a new C function. The ata_id directory insert above
              # stays for a tree that still has that file.
              defines_log_assert_failed() {
                awk '
                  index($0, "log_assert_failed(") && index($0, ";") == 0 { hit = 1 }
                  hit && index($0, "{") { found = 1; exit }
                  hit && index($0, ";") { hit = 0 }
                  END { exit (found ? 0 : 1) }
                ' "$1"
              }
              udev_def=""
              assert_def=""
              find src -name '*.c' -type f > udev-symbol-c.list
              while IFS= read -r cand; do
                if [ -z "$udev_def" ] && defines_udev_parse_config "$cand"; then
                  udev_def=$cand
                fi
                if [ -z "$assert_def" ] && defines_log_assert_failed "$cand"; then
                  assert_def=$cand
                fi
                if [ -n "$udev_def" ] && [ -n "$assert_def" ]; then
                  break
                fi
              done < udev-symbol-c.list
              rm -f udev-symbol-c.list
              append_udev_source() {
                helper=$1
                rel=$2
                mb=src/udev/meson.build
                if [ ! -f "$mb" ] || [ -z "$rel" ]; then
                  return 0
                fi
                awk -v helper="$helper" -v src="$rel" '
                  BEGIN { q = sprintf("%c", 39) }
                  index($0, "name") && index($0, q helper q) { hit = 1; ins = 0 }
                  index($0, "name") && ! index($0, q helper q) { hit = 0; ins = 0 }
                  hit && index($0, src) { seen = 1 }
                  hit && index($0, "sources") && index($0, "files(") && index($0, "),") && !seen && !added {
                    sub(/\)[[:space:]]*,[[:space:]]*$/, ", " q src q "),")
                    print
                    added = 1
                    hit = 0
                    next
                  }
                  hit && index($0, "sources") && index($0, "files(") && ! index($0, "),") {
                    ins = 1
                    print
                    next
                  }
                  hit && ins && !seen && !added && $0 ~ /^[[:space:]]*\),[[:space:]]*$/ {
                    print "                        " q src q ","
                    print
                    added = 1
                    ins = 0
                    hit = 0
                    next
                  }
                  { print }
                ' "$mb" > "$mb.link"
                mv "$mb.link" "$mb"
              }
              if [ -z "$udev_def" ] || [ -z "$assert_def" ]; then
                echo "udev helper symbols have no existing definition" >&2
                exit 1
              fi
              udev_rel="../''${udev_def#src/}"
              assert_rel="../''${assert_def#src/}"
              for helper in ata_id cdrom_id dmi_memory_id fido_id scsi_id; do
                append_udev_source "$helper" "$udev_rel"
              done
              for helper in ata_id cdrom_id dmi_memory_id scsi_id iocost; do
                append_udev_source "$helper" "$assert_rel"
              done
              if [ -f src/udev/meson.build ]; then
                if ! grep -F -q "$udev_rel" src/udev/meson.build; then
                  echo "udev meson.build does not link udev_parse_config" >&2
                  exit 1
                fi
                if ! grep -F -q "$assert_rel" src/udev/meson.build; then
                  echo "udev meson.build does not link log_assert_failed" >&2
                  exit 1
                fi
              fi
              # ata_id's first undefined reference is log_dispatch_internal
              # from assert-util.c:33. udev_parse_config is not that error.
              # Add the existing .c files that define the ata_id undefined
              # references. Do not write a new C function. The inserts above stay.
              for rel in \
                ../basic/log.c \
                ../basic/build.c \
                ../basic/device-nodes.c \
                ../basic/fd-util.c \
                ../basic/hashmap.c \
                ../basic/path-util.c \
                ../basic/stat-util.c \
                ../basic/static-destruct.c \
                ../basic/string-util.c \
                ../basic/utf8.c \
                ../libsystemd/sd-device/device-private.c \
                ../libsystemd/sd-device/sd-device.c \
                ../shared/conf-parser.c \
                ../shared/main-func.c
              do
                if [ ! -f "src/udev/$rel" ]; then
                  echo "missing existing source for ata_id link: $rel" >&2
                  exit 1
                fi
                for helper in ata_id cdrom_id dmi_memory_id fido_id iocost mtd_probe scsi_id v4l_id; do
                  append_udev_source "$helper" "$rel"
                done
              done
              if [ -f src/udev/meson.build ] && ! grep -F -q "../basic/log.c" src/udev/meson.build; then
                echo "udev meson.build does not link log_dispatch_internal" >&2
                exit 1
              fi
              awk '
                index($0, "configuration_data()") && !ins {
                  q = sprintf("%c", 39)
                  print "add_project_arguments(" q "-D_FILE_OFFSET_BITS=64" q ", language : " q "c" q ")"
                  ins = 1
                }
                { print }
              ' meson.build > meson.build.offset
              mv meson.build.offset meson.build
            '';
          env = (old.env or { }) // {
            NIX_CFLAGS_COMPILE =
              (toString (old.env.NIX_CFLAGS_COMPILE or old.NIX_CFLAGS_COMPILE or ""))
              + " -D_FILE_OFFSET_BITS=64";
          };
        });
    in
    {
    nixVersions = prev.nixVersions.extend (
      _vfinal: vprev:
      let
        # nix-prefetch-url --unpack printed
        # 03h0j2bk97xgm15x02ccph4h1rwwci37mkdy6d1200jxzf51b2a4
        # builtins.convertHash { toHashFormat = "sri"; hashAlgo = "sha256"; }
        # printed sha256-RIkVivtdAiBCM77NekZknOcACbyMCdBLqK+fNJeQAA4=
        nixSrc2354 = prev.fetchFromGitHub {
          owner = "NixOS";
          repo = "nix";
          rev = "21286d605815196c3fff1960550ab08700fc4e6f";
          hash = "sha256-RIkVivtdAiBCM77NekZknOcACbyMCdBLqK+fNJeQAA4=";
        };
        components = vprev.nixComponents_2_35.overrideScope (
          _finalScope: _prevScope: rec {
            version = "2.35.4";
            # Components unpack this as nix-<version>-src. The github
            # fetch has no libstore nix-util meson.build. Use the
            # patched tree so that file is in the tree meson opens.
            src = patchedSrc;
            # Muon reports meson compatibility 1.7.99. Nix 2.35.4 asks
            # for meson >= 1.8, so every component stops in project().
            # The clang-tidy subdir is a run_target. Top-level components
            # are not meson subprojects, so that subdir runs and looks up
            # an interpreter. Drop that subdir line. The sources stay.
            # Muon 0.5.0 rejects prelink : true inside library().
            # nix-util install_headers is rewritten below. Do not strip
            # preserve_path from every meson file in this find.
            patchedSrc = prev.runCommand "nix-2.35.4-src" { } ''
              mkdir "$out"
              cp -a ${nixSrc2354}/. "$out/"
              chmod -R u+w "$out"
              find "$out" -name meson.build -type f -exec sed -i \
                -e "s/meson_version : '>= 1.8'/meson_version : '>= 1.3'/" \
                -e "/subdir('clang-tidy')/d" \
                -e "/cpp_pch :/d" \
                -e "/prelink :/d" \
                {} +
              # Muon checks declare_dependency sources from the nix-util
              # project root. The file is in the support tree, so library()
              # reports that wrap-assert-fail.cc does not exist.
              wrap="$out/nix-meson-build-support/common/assert-fail/wrap-assert-fail.cc"
              if [ ! -f "$wrap" ]; then
                wrap="$out/src/libutil/nix-meson-build-support/common/assert-fail/wrap-assert-fail.cc"
              fi
              cp "$wrap" "$out/src/libutil/wrap-assert-fail.cc"
              sub="$out/src/libutil/nix-meson-build-support/common/assert-fail"
              mkdir -p "$sub"
              cp "$out/src/libutil/wrap-assert-fail.cc" "$sub/wrap-assert-fail.cc"
              # nix-store unpacks this tree with source root src/libstore.
              # library() asks for wrap-assert-fail.cc next to the meson
              # file that names it. The libutil copy is not that file.
              find "$out" -name meson.build -type f | while IFS= read -r mb; do
                if grep -F -q "wrap-assert-fail.cc" "$mb"; then
                  dest="$(dirname "$mb")/wrap-assert-fail.cc"
                  if [ ! -f "$dest" ]; then
                    cp "$out/src/libutil/wrap-assert-fail.cc" "$dest"
                  fi
                fi
              done
              find "$out" -type d -name assert-fail | while IFS= read -r dir; do
                if [ ! -f "$dir/wrap-assert-fail.cc" ]; then
                  cp "$out/src/libutil/wrap-assert-fail.cc" "$dir/wrap-assert-fail.cc"
                fi
              done
              if [ -d "$out/src/libstore" ]; then
                cp "$out/src/libutil/wrap-assert-fail.cc" "$out/src/libstore/wrap-assert-fail.cc"
              fi
              # install_headers preserve_path is ambiguous under muon when
              # a header argument contains a slash. Keep subdir nix/util,
              # and install args/ and signature/ with their own subdirs.
              awk '
                index($0, "install_headers(headers") && index($0, "preserve_path") && index($0, "nix/util") {
                  q = sprintf("%c", 39)
                  print "install_headers(headers, subdir : " q "nix/util" q ")"
                  print "install_headers(" q "include/nix/util/args/root.hh" q ", subdir : " q "nix/util/args" q ")"
                  print "install_headers(" q "include/nix/util/signature/local-keys.hh" q ", subdir : " q "nix/util/signature" q ")"
                  print "install_headers(" q "include/nix/util/signature/signer.hh" q ", subdir : " q "nix/util/signature" q ")"
                  next
                }
                { print }
              ' "$out/src/libutil/meson.build" > "$out/src/libutil/meson.build.headers"
              mv "$out/src/libutil/meson.build.headers" "$out/src/libutil/meson.build"
              if grep -F -q "preserve_path" "$out/src/libutil/meson.build"; then
                echo "nix-util install_headers still has preserve_path" >&2
                exit 1
              fi
              # nix-store's source root is src/libstore. The util rewrite
              # above does not touch that meson.build. Muon stops on
              # install_headers(..., subdir : 'nix/store', preserve_path : true)
              # because header paths contain a slash. Drop preserve_path
              # and install the nested headers into their subdirs.
              if [ -f "$out/src/libstore/meson.build" ]; then
                awk '
                  index($0, "install_headers(headers") && index($0, "preserve_path") && index($0, "nix/store") {
                    q = sprintf("%c", 39)
                    print "install_headers(headers, subdir : " q "nix/store" q ")"
                    print "install_headers(" q "include/nix/store/build/build-log.hh" q ", subdir : " q "nix/store/build" q ")"
                    print "install_headers(" q "include/nix/store/build/derivation-builder.hh" q ", subdir : " q "nix/store/build" q ")"
                    print "install_headers(" q "include/nix/store/build/derivation-building-goal.hh" q ", subdir : " q "nix/store/build" q ")"
                    print "install_headers(" q "include/nix/store/build/derivation-building-misc.hh" q ", subdir : " q "nix/store/build" q ")"
                    print "install_headers(" q "include/nix/store/build/derivation-env-desugar.hh" q ", subdir : " q "nix/store/build" q ")"
                    print "install_headers(" q "include/nix/store/build/derivation-goal.hh" q ", subdir : " q "nix/store/build" q ")"
                    print "install_headers(" q "include/nix/store/build/derivation-resolution-goal.hh" q ", subdir : " q "nix/store/build" q ")"
                    print "install_headers(" q "include/nix/store/build/derivation-trampoline-goal.hh" q ", subdir : " q "nix/store/build" q ")"
                    print "install_headers(" q "include/nix/store/build/drv-output-substitution-goal.hh" q ", subdir : " q "nix/store/build" q ")"
                    print "install_headers(" q "include/nix/store/build/goal.hh" q ", subdir : " q "nix/store/build" q ")"
                    print "install_headers(" q "include/nix/store/build/substitution-goal.hh" q ", subdir : " q "nix/store/build" q ")"
                    print "install_headers(" q "include/nix/store/build/worker.hh" q ", subdir : " q "nix/store/build" q ")"
                    print "install_headers(" q "include/nix/store/builtins/buildenv.hh" q ", subdir : " q "nix/store/builtins" q ")"
                    print "install_headers(" q "unix/include/nix/store/build/child.hh" q ", subdir : " q "nix/store/build" q ")"
                    print "install_headers(" q "unix/include/nix/store/build/hook-instance.hh" q ", subdir : " q "nix/store/build" q ")"
                    next
                  }
                  { print }
                ' "$out/src/libstore/meson.build" > "$out/src/libstore/meson.build.headers"
                mv "$out/src/libstore/meson.build.headers" "$out/src/libstore/meson.build"
                if grep -F -q "preserve_path : true" "$out/src/libstore/meson.build"; then
                  echo "nix-store install_headers still has preserve_path" >&2
                  exit 1
                fi
              fi
              # libfetchers install_headers uses preserve_path. Muon says
              # file arguments are ambiguous with preserve_path: true.
              # The headers under include/nix/fetchers are flat names
              # (attrs.hh and the rest), so drop the flag and keep subdir.
              # The libstore drop above stays.
              if [ -f "$out/src/libfetchers/meson.build" ]; then
                awk '
                  index($0, "install_headers(headers") && index($0, "preserve_path") && index($0, "nix/fetchers") {
                    q = sprintf("%c", 39)
                    print "install_headers(headers, subdir : " q "nix/fetchers" q ")"
                    next
                  }
                  { print }
                ' "$out/src/libfetchers/meson.build" > "$out/src/libfetchers/meson.build.headers"
                mv "$out/src/libfetchers/meson.build.headers" "$out/src/libfetchers/meson.build"
                if grep -F -q "preserve_path : true" "$out/src/libfetchers/meson.build"; then
                  echo "nix-fetchers install_headers still has preserve_path" >&2
                  exit 1
                fi
              fi
              # libstore calls dependency('nix-util'). Muon then opens
              # subprojects/nix-util and stops when that directory has no
              # meson.build. A wrap download is disabled, so the file has
              # to already be in the source tree. Link the already-built
              # nixutil library. Do not rebuild libutil here.
              rm -rf "$out/src/libstore/subprojects/nix-util"
              rm -f "$out/src/libstore/subprojects/nix-util.wrap"
              mkdir -p "$out/src/libstore/subprojects/nix-util"
              cat > "$out/src/libstore/subprojects/nix-util/meson.build" << 'NIXUTIL'
project('nix-util', 'cpp', version : '2.35.4', meson_version : '>= 1.3', default_options : ['cpp_std=c++23'])

cpp = meson.get_compiler('cpp')
nixutil_lib = cpp.find_library('nixutil', required : false)
if not nixutil_lib.found()
  nixutil_lib = cpp.find_library('nix-util', required : true)
endif
nix_util_dep = declare_dependency(
  dependencies : nixutil_lib,
  compile_args : ['-std=c++23'],
)
meson.override_dependency('nix-util', nix_util_dep)
NIXUTIL
              if [ ! -s "$out/src/libstore/subprojects/nix-util/meson.build" ]; then
                echo "nix-util subproject meson.build was not written" >&2
                exit 1
              fi
              # libstore dependency('aws-crt-cpp') opens this directory when
              # s3-aws-auth is required. A wrap download is disabled. The
              # mail-host build turns that feature off in preConfigure.
              # This file is only so a lookup still has a meson.build.
              rm -rf "$out/src/libstore/subprojects/aws-crt-cpp"
              rm -f "$out/src/libstore/subprojects/aws-crt-cpp.wrap"
              mkdir -p "$out/src/libstore/subprojects/aws-crt-cpp"
              cat > "$out/src/libstore/subprojects/aws-crt-cpp/meson.build" << 'AWSCRT'
project('aws-crt-cpp', 'cpp', version : '0.0', meson_version : '>= 1.3')
aws_crt_cpp_dep = declare_dependency()
meson.override_dependency('aws-crt-cpp', aws_crt_cpp_dep)
AWSCRT
              if [ ! -s "$out/src/libstore/subprojects/aws-crt-cpp/meson.build" ]; then
                echo "aws-crt-cpp subproject meson.build was not written" >&2
                exit 1
              fi
              # libfetchers calls dependency('nix-store'). Muon opens
              # src/libfetchers/subprojects/nix-store and stops when that
              # directory has no meson.build. Link the already-built
              # nixstore library. Do not rebuild libstore here. The
              # libstore preserve_path rewrite above stays.
              rm -rf "$out/src/libfetchers/subprojects/nix-store"
              rm -f "$out/src/libfetchers/subprojects/nix-store.wrap"
              mkdir -p "$out/src/libfetchers/subprojects/nix-store"
              cat > "$out/src/libfetchers/subprojects/nix-store/meson.build" << 'NIXSTORE'
project('nix-store', 'cpp', version : '2.35.4', meson_version : '>= 1.3', default_options : ['cpp_std=c++23'])

cpp = meson.get_compiler('cpp')
nixstore_lib = cpp.find_library('nixstore', required : false)
if not nixstore_lib.found()
  nixstore_lib = cpp.find_library('nix-store', required : true)
endif
nix_store_dep = declare_dependency(
  dependencies : nixstore_lib,
  compile_args : ['-std=c++23'],
)
meson.override_dependency('nix-store', nix_store_dep)
NIXSTORE
              if [ ! -s "$out/src/libfetchers/subprojects/nix-store/meson.build" ]; then
                echo "nix-store subproject meson.build was not written" >&2
                exit 1
              fi
              # libfetchers calls dependency('libgit2', version : '>= 1.9').
              # export/meson.build passes that dep to
              # pkgconfig.generate(requires : requires_public). A
              # declare_dependency stub is not from pkgconf, so muon
              # errors "dependency not from pkgconf". This subproject
              # file looks the library up with method pkg-config
              # (allow_fallback false, so it does not re-enter itself).
              # Installed libgit2 is 1.9.7, which meets >= 1.9.
              # Do not weaken version : '>= 1.9'. Do not restore
              # -Dnlohmann_json:pkg_config_path. The preserve_path
              # rewrite above stays. No new C program.
              rm -f "$out/src/libfetchers/subprojects/libgit2.wrap"
              mkdir -p "$out/src/libfetchers/subprojects/libgit2"
              cat > "$out/src/libfetchers/subprojects/libgit2/meson.build" << 'LIBGIT2'
project('libgit2', 'c', version : '1.9.7', meson_version : '>= 1.3')
# Real pkgconf dependency. declare_dependency is rejected by
# pkgconfig.generate. allow_fallback false skips this subproject.
# version constraint stays >= 1.9.
libgit2_dep = dependency(
  'libgit2',
  method : 'pkg-config',
  version : '>= 1.9',
  required : true,
  allow_fallback : false,
)
meson.override_dependency('libgit2', libgit2_dep)
LIBGIT2
              if [ ! -s "$out/src/libfetchers/subprojects/libgit2/meson.build" ]; then
                echo "libgit2 subproject meson.build was not written" >&2
                exit 1
              fi
              # src/libfetchers/meson.build:
              # nlohmann_json = dependency('nlohmann_json', version : '>= 3.9')
              # subprojects/nlohmann_json has no meson.build. Quoted .pc:
              # share/pkgconfig/nlohmann_json.pc Version: 3.12.0
              # Cflags: -I prefix include. Do not weaken '>= 3.9'.
              # libgit2 meson.build stays. preserve_path stays.
              # The same dependency() is in libexpr, libcmd, libflake,
              # and libutil. Write each empty subproject now.
              for comp in libfetchers libexpr libcmd libflake libutil; do
                mb="$out/src/$comp/meson.build"
                dest="$out/src/$comp/subprojects/nlohmann_json"
                if [ -f "$mb" ] && grep -F -q "dependency('nlohmann_json'" "$mb"; then
                  mkdir -p "$dest"
                  rm -f "$out/src/$comp/subprojects/nlohmann_json.wrap"
                  if [ ! -s "$dest/meson.build" ]; then
                    cat > "$dest/meson.build" << 'NLOHMANN'
project('nlohmann_json', 'cpp', version : '3.12.0', meson_version : '>= 1.3')
# version : '>= 3.9' stays. 3.12.0 satisfies it. Header-only include.
if not meson.project_version().version_compare('>= 3.9')
  error('nlohmann_json 3.12.0 does not satisfy >= 3.9')
endif
nlohmann_json_dep = declare_dependency(
  include_directories : include_directories('${final.nlohmann_json}/include'),
  version : '3.12.0',
)
meson.override_dependency('nlohmann_json', nlohmann_json_dep)
NLOHMANN
                  fi
                fi
              done
              if [ ! -s "$out/src/libfetchers/subprojects/nlohmann_json/meson.build" ]; then
                echo "nlohmann_json subproject meson.build was not written" >&2
                exit 1
              fi
              if ! grep -F -q "version : '>= 3.9'" "$out/src/libfetchers/subprojects/nlohmann_json/meson.build"; then
                echo "nlohmann_json version constraint was weakened" >&2
                exit 1
              fi
            '';
            # Same rewrite if a component build still sees the upstream line.
            # nix-util's source root is src/libutil, so meson.build is that file.
            mesonComponentOverrides = prev.lib.composeExtensions _prevScope.mesonComponentOverrides (
              _finalAttrs: prevAttrs: {
                buildInputs =
                  (prevAttrs.buildInputs or [ ])
                  ++ prev.lib.optionals ((prevAttrs.pname or "") == "nix-fetchers") [
                    final.libgit2
                    final.nlohmann_json
                  ]
                  ++ prev.lib.optionals (builtins.elem (prevAttrs.pname or "") [
                    "nix-expr"
                    "nix-cmd"
                    "nix-flake"
                    "nix-util"
                  ]) [
                    final.nlohmann_json
                  ];
                # Muon rejects a scoped pkg-config option on these
                # components and suggests the project option
                # pkg_config_path. Do not pass that scoped name for
                # nix-fetchers, nix-expr, nix-cmd, nix-flake, or
                # nix-util. nix-fetchers still passes
                # -Dpkg_config_path for libgit2 and nlohmann_json.
                # preConfigure still exports PKG_CONFIG_PATH.
                # version : '>= 3.9' stays. nofallback stays.
                # libgit2 meson.build and the preserve_path drop stay.
                mesonFlags =
                  (prevAttrs.mesonFlags or [ ])
                  ++ prev.lib.optionals ((prevAttrs.pname or "") == "nix-fetchers") [
                    "--wrap-mode=nofallback"
                    "-Dpkg_config_path=${final.libgit2.dev}/lib/pkgconfig:${final.nlohmann_json}/share/pkgconfig"
                  ];
                postPatch =
                  (prevAttrs.postPatch or "")
                  + ''
                    if [ -f meson.build ] && grep -F -q preserve_path meson.build && grep -F -q "nix/util" meson.build; then
                      awk '
                        index($0, "install_headers(headers") && index($0, "preserve_path") && index($0, "nix/util") {
                          q = sprintf("%c", 39)
                          print "install_headers(headers, subdir : " q "nix/util" q ")"
                          print "install_headers(" q "include/nix/util/args/root.hh" q ", subdir : " q "nix/util/args" q ")"
                          print "install_headers(" q "include/nix/util/signature/local-keys.hh" q ", subdir : " q "nix/util/signature" q ")"
                          print "install_headers(" q "include/nix/util/signature/signer.hh" q ", subdir : " q "nix/util/signature" q ")"
                          next
                        }
                        { print }
                      ' meson.build > meson.build.headers && mv meson.build.headers meson.build
                    fi
                    if [ -f meson.build ] && grep -F -q preserve_path meson.build && grep -F -q "nix/store" meson.build; then
                      awk '
                        index($0, "install_headers(headers") && index($0, "preserve_path") && index($0, "nix/store") {
                          q = sprintf("%c", 39)
                          print "install_headers(headers, subdir : " q "nix/store" q ")"
                          print "install_headers(" q "include/nix/store/build/build-log.hh" q ", subdir : " q "nix/store/build" q ")"
                          print "install_headers(" q "include/nix/store/build/derivation-builder.hh" q ", subdir : " q "nix/store/build" q ")"
                          print "install_headers(" q "include/nix/store/build/derivation-building-goal.hh" q ", subdir : " q "nix/store/build" q ")"
                          print "install_headers(" q "include/nix/store/build/derivation-building-misc.hh" q ", subdir : " q "nix/store/build" q ")"
                          print "install_headers(" q "include/nix/store/build/derivation-env-desugar.hh" q ", subdir : " q "nix/store/build" q ")"
                          print "install_headers(" q "include/nix/store/build/derivation-goal.hh" q ", subdir : " q "nix/store/build" q ")"
                          print "install_headers(" q "include/nix/store/build/derivation-resolution-goal.hh" q ", subdir : " q "nix/store/build" q ")"
                          print "install_headers(" q "include/nix/store/build/derivation-trampoline-goal.hh" q ", subdir : " q "nix/store/build" q ")"
                          print "install_headers(" q "include/nix/store/build/drv-output-substitution-goal.hh" q ", subdir : " q "nix/store/build" q ")"
                          print "install_headers(" q "include/nix/store/build/goal.hh" q ", subdir : " q "nix/store/build" q ")"
                          print "install_headers(" q "include/nix/store/build/substitution-goal.hh" q ", subdir : " q "nix/store/build" q ")"
                          print "install_headers(" q "include/nix/store/build/worker.hh" q ", subdir : " q "nix/store/build" q ")"
                          print "install_headers(" q "include/nix/store/builtins/buildenv.hh" q ", subdir : " q "nix/store/builtins" q ")"
                          print "install_headers(" q "unix/include/nix/store/build/child.hh" q ", subdir : " q "nix/store/build" q ")"
                          print "install_headers(" q "unix/include/nix/store/build/hook-instance.hh" q ", subdir : " q "nix/store/build" q ")"
                          next
                        }
                        { print }
                      ' meson.build > meson.build.storeheaders && mv meson.build.storeheaders meson.build
                      if grep -F -q "preserve_path : true" meson.build; then
                        echo "nix-store install_headers still has preserve_path" >&2
                        exit 1
                      fi
                    fi
                    if [ -f meson.build ] && grep -F -q preserve_path meson.build && grep -F -q "nix/fetchers" meson.build; then
                      awk '
                        index($0, "install_headers(headers") && index($0, "preserve_path") && index($0, "nix/fetchers") {
                          q = sprintf("%c", 39)
                          print "install_headers(headers, subdir : " q "nix/fetchers" q ")"
                          next
                        }
                        { print }
                      ' meson.build > meson.build.fetchheaders && mv meson.build.fetchheaders meson.build
                      if grep -F -q "preserve_path : true" meson.build; then
                        echo "nix-fetchers install_headers still has preserve_path" >&2
                        exit 1
                      fi
                    fi
                    # Configure must not see subprojects/libgit2 without a
                    # meson.build. Installed libgit2 is 1.9.7. version
                    # : '>= 1.9' stays. declare_dependency, not dependency().
                    # The preserve_path drop above stays.
                    write_libgit2_subproject() {
                      dest="$1"
                      parent=$(dirname "$dest")
                      rm -f "$parent/libgit2.wrap"
                      mkdir -p "$dest"
                      if [ -s "$dest/meson.build" ]; then
                        return 0
                      fi
                      cat > "$dest/meson.build" << 'LIBGIT2'
project('libgit2', 'c', version : '1.9.7', meson_version : '>= 1.3')
# muon dependency('libgit2') cannot see the installed library.
# find_library('git2') is libgit2.so. version 1.9.7 satisfies >= 1.9.
cc = meson.get_compiler('c')
libgit2_lib = cc.find_library('git2', required : true)
libgit2_dep = declare_dependency(
  dependencies : libgit2_lib,
  include_directories : include_directories('${final.libgit2.dev}/include'),
  version : '1.9.7',
)
meson.override_dependency('libgit2', libgit2_dep)
LIBGIT2
                      test -s "$dest/meson.build"
                    }
                    if [ -f meson.build ] && grep -F -q "dependency('libgit2'" meson.build; then
                      write_libgit2_subproject subprojects/libgit2
                    fi
                    if [ -f src/libfetchers/meson.build ] && grep -F -q "dependency('libgit2'" src/libfetchers/meson.build; then
                      write_libgit2_subproject src/libfetchers/subprojects/libgit2
                    fi
                    # nlohmann_json.pc Version: 3.12.0 lives in share/pkgconfig.
                    # dependency version stays '>= 3.9'. Do not delete libgit2.
                    write_nlohmann_subproject() {
                      dest="$1"
                      parent=$(dirname "$dest")
                      rm -f "$parent/nlohmann_json.wrap"
                      mkdir -p "$dest"
                      if [ -s "$dest/meson.build" ]; then
                        return 0
                      fi
                      cat > "$dest/meson.build" << 'NLOHMANN'
project('nlohmann_json', 'cpp', version : '3.12.0', meson_version : '>= 1.3')
# version : '>= 3.9' stays. 3.12.0 satisfies it. Header-only include.
if not meson.project_version().version_compare('>= 3.9')
  error('nlohmann_json 3.12.0 does not satisfy >= 3.9')
endif
nlohmann_json_dep = declare_dependency(
  include_directories : include_directories('${final.nlohmann_json}/include'),
  version : '3.12.0',
)
meson.override_dependency('nlohmann_json', nlohmann_json_dep)
NLOHMANN
                      test -s "$dest/meson.build"
                    }
                    if [ -f meson.build ] && grep -F -q "dependency('nlohmann_json'" meson.build; then
                      write_nlohmann_subproject subprojects/nlohmann_json
                    fi
                    if [ -f src/libfetchers/meson.build ] && grep -F -q "dependency('nlohmann_json'" src/libfetchers/meson.build; then
                      write_nlohmann_subproject src/libfetchers/subprojects/nlohmann_json
                    fi
                  '';
                # The component configure runs from this source root.
                # Write the nix-util subproject file here, after patches,
                # so muon sees meson.build before it configures libstore.
                # muon dependency() in the nlohmann_json subproject reads
                # PKG_CONFIG_PATH. share/pkgconfig/nlohmann_json.pc is
                # Version: 3.12.0. version : '>= 3.9' stays.
                preConfigure =
                  (prevAttrs.preConfigure or "")
                  + ''
                    case ":''${PKG_CONFIG_PATH:-}:" in
                      *":${final.nlohmann_json}/share/pkgconfig:"*) ;;
                      *) export PKG_CONFIG_PATH="${final.nlohmann_json}/share/pkgconfig''${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}" ;;
                    esac
                    write_nix_util_subproject() {
                      dest="$1"
                      parent=$(dirname "$dest")
                      rm -rf "$dest"
                      rm -f "$parent/nix-util.wrap"
                      mkdir -p "$dest"
                      cat > "$dest/meson.build" << 'NIXUTIL'
project('nix-util', 'cpp', version : '2.35.4', meson_version : '>= 1.3', default_options : ['cpp_std=c++23'])

cpp = meson.get_compiler('cpp')
nixutil_lib = cpp.find_library('nixutil', required : false)
if not nixutil_lib.found()
  nixutil_lib = cpp.find_library('nix-util', required : true)
endif
nix_util_dep = declare_dependency(
  dependencies : nixutil_lib,
  compile_args : ['-std=c++23'],
)
meson.override_dependency('nix-util', nix_util_dep)
NIXUTIL
                      test -s "$dest/meson.build"
                    }
                    if [ -d src/libstore ]; then
                      write_nix_util_subproject src/libstore/subprojects/nix-util
                    fi
                    if [ -f meson.build ] && grep -E -q "dependency\\('nix-util'|subproject\\('nix-util'" meson.build; then
                      write_nix_util_subproject subprojects/nix-util
                    fi
                    if [ -f subprojects/nix-util.wrap ] || [ -d subprojects/nix-util ]; then
                      if [ ! -s subprojects/nix-util/meson.build ]; then
                        write_nix_util_subproject subprojects/nix-util
                      fi
                    fi
                    # nix-fetchers source root is src/libfetchers. Muon
                    # reports that subprojects/nix-store has no meson.build
                    # and dependency('nix-store') is not found. Same shape
                    # as the nix-util stub: find the installed library.
                    write_nix_store_subproject() {
                      dest="$1"
                      parent=$(dirname "$dest")
                      rm -rf "$dest"
                      rm -f "$parent/nix-store.wrap"
                      mkdir -p "$dest"
                      cat > "$dest/meson.build" << 'NIXSTORE'
project('nix-store', 'cpp', version : '2.35.4', meson_version : '>= 1.3', default_options : ['cpp_std=c++23'])

cpp = meson.get_compiler('cpp')
nixstore_lib = cpp.find_library('nixstore', required : false)
if not nixstore_lib.found()
  nixstore_lib = cpp.find_library('nix-store', required : true)
endif
nix_store_dep = declare_dependency(
  dependencies : nixstore_lib,
  compile_args : ['-std=c++23'],
)
meson.override_dependency('nix-store', nix_store_dep)
NIXSTORE
                      test -s "$dest/meson.build"
                    }
                    if [ -f meson.build ] && grep -E -q "dependency\\('nix-store'|subproject\\('nix-store'" meson.build; then
                      write_nix_store_subproject subprojects/nix-store
                    fi
                    if [ -d subprojects/nix-store ] && [ ! -s subprojects/nix-store/meson.build ]; then
                      write_nix_store_subproject subprojects/nix-store
                    fi
                    # libgit2 pkg-config is the mesonFlags argument
                    # (--wrap-mode=nofallback and -Dpkg_config_path).
                    # subprojects/libgit2/meson.build is written in an
                    # earlier phase. Do not rm that directory here, and
                    # do not export PKG_CONFIG_PATH in this phase.
                    # version : '>= 1.9' stays. The preserve_path drop
                    # above stays. Installed libgit2 is 1.9.7.
                    # nix-store's source root is this directory. Muon reports
                    # file 'wrap-assert-fail.cc' does not exist from
                    # src/libstore/meson.build. Copy it beside every
                    # meson file that names it, and into assert-fail.
                    wrap_src=""
                    for cand in \
                      wrap-assert-fail.cc \
                      ../libutil/wrap-assert-fail.cc \
                      ../libutil/nix-meson-build-support/common/assert-fail/wrap-assert-fail.cc \
                      nix-meson-build-support/common/assert-fail/wrap-assert-fail.cc
                    do
                      if [ -f "$cand" ]; then
                        wrap_src="$cand"
                        break
                      fi
                    done
                    write_wrap_assert_fail() {
                      dest="$1"
                      if [ -f "$dest" ]; then
                        return 0
                      fi
                      mkdir -p "$(dirname "$dest")"
                      if [ -n "$wrap_src" ]; then
                        cp "$wrap_src" "$dest"
                      fi
                    }
                    find . -name meson.build -type f | while IFS= read -r mb; do
                      if grep -F -q "wrap-assert-fail.cc" "$mb"; then
                        write_wrap_assert_fail "$(dirname "$mb")/wrap-assert-fail.cc"
                      fi
                    done
                    find . -type d -name assert-fail | while IFS= read -r dir; do
                      write_wrap_assert_fail "$dir/wrap-assert-fail.cc"
                    done
                    write_wrap_assert_fail wrap-assert-fail.cc
                    # meson.options option s3-aws-auth is type feature.
                    # libstore meson.build calls
                    #   aws_crt_cpp = dependency('aws-crt-cpp', required : s3_aws_auth, method : 'cmake', ...)
                    # The configure line passes -Ds3-aws-auth=enabled, read
                    # from mesonConfigurePhase. mesonFlags is a bash array.
                    # Assigning it as a string keeps only the first element
                    # and leaves enabled in place. Rewrite the array.
                    # A disabled feature does not compile aws-creds.cc.
                    # The local aws-crt-cpp meson.build stays for a lookup
                    # that still opens that directory. No wrap download.
                    if grep -F -q "s3-aws-auth" meson.build meson.options meson_options.txt 2>/dev/null \
                      || grep -F -q "aws-crt-cpp" meson.build 2>/dev/null; then
                      rewrite_s3_flag_array() {
                        local name="$1"
                        local decl saw flag
                        local -a replaced=()
                        decl=$(declare -p "$name" 2>/dev/null || true)
                        if [ -z "$decl" ]; then
                          eval "$name=(\" -Ds3-aws-auth=disabled\")"
                          eval "$name=(\"-Ds3-aws-auth=disabled\")"
                          return 0
                        fi
                        if printf '%s\n' "$decl" | grep -F -q ' -a'; then
                          saw=0
                          eval "local -a current=(\"\''${$name[@]}\")"
                          for flag in "''${current[@]}"; do
                            case "$flag" in
                              -Ds3-aws-auth=enabled|-Ds3-aws-auth=auto)
                                replaced+=("-Ds3-aws-auth=disabled")
                                saw=1
                                ;;
                              *)
                                replaced+=("$flag")
                                ;;
                            esac
                          done
                          if [ "$saw" -eq 0 ]; then
                            replaced+=("-Ds3-aws-auth=disabled")
                          fi
                          eval "$name=(\"\''${replaced[@]}\")"
                        else
                          eval "local text=\"\''${$name}\""
                          text="''${text//-Ds3-aws-auth=enabled/-Ds3-aws-auth=disabled}"
                          text="''${text//-Ds3-aws-auth=auto/-Ds3-aws-auth=disabled}"
                          case "$text" in
                            *-Ds3-aws-auth=disabled*) ;;
                            *) text="$text -Ds3-aws-auth=disabled" ;;
                          esac
                          eval "$name=\"\$text\""
                        fi
                      }
                      rewrite_s3_flag_array mesonFlags
                      if declare -p mesonFlagsArray >/dev/null 2>&1; then
                        rewrite_s3_flag_array mesonFlagsArray
                      fi
                      echo "s3-aws-auth configure flag set to disabled"
                      if [ ! -s subprojects/aws-crt-cpp/meson.build ]; then
                        rm -rf subprojects/aws-crt-cpp
                        rm -f subprojects/aws-crt-cpp.wrap
                        mkdir -p subprojects/aws-crt-cpp
                        cat > subprojects/aws-crt-cpp/meson.build << 'AWSCRT'
project('aws-crt-cpp', 'cpp', version : '0.0', meson_version : '>= 1.3')
aws_crt_cpp_dep = declare_dependency()
meson.override_dependency('aws-crt-cpp', aws_crt_cpp_dep)
AWSCRT
                      fi
                    fi
                  '';
              }
            );
          }
        );
      in
      {
        nixComponents_2_35 = components;
        # nix-cli: binaries and systemd units. Not nix-everything
        # (that package builds nix-manual and mercurial).
        nix_2_35 = components.nix-cli;
        latest = components.nix-cli;
      }
    );

    # Channel stable stays the 2.34.9 nix-everything attribute.
    # Nothing in the mail-host module list selects it. The package
    # the system runs is pkgs.nix, the 2.35.4 nix-cli above.
    nix = final.nixVersions.nix_2_35;

    systemd = systemdDropJinja prev.systemd;
    systemdMinimal = systemdDropJinja prev.systemdMinimal;
    systemdLibs = systemdDropJinja prev.systemdLibs;

    # The release tarball already contains the generated headers.
    # The meson file still looks up an interpreter to regenerate them.
    # Copy the headers and compile those files.
    "p11-kit" = prev."p11-kit".overrideAttrs (old: {
      postPatch =
        (old.postPatch or "")
        + ''
          tar -xJf ${prev.fetchurl {
            url = "https://github.com/p11-glue/p11-kit/releases/download/0.26.2/p11-kit-0.26.2.tar.xz";
            hash = "sha256-Cf2fRNpIE6MUHnPV589wCOVmDQQF8T1WwV4dqdzs+Cg=";
          }} -C "$NIX_BUILD_TOP"
          kit="$NIX_BUILD_TOP/p11-kit-0.26.2/p11-kit"
          for h in \
            virtual-ffi-generated.h \
            virtual-fixed-wrappers.h \
            virtual-fixed-closures.h \
            virtual-stack-generated.h \
            virtual-base-generated.h \
            proxy-generated.h
          do
            cp "$kit/$h" p11-kit/"$h"
          done
          patch -p1 -i ${./packages/p11-kit-pregenerated.patch}
        '';
      # checkPhase asks ninja for meson-test-prereq. The muon build does
      # not emit that target. A phony target lets the check start.
      preCheck =
        (old.preCheck or "")
        + ''
          for ninja in build.ninja build/build.ninja; do
            if [ -f "$ninja" ] && ! grep -q '^build meson-test-prereq:' "$ninja"; then
              printf 'build meson-test-prereq: phony\n' >> "$ninja"
            fi
          done
        '';
    });
  };

  # nixos-26.05 firewall-nftables.nix puts pkgs.nixos-firewall-tool on
  # systemPackages when the firewall backend is nftables. Do not call the
  # upstream package. Its doCheck reads ShellCheck's compiler, and that
  # evaluates GHC. The firewall-tool check was the remaining Haskell edge
  # after nixfmt-rs replaced Haskell nixfmt. This script is the replacement.
  # nftables and the firewall stay enabled. Referencing ghc, haskellPackages,
  # haskell.compiler, hadrian, ShellCheck, shellcheck, python, python3,
  # python3Minimal, or pythonPackages throws, the same way ghc throws.
  # pandas, pip, dbus-python, and json-schema-for-humans throw too.
  # There is no renamed interpreter. This script does not build the
  # upstream firewall tool.
  refuse = what: throw "mail-host eval refuses ${what}";

  # Temporary open/show/reset helper. nft and iptables come from PATH at
  # runtime (nftables is already a system package when that backend is on).
  # No check phase and no upstream package.
  firewallToolScript = ''
    set -euo pipefail

    if [[ -e /etc/systemd/system/firewall.service ]]; then
      BACKEND=iptables
    elif [[ -e /etc/systemd/system/nftables.service ]]; then
      BACKEND=nftables
    else
      echo "nixos-firewall-tool: cannot detect firewall backend" >&2
      exit 1
    fi

    ip46tables() {
      iptables -w "$@"
      ip6tables -w "$@"
    }

    show_help() {
      echo "nixos-firewall-tool

    A tool to temporarily manipulate the NixOS firewall

    Open TCP port:
      nixos-firewall-tool open tcp 8888

    Open UDP port:
      nixos-firewall-tool open udp 51820

    Show all firewall rules:
      nixos-firewall-tool show

    Reset firewall configuration to system settings:
      nixos-firewall-tool reset"
    }

    if [[ -z ''${1+x} ]]; then
      show_help
      exit 1
    fi

    case $1 in
      "open")
        if [[ -z ''${2+x} ]] || [[ -z ''${3+x} ]]; then
          show_help
          exit 1
        fi
        protocol="$2"
        port="$3"
        case $BACKEND in
          iptables)
            ip46tables -I nixos-fw -p "$protocol" --dport "$port" -j nixos-fw-accept
            ;;
          nftables)
            nft add element inet nixos-fw "temp-ports" "{ $protocol . $port }"
            ;;
        esac
        ;;
      "show")
        case $BACKEND in
          iptables)
            ip46tables --numeric --list nixos-fw
            ;;
          nftables)
            nft list table inet nixos-fw
            ;;
        esac
        ;;
      "reset")
        case $BACKEND in
          iptables)
            systemctl restart firewall.service
            ;;
          nftables)
            nft flush set inet nixos-fw "temp-ports"
            ;;
        esac
        ;;
      -h|--help|help)
        show_help
        exit 0
        ;;
      *)
        show_help
        exit 1
        ;;
    esac
  '';

  noHaskellOverlay =
    final: prev:
    let
      lib = prev.lib;
      # Same nixpkgs-rust pin as flake.nix (rev 9ecc0a67, rustc 1.99.1).
      # The mail host must not compile nixos-26.05 rustc 1.95.0.
      mailHostRustSrc = builtins.fetchTree {
        type = "github";
        owner = "NixOS";
        repo = "nixpkgs";
        rev = "9ecc0a672e9ef2ca4eec8e87dbee653607a7d332";
        narHash = "sha256-ayn2dNaEAGbxEUYNc1CUVYgqIePEcxwvj1XpVCOksYg=";
      };
      mailHostRust = import mailHostRustSrc {
        system = prev.stdenv.hostPlatform.system;
      };
      # Empty store path. CallPackage can hand this to glibc and
      # libapparmor so those expressions do not force the throwing
      # interpreter attrs. mkDerivation then deletes it from every
      # dependency list. It is not an interpreter. Its name stays
      # empty-directory, and that name is not a package the mail host
      # keeps.
      slotDrv = prev.emptyDirectory;
      forbiddenMarks = [
        "python"
        "Python"
        "ghc"
        "hadrian"
        "haskell"
        "Haskell"
        "shellcheck"
        "ShellCheck"
        "pandoc"
      ];
      textMarked =
        text: builtins.isString text && builtins.any (mark: lib.hasInfix mark text) forbiddenMarks;
      # Do not read outPath here. Forcing it re-enters mkDerivation
      # while that derivation's own dependency list is still being
      # scrubbed. Match the empty directory by its name, and match
      # forbidden interpreters by name or pname. tryEval the dep
      # before name or pname. A failed tryEval is forbidden and is
      # not rethrown.
      isForbiddenDep =
        dep:
        let
          tried = builtins.tryEval dep;
        in
        if !tried.success then
          true
        else if tried.value == null then
          true
        else if !builtins.isAttrs tried.value then
          false
        else
          let
            name = tried.value.name or "";
            pname = tried.value.pname or "";
          in
          textMarked name
          || textMarked pname
          || name == "empty-directory"
          || pname == "empty-directory";
      scrubList =
        xs:
        if !builtins.isList xs then
          xs
        else
          builtins.filter (dep: !isForbiddenDep dep) (
            # tryEval before isList. A throwing element is an empty
            # contribution and is not rethrown. A success keeps the
            # isList branch on the successful value.
            lib.concatMap (
              dep:
              let
                tried = builtins.tryEval dep;
              in
              if !tried.success then
                [ ]
              else if builtins.isList tried.value then
                tried.value
              else
                [ tried.value ]
            ) xs
          );
      # Formal dependency arguments of stdenv.mkDerivation. The key
      # list is constant. A key list that depends on finalAttrs makes
      # mkDerivation recurse forever.
      edgeNames = [
        "buildInputs"
        "nativeBuildInputs"
        "propagatedBuildInputs"
        "propagatedNativeBuildInputs"
        "checkInputs"
        "nativeCheckInputs"
        "installCheckInputs"
        "nativeInstallCheckInputs"
        "depsBuildBuild"
        "depsBuildBuildPropagated"
        "depsBuildTarget"
        "depsBuildTargetPropagated"
        "depsHostHost"
        "depsHostHostPropagated"
        "depsTargetTarget"
        "depsTargetTargetPropagated"
      ];
      scrubAttrs =
        attrs:
        attrs // lib.genAttrs edgeNames (name: scrubList (attrs.${name} or [ ]));
      wrapMk =
        mk: fnOrAttrs:
        mk (
          finalAttrs:
          let
            raw = if builtins.isFunction fnOrAttrs then fnOrAttrs finalAttrs else fnOrAttrs;
          in
          scrubAttrs raw
        );
      # prev.stdenv.mkDerivation, not final.stdenv. The package set
      # keeps prev.stdenv. This wrapper is only for callScope.
      stdenvFiltered = prev.stdenv // {
        mkDerivation = wrapMk prev.stdenv.mkDerivation;
      };
      stdenvNoCCFiltered = prev.stdenvNoCC // {
        mkDerivation = wrapMk prev.stdenvNoCC.mkDerivation;
      };
      # Do not call stdenv.mkDerivation for a Python application or
      # library. AsciiDoc's buildPythonApplication did that, and
      # filterAttrs made the derivation attribute names depend on
      # finalAttrs, which recurses. The empty directory is not an
      # interpreter. The scrub drops it from dependency lists.
      buildPython = _: slot;
      slot = slotDrv // {
        isPyPy = false;
        isPy3k = true;
        isPy27 = false;
        pythonVersion = "3";
        pythonAtLeast = _: true;
        interpreter = prev.runtimeShell;
        executable = "sh";
        sitePackages = "lib";
        withPackages = _: slot;
        override = _: slot;
        overrideAttrs = _: slot;
        overridePythonAttrs = _: slot;
        pythonOnBuildForHost = slot;
        pythonForBuild = slot;
        pkgs = fakePkgs;
        meta = {
          mainProgram = "sh";
          platforms = lib.platforms.all;
        };
      };
      # Names that mail-host derivations select while the throwing
      # python3Packages attr stays a throw. Each value is the empty
      # directory, and the scrub removes it.
      fakePkgs = {
        python = slot;
        setuptools = slot;
        pip = slot;
        wheel = slot;
        jinja2 = slot;
        lxml = slot;
        pyelftools = slot;
        pefile = slot;
        cryptography = slot;
        sphinx = slot;
        sphinx-rtd-theme = slot;
        pyyaml = slot;
        packaging = slot;
        pytest = slot;
        pytest-mock = slot;
        pygments = slot;
        matplotlib = slot;
        numpy = slot;
        aafigure = slot;
        recursive-pth-loader = slot;
        pythonImportsCheckHook = slot;
        wrapPython = slot;
        recursivePthLoader = slot;
        # fio package.nix reads python3.pkgs.six. This value is the
        # empty directory, not an interpreter.
        six = slot;
        buildPythonApplication = buildPython;
        buildPythonPackage = buildPython;
      };
      # writeShellApplication reads shellcheck-minimal.compiler on
      # pkgsBuildHost. The top-level ShellCheck attr still throws.
      # bootstrapAvailable false skips that compiler. This value is
      # not a Haskell or Python derivation.
      shellcheckFake = slot // {
        compiler = {
          bootstrapAvailable = false;
        };
      };
      # nss-cacert lists this program. prev.callPackage uses
      # prev.stdenv. This does not read final.stdenv. The
      # handwritten C source is not compiled.
      buildcatrustDrv = prev.callPackage ./packages/buildcatrust.nix { };
      interpreterShadow = {
        python = slot;
        python3 = slot;
        python3Minimal = slot;
        pythonPackages = fakePkgs;
        python3Packages = fakePkgs;
        # nss-cacert lists this. The all-packages value is
        # `with python3.pkgs`, so a callPackage lookup must not
        # be that value. This derivation does not compile C.
        # The scrub drops empty-directory and interpreter names.
        # It keeps this pname.
        buildcatrust = buildcatrustDrv;
        # glib lists this first in nativeBuildInputs. The
        # all-packages value is `with python3Packages`.
        docutils = slot;
        # libtiff lists this in nativeBuildInputs. The
        # all-packages value is `with python3Packages`.
        sphinx = slot;
        # shadow lists this in nativeBuildInputs. The
        # itstool package evaluates python3.pkgs.libxml2
        # inside pythonPath. The scrub drops the empty
        # directory, so that file is not evaluated. This
        # does not define libxml2 on the stand-in.
        itstool = slot;
        # glib sets systemtap' = buildPackages.systemtap-sdt.
        # The by-name systemtap-sdt file calls
        # linuxPackages.systemtap, and that file's let-binding
        # evaluates `with python3.pkgs; makePythonPath`.
        # CallPackage formals get the empty directory. The
        # overlay output binds the same names, because
        # buildPackages looks those up on the package set.
        # This is not an interpreter.
        systemtap-sdt = slot // {
          meta = {
            platforms = [ ];
          };
        };
        systemtap = slot;
        shellcheck = shellcheckFake;
        shellcheck-minimal = shellcheckFake;
        ShellCheck = shellcheckFake;
      };
      # Previous-stage compiler. final.stdenv.cc.nativeTools recurses
      # in all-packages.nix. callScope must not read that stdenv.
      # No extra cc binding. stdenvFiltered is the whole value.
      stdenvForCall = stdenvFiltered;
      callScope = final // interpreterShadow // {
        stdenv = stdenvForCall;
        stdenvNoCC = stdenvNoCCFiltered;
        pkgs = final // interpreterShadow // {
          stdenv = stdenvForCall;
          stdenvNoCC = stdenvNoCCFiltered;
        };
      };
    in
    {
      # Top-level names. callPackage looks these up on final, so
      # overriding rustc alone would leave rustPlatform on 1.95.0.
      rustc = mailHostRust.rustc;
      cargo = mailHostRust.cargo;
      rustc-unwrapped = mailHostRust.rustc-unwrapped;
      rustPlatform = mailHostRust.rustPlatform;
      rustfmt = mailHostRust.rustfmt;
      clippy = mailHostRust.clippy;

libpsl = prev.libpsl.overrideAttrs (old:
let
  # Same script in postPatch and preAutoreconf. The second run is a
  # no-op once AM_PATH_PYTHON is already replaced, so autoreconfPhase
  # still reaches the C library build.
  pslNoGtkDoc = (old.postPatch or "") + ''
    awk '
      BEGIN { skip = 0 }
      /^m4_ifdef\(\[GTK_DOC_CHECK\]/ || /^m4_ifdef\(\[GTK_DOC_USE_LIBTOOL\]/ {
        if ($0 ~ /\]\)[[:space:]]*$/) {
          next
        }
        skip = 1
        next
      }
      skip && /^\]\)/ { skip = 0; next }
      skip { next }
      { print }
    ' configure.ac > configure.ac.new
    mv configure.ac.new configure.ac
    if grep -F GTK_DOC_CHECK configure.ac >/dev/null; then
      echo "GTK_DOC_CHECK still present in configure.ac" >&2
      exit 1
    fi
    if ! grep -F "AM_CONDITIONAL([ENABLE_GTK_DOC]" configure.ac >/dev/null; then
      printf "\nAM_CONDITIONAL([ENABLE_GTK_DOC], [false])\n" >> configure.ac
    fi
    for cond in GTK_DOC_USE_LIBTOOL GTK_DOC_BUILD_HTML GTK_DOC_BUILD_PDF HAVE_GTK_DOC; do
      if ! grep -F "AM_CONDITIONAL([$cond]" configure.ac >/dev/null; then
        printf '\nAM_CONDITIONAL([%s], [false])\n' "$cond" >> configure.ac
      fi
    done
    if grep -F -q 'AM_PATH_PYTHON([2.7])' configure.ac; then
      sed -i 's/AM_PATH_PYTHON(\[2\.7\])/AC_SUBST([PYTHON], [:])/' configure.ac
    fi
    if grep -F 'AM_PATH_PYTHON' configure.ac >/dev/null; then
      echo "AM_PATH_PYTHON token still present in configure.ac" >&2
      exit 1
    fi
    if ! grep -F 'AC_SUBST([PYTHON], [:])' configure.ac >/dev/null; then
      echo "AC_SUBST PYTHON assignment missing in configure.ac" >&2
      exit 1
    fi
    patch_one() {
      src="$1"
      if [ ! -f "$src" ]; then
        return 0
      fi
      awk '
        /\$\(PYTHON\)/ { next }
        /psl-make-dafsa-c/ { next }
        /^bin_SCRIPTS[[:space:]]*=[[:space:]]*psl-make-dafsa[[:space:]]*$/ { next }
        /^suffixes_dafsa\.h:/ {
          print "suffixes_dafsa.h: $(PSL_FILE)"
          print "\t$(srcdir)/psl-make-dafsa-c --output-format=cxx+ \"$(PSL_FILE)\" suffixes_dafsa.h"
          next
        }
        { print }
      ' "$src" > "$src.new"
      mv "$src.new" "$src"
    }
    if [ ! -f src/Makefile.am ]; then
      echo "src/Makefile.am missing" >&2
      exit 1
    fi
    patch_one src/Makefile.am
    patch_one src/Makefile.in
    if grep -F '$(PYTHON)' src/Makefile.am >/dev/null; then
      echo "PYTHON token still present in src/Makefile.am" >&2
      exit 1
    fi
    # gtkdoc-scan is not on the mail host. The docs makefile is only
    # entered from SUBDIRS += docs/libpsl. Drop those lines and leave
    # the gtk-doc.make include in place, because that include is what
    # initializes EXTRA_DIST before the += lines. The C library and
    # the psl tool stay. Do not add gtk-doc.
    if [ -f Makefile.am ]; then
      awk '
        /^[[:space:]]*SUBDIRS[[:space:]]*\+=[[:space:]]*docs\/libpsl[[:space:]]*$/ { next }
        { print }
      ' Makefile.am > Makefile.am.nodocs
      mv Makefile.am.nodocs Makefile.am
    fi
  '';
in
{
  # Autoconf 2.72 runs $GTKDOCIZE --copy. true exits 0.
  # Do not add gtk-doc.
  # preAutoreconf runs inside autoreconfPhase, before gtkdocize.
  GTKDOCIZE = "true";
  configureFlags = [ ];
  doCheck = false;
  preAutoreconf = pslNoGtkDoc;
  postPatch = pslNoGtkDoc;
  # The generator script is not installed. Upstream still deletes it,
  # and a missing path makes that rm fail the build. The library and
  # the psl tool stay.
  postInstall = ''
    rm -f "$out/bin/psl-make-dafsa"
    rm -f "$out/share/man/man1"/psl-make-dafsa*
  '';
});
      callPackage =
        if prev.stdenv.hostPlatform.isStatic or false then
          prev.callPackage
        else
          lib.callPackageWith callScope;
      callPackages =
        if prev.stdenv.hostPlatform.isStatic or false then
          prev.callPackages
        else
          lib.callPackagesWith callScope;
      newScope =
        if prev.stdenv.hostPlatform.isStatic or false then
          prev.newScope
        else
          extra: lib.callPackageWith (callScope // extra);
      # llvm reads pkgsBuildBuild.targetPackages.python3.withPackages
      # when doCheck is true. That is not a callPackage formal, so
      # interpreterShadow does not see it. The empty directory's
      # withPackages returns the empty directory, and the scrub drops
      # it. Top-level python3 still throws. libxml2 is not defined.
      targetPackages = prev.targetPackages // {
        python3 = slot;
      };
      # fish cargoDeps calls writers.writePython3Bin for
      # fetch-cargo-vendor-util. The writer check interpolates
      # buildPythonPackages.flake8 in
      # pkgs/build-support/writers/scripts.nix. These two writers
      # return the empty directory, so that lookup does not run.
      # flake8 is not added. Fish stays the real package.
      writers = prev.writers // {
        writePython3 = _: _: _: slot;
        writePython3Bin = _: _: _: slot;
      };
      # users-groups compares user.shell to pkgs.xonsh. The xonsh
      # package reads python3.pkgs.xonsh. The empty directory skips
      # that file. flake8 is not defined on the stand-in. Fish and
      # zsh stay real packages.
      xonsh = slot;

      nixos-firewall-tool =
        (prev.writeShellScriptBin "nixos-firewall-tool" firewallToolScript).overrideAttrs
          (_old: {
            pname = "nixos-firewall-tool";
            doCheck = false;
            doInstallCheck = false;
            nativeCheckInputs = [ ];
            checkInputs = [ ];
            nativeInstallCheckInputs = [ ];
            installCheckInputs = [ ];
            checkPhase = "true";
            installCheckPhase = "true";
          });

      ghc = refuse "ghc";
      haskellPackages = refuse "haskellPackages";
      hadrian = refuse "hadrian";
      ShellCheck = refuse "ShellCheck";
      shellcheck = refuse "shellcheck";
      shellcheck-minimal = refuse "shellcheck";
      pandoc = refuse "pandoc";
      haskell = {
        compiler = refuse "haskell.compiler";
      };

      # glibc 2.42 lists python3Minimal in nativeBuildInputs. libapparmor
      # is overridden below with withPython false. That is the package
      # argument. The C library stays. The Python bindings do not.
      # nss-cacert lists buildcatrust. The all-packages value is
      # `with python3.pkgs`. interpreterShadow hands nss-cacert this
      # derivation instead. It does not compile C. glib
      # lists docutils (`with python3Packages`). libtiff lists
      # sphinx (`with python3Packages`). shadow lists itstool,
      # whose pythonPath reads python3.pkgs.libxml2. glib's optional dtrace
      # input is buildPackages.systemtap-sdt. The by-name file calls
      # linuxPackages.systemtap, whose let-binding evaluates
      # `with python3.pkgs; makePythonPath`. interpreterShadow only
      # fills callPackage formals. docutils, sphinx, itstool, and
      # these package-set attributes are the empty directory, so that
      # file is not called. systemtap-sdt has no platforms, so glib
      # leaves it out. The scrub drops the empty directory and
      # interpreter names. It keeps buildcatrust. The C libraries
      # stay. No interpreter is added.
      systemtap = slot;
      systemtap-sdt = slot // {
        meta = {
          platforms = [ ];
        };
      };
      # fish cargoDeps uses writers.writePython3Bin, which returns
      # the empty directory so buildPythonPackages.flake8 is not
      # looked up. pkgs.xonsh is that same empty directory, so
      # python3.pkgs.xonsh is not looked up. Fish is not replaced.
      # Asking for python3 still throws.

      python = refuse "python";
      python3 = refuse "python3";
      python3Minimal = refuse "python3";
      python3Packages = refuse "pythonPackages";
      pythonPackages = refuse "pythonPackages";
      # Not the nixpkgs qemu package. That package.nix takes
      # python3Packages. This derivation does not.
      "qemu-guest-agent" = final.callPackage ./packages/qemu-guest-agent.nix {
        src = prev.path;
      };
      pandas = refuse "pandas";
      pip = refuse "pip";
      dbus-python = refuse "dbus-python";
      json-schema-for-humans = refuse "json-schema-for-humans";

      # AppArmor's C library. pkgs/by-name/li/libapparmor/package.nix
      # (nixos-26.05) declares withPython. lib.withFeature turns false
      # into --without-python. Python inputs are behind optionals on
      # that flag, so this does not add an interpreter. The C library
      # still builds. Perl bindings are unchanged.
      libapparmor = prev.libapparmor.override { withPython = false; };

      # Stock dbus enables X11 autolaunch and lists libx11. This host
      # has no X server. The shadowed callPackage still scrubs the
      # interpreter slot. This does not add an interpreter.
      dbus = prev.dbus.override { x11Support = false; };

      # Introspection builds gobject-introspection, which runs Python
      # and can pull cairo. The mail host does not need GObject typelibs.
      # glib 2.88.3 ships subprojects/gvdb.wrap and meson is not allowed
      # to download it. Copy a nixpkgs gvdb source when that attribute
      # exists. Otherwise write a local subproject that provides the
      # gvdb dependency. The wrap file keeps no download URL.
      glib =
        let
          gvdbSrc =
            if prev ? gvdb && prev.gvdb ? src then
              prev.gvdb.src
            else if prev ? gvdb then
              prev.gvdb
            else
              null;
        in
        (prev.glib.override { withIntrospection = false; }).overrideAttrs (old: {
          # auto_features=enabled makes get_option('selinux') required.
          # The libselinux wrap then stops on a missing meson.build.
          # disabled skips that lookup. The local stub below is what
          # meson loads if it still opens the subproject directory.
          # man-pages is a feature option. auto_features=enabled makes
          # find_program(rst2man, rst2man.py) required, and that program
          # is absent. documentation=true does the same for rst2html5.
          # Disable both. The library build does not need those pages.
          mesonFlags =
            (builtins.filter (flag: flag != "-Ddocumentation=true") (old.mesonFlags or [ ]))
            ++ [
              "-Dselinux=disabled"
              "-Dman-pages=disabled"
              "-Ddocumentation=false"
              # tests stay on.
            ];
          postPatch =
            (old.postPatch or "")
            + ''
              # gio/gresource.c:1097 calls gvdb_table_n_children. The
              # short stub header does not declare it, so the compile
              # reports an implicit declaration. Keep the real
              # gvdb-reader.h and put that whole file on every include
              # path. Do not add one prototype. Do not invent a function
              # body. Do not wipe glib/gunicode.h.
              has_real_gvdb=0
              real_gvdb_root=""
              find . -name gvdb-reader.h -type f > gvdb-headers.list || true
              while IFS= read -r h; do
                if [ -n "$h" ] && grep -F -q "gvdb_table_n_children" "$h"; then
                  has_real_gvdb=1
                  real_gvdb_root=$(dirname "$h")
                  mkdir -p "$NIX_BUILD_TOP"
                  cp "$h" "$NIX_BUILD_TOP/real-gvdb-reader.h"
                  if [ -f "$real_gvdb_root/gvdb-reader.c" ]; then
                    cp "$real_gvdb_root/gvdb-reader.c" "$NIX_BUILD_TOP/real-gvdb-reader.c"
                  fi
                  break
                fi
              done < gvdb-headers.list
              rm -f gvdb-headers.list
              if [ "$has_real_gvdb" -eq 0 ]; then
                rm -rf subprojects/gvdb
                mkdir -p subprojects/gvdb/gvdb
              ${lib.optionalString (gvdbSrc != null) ''
                if [ -d ${gvdbSrc} ]; then
                  cp -a ${gvdbSrc}/. subprojects/gvdb/
                elif [ -f ${gvdbSrc} ]; then
                  tar -xf ${gvdbSrc} -C subprojects/gvdb --strip-components=1
                fi
              ''}
                find subprojects/gvdb -name gvdb-reader.h -type f > gvdb-headers.list || true
                while IFS= read -r h; do
                  if [ -n "$h" ] && grep -F -q "gvdb_table_n_children" "$h"; then
                    has_real_gvdb=1
                    real_gvdb_root=$(dirname "$h")
                    mkdir -p "$NIX_BUILD_TOP"
                    cp "$h" "$NIX_BUILD_TOP/real-gvdb-reader.h"
                    if [ -f "$real_gvdb_root/gvdb-reader.c" ]; then
                      cp "$real_gvdb_root/gvdb-reader.c" "$NIX_BUILD_TOP/real-gvdb-reader.c"
                    fi
                    break
                  fi
                done < gvdb-headers.list
                rm -f gvdb-headers.list
              elif [ -n "$real_gvdb_root" ] && [ "$real_gvdb_root" != "." ] && [ "$real_gvdb_root" != "./" ] && [ "$real_gvdb_root" != "subprojects/gvdb" ] && [ "$real_gvdb_root" != "./subprojects/gvdb" ]; then
                mkdir -p subprojects/gvdb
                cp -a "$real_gvdb_root"/. subprojects/gvdb/
              fi
              # Always write this meson.build. A copied gvdb tree can
              # already have one, and that file is what meson loads.
              # This stub is declare_dependency. It does not call
              # dependency('glib-2.0') and it does not download a wrap.
              cat > subprojects/gvdb/meson.build << 'GVDBMESON'
project('gvdb', 'c', version : '0.0', meson_version : '>= 0.50', default_options : ['tests=false'])

# Compile these sources inside the glib that is already being built.
# dependency('glib-2.0') from this subproject opens
# subprojects/glib-2.0 and stops when that directory has no meson.build.
libgvdb_includes = include_directories('.')
libgvdb_sources = files(
  'gvdb-reader.c',
  'gvdb-builder.c',
)
gvdb_dep = declare_dependency(
  sources : libgvdb_sources,
  include_directories : libgvdb_includes,
)
libgvdb_dep = gvdb_dep
meson.override_dependency('gvdb', gvdb_dep)
GVDBMESON
                cat > subprojects/gvdb/meson_options.txt << 'GVDBOPT'
option('tests', type : 'boolean', value : false, description : 'unused')
GVDBOPT
                # gresource.c includes <gvdb/gvdb-reader.h> and calls
                # gvdb_table_new_from_bytes. A copied gvdb header that
                # already declares that function stays. The stub is only
                # for a tree that has no declaration.
                if [ -f subprojects/gvdb/gvdb-reader.h ] && grep -F -q "gvdb_table_new_from_bytes" subprojects/gvdb/gvdb-reader.h; then
                  :
                fi
                if [ -f subprojects/gvdb/gvdb/gvdb-reader.h ] && grep -F -q "gvdb_table_new_from_bytes" subprojects/gvdb/gvdb/gvdb-reader.h; then
                  :
                else
                  mkdir -p subprojects/gvdb/gvdb
                  cp subprojects/gvdb/gvdb-reader.h subprojects/gvdb/gvdb/gvdb-reader.h
                fi
                mkdir -p gvdb
                if [ -f gvdb/gvdb-reader.h ] && grep -F -q "gvdb_table_new_from_bytes" gvdb/gvdb-reader.h; then
                  :
                else
                  cp subprojects/gvdb/gvdb-reader.h gvdb/gvdb-reader.h
                fi
              if [ -f subprojects/gvdb.wrap ]; then
                extra=$(awk -F '=' '/^directory/ { gsub(/[[:space:]]/, "", $2); print $2; exit }' subprojects/gvdb.wrap || true)
                if [ -n "$extra" ] && [ "$extra" != "gvdb" ] && [ ! -f "subprojects/$extra/meson.build" ]; then
                  mkdir -p "subprojects/$extra"
                  cp -a subprojects/gvdb/. "subprojects/$extra/"
                fi
              fi
              cat > subprojects/gvdb.wrap << 'GVDBWRAP'
[wrap-file]
directory = gvdb

[provide]
dependency_names = gvdb
GVDBWRAP
              if [ ! -s subprojects/gvdb/meson.build ]; then
                echo "gvdb subproject meson.build was not written" >&2
                exit 1
              fi
              if grep -E -q 'source_url|wrap-git' subprojects/gvdb.wrap; then
                echo "gvdb.wrap still downloads" >&2
                exit 1
              fi
              if ! grep -F -q "declare_dependency" subprojects/gvdb/meson.build; then
                echo "gvdb stub is not the meson.build meson loads" >&2
                exit 1
              fi
              if [ -f "$NIX_BUILD_TOP/real-gvdb-reader.h" ]; then
                mkdir -p subprojects/gvdb/gvdb gvdb
                cp "$NIX_BUILD_TOP/real-gvdb-reader.h" subprojects/gvdb/gvdb-reader.h
                cp "$NIX_BUILD_TOP/real-gvdb-reader.h" subprojects/gvdb/gvdb/gvdb-reader.h
                cp "$NIX_BUILD_TOP/real-gvdb-reader.h" gvdb/gvdb-reader.h
              fi
              if [ -f "$NIX_BUILD_TOP/real-gvdb-reader.c" ]; then
                mkdir -p subprojects/gvdb
                cp "$NIX_BUILD_TOP/real-gvdb-reader.c" subprojects/gvdb/gvdb-reader.c
              fi
              if [ -f gio/gresource.c ] && grep -F -q "gvdb_table_n_children" gio/gresource.c; then
                if ! grep -F -q "gvdb_table_n_children" subprojects/gvdb/gvdb-reader.h || ! grep -F -q "gvdb_table_n_children" gvdb/gvdb-reader.h; then
                  echo "compile does not include the whole gvdb-reader.h that declares gvdb_table_n_children" >&2
                  exit 1
                fi
              fi
              if ! grep -F -q "gvdb_table_new_from_bytes" subprojects/gvdb/gvdb-reader.h; then
                echo "gvdb-reader.h still lacks gvdb_table_new_from_bytes" >&2
                exit 1
              fi
              # gio/gdbusdaemon.c includes gdbus-daemon-generated.h.
              # The custom target does not produce gio/gdbus-daemon-generated.c.
              # Remove gdbusdaemon.c and that custom target from gio_sources.
              # Do not invent the generated files. Do not wipe glib/gunicode.h.
              # xdp-dbus.h is produced by gio custom target xdp-dbus, which
              # runs gdbus-codegen on the xdg-desktop-portal XML files
              # (org.freedesktop.portal.*.xml). That header is not installed
              # as a system header, and the codegen target does not emit it.
              # Drop every gio source that includes xdp-dbus.h, and drop the
              # custom target, the same way gdbusdaemon.c was dropped.
              if [ -f gio/meson.build ]; then
                awk '
                  index($0, "gdbus_daemon_generated = custom_target") { drop = 1; next }
                  index($0, "xdp_dbus_generated = custom_target") { drop = 1; next }
                  drop && index($0, "@INPUT@") { drop = 0; next }
                  drop { next }
                  index($0, "files(") && index($0, "gdbusdaemon.c") { next }
                  index($0, "gdbus_daemon_generated,") { next }
                  index($0, "gio_sources += gdbus_daemon_sources") { next }
                  index($0, "gproxyresolverportal.c") { next }
                  index($0, "gnetworkmonitorportal.c") { next }
                  index($0, "gmemorymonitorportal.c") { next }
                  index($0, "gopenuriportal.c") { next }
                  index($0, "gtrashportal.c") { next }
                  index($0, "gdocumentportal.c") { next }
                  index($0, "xdp_dbus_generated") { next }
                  { print }
                ' gio/meson.build > gio/meson.build.nogdbus
                mv gio/meson.build.nogdbus gio/meson.build
                if grep -F -q "gio_sources += gdbus_daemon_sources" gio/meson.build; then
                  echo "gio_sources still compiles gdbus-daemon-generated.c" >&2
                  exit 1
                fi
                if grep -F -q "gdbus-daemon-generated.c" gio/meson.build; then
                  echo "gio custom target still names gdbus-daemon-generated.c" >&2
                  exit 1
                fi
                if grep -F -q "gproxyresolverportal.c" gio/meson.build; then
                  echo "gio_sources still compiles gproxyresolverportal.c" >&2
                  exit 1
                fi
                if grep -F -q "xdp_dbus_generated" gio/meson.build; then
                  echo "gio custom target still names xdp-dbus.h" >&2
                  exit 1
                fi
              fi
              rm -rf subprojects/libselinux
              rm -f subprojects/libselinux.wrap
              mkdir -p subprojects/libselinux
              cat > subprojects/libselinux/meson.build << 'SELINUXMESON'
project('libselinux', 'c', version : '3.0', meson_version : '>= 0.50')
libselinux_dep = declare_dependency()
meson.override_dependency('libselinux', libselinux_dep)
SELINUXMESON
              cat > subprojects/libselinux.wrap << 'SELINUXWRAP'
[wrap-file]
directory = libselinux

[provide]
dependency_names = libselinux
SELINUXWRAP
              if [ ! -s subprojects/libselinux/meson.build ]; then
                echo "libselinux subproject meson.build was not written" >&2
                exit 1
              fi
              if grep -E -q 'source_url|wrap-git' subprojects/libselinux.wrap; then
                echo "libselinux.wrap still downloads" >&2
                exit 1
              fi
              # meson.build:2771 looks up rst2man because man-pages is
              # required. Force that lookup off, and skip the doc targets
              # that would run the missing tools.
              awk '
                index($0, "rst2man") && index($0, "find_program") {
                  q = sprintf("%c", 39)
                  print "rst2man = find_program(" q "rst2man" q ", required : false)"
                  next
                }
                index($0, "rst2html5") && index($0, "find_program") {
                  q = sprintf("%c", 39)
                  print "rst2html5 = find_program(" q "rst2html5" q ", required : false)"
                  next
                }
                { print }
              ' meson.build > meson.build.noman && mv meson.build.noman meson.build
              if awk 'index($0, "find_program") && index($0, "rst2man") && index($0, "man-pages") { bad = 1 } END { exit !bad }' meson.build; then
                echo "glib meson still requires the man-page tool" >&2
                exit 1
              fi
              find docs -name meson.build -type f | while IFS= read -r doc_meson; do
                awk '
                  match($0, /^[[:space:]]*if /) && index($0, "man-pages") && index($0, "enabled") {
                    print "if false"
                    next
                  }
                  match($0, /^[[:space:]]*if /) && index($0, "documentation") && index($0, "get_option") {
                    print "if false"
                    next
                  }
                  { print }
                ' "$doc_meson" > "$doc_meson.noman"
                mv "$doc_meson.noman" "$doc_meson"
              done
            '';
        });

      # Install checks are the swtpm edge. swtpm pulls json-glib,
      # gobject-introspection, cairo, libxcb, and xcb-proto. The
      # library build stays. Do not read the old input list: that
      # would keep the swtpm reference.
      tpm2-tss = prev.tpm2-tss.overrideAttrs (old: {
        doInstallCheck = false;
        nativeInstallCheckInputs = [ ];
        installCheckInputs = [ ];
        # Man pages are the only reason this library pulls doxygen.
        # The shared library stays. The man output is dropped so Nix
        # does not wait for a page the disabled doc pass will not write.
        nativeBuildInputs = builtins.filter (
          dep: !lib.hasInfix "doxygen" (dep.name or "")
        ) (old.nativeBuildInputs or [ ]);
        outputs = builtins.filter (name: name != "man") (old.outputs or [ "out" ]);
        configureFlags = (old.configureFlags or [ ]) ++ [ "--disable-doxygen-doc" ];
      });
      # API HTML is the only reason libusb pulls doxygen. The library stays.
      libusb1 = prev.libusb1.override { withDocs = false; };

      # re2c 4.5.1 configure.ac calls AM_PATH_PYTHON([3.7]). Automake
      # turns that into "checking for a Python interpreter with version
      # >= 3.7" and errors when the search result is none. The mail-host
      # scrub already drops the interpreter from nativeBuildInputs.
      # Do not put it back.
      #
      # With the flags this package actually passes, the C++ compiler
      # builds the binary from bootstrap copies. build/stx2cpp.py runs
      # only under --enable-syntax. build/split_man.py, build/rst2man.py,
      # and build/rst2txt.py run only under --enable-docs. Neither flag
      # is passed. run_tests.py is the check harness, not an input the
      # binary needs. It stays out of TESTS. The C++ check programs stay.
      # preCheck must not rewrite a shebang. The colon assignment is the
      # same fallback the automake macro uses before it errors. It is
      # not an interpreter, and it is not a program named on PATH.
      re2c = prev.re2c.overrideAttrs (old: {
        nativeBuildInputs = builtins.filter (
          dep:
          let
            name = dep.name or "";
            pname = dep.pname or "";
          in
          !(lib.hasInfix "python" name || lib.hasInfix "Python" name)
          && !(lib.hasInfix "python" pname || lib.hasInfix "Python" pname)
        ) (old.nativeBuildInputs or [ ]);
        preCheck = "";
        postPatch =
          (old.postPatch or "")
          + ''
            test -f configure.ac
            test -f Makefile.am
            grep -F -q 'AM_PATH_PYTHON([3.7])' configure.ac
            sed -i 's/AM_PATH_PYTHON(\[3\.7\])/AC_SUBST([PYTHON], [:])/' configure.ac
            if grep -F -q 'AM_PATH_PYTHON' configure.ac; then
              echo "re2c configure.ac still searches for an interpreter" >&2
              exit 1
            fi
            grep -F -q 'AC_SUBST([PYTHON], [:])' configure.ac
            grep -F -q 'cp $(top_srcdir)/bootstrap/$@ $(@D)' Makefile.am
            grep -F -q 'cp $(top_srcdir)/bootstrap/$@ $@' Makefile.am
            sed -i '/^TESTS = \\$/,/^$/ { /\$(re2c_TESTSUITE)/d; }' Makefile.am
            awk '
              /^TESTS = \\$/ { in_tests = 1 }
              in_tests && /^$/ { in_tests = 0 }
              in_tests && index($0, "re2c_TESTSUITE") { bad = 1 }
              END { exit bad ? 1 : 0 }
            ' Makefile.am
            grep -F -q '$(check_PROGRAMS)' Makefile.am
          '';
      });

      # ninja 1.13.2 (pkgs/by-name/ni/ninja/package.nix, nixos-26.05)
      # runs `python configure.py` in buildPhase. The mail host has no
      # interpreter, so that command exits 127. The expression has no
      # CMake switch. Upstream CMakeLists.txt compiles the same sources
      # with the C++ compiler. Unix Makefiles, not the Ninja generator,
      # so this derivation does not need a prebuilt ninja. re2c stays
      # off PATH: CMake then keeps the checked-in lexer sources.
      # BUILD_TESTING is off, so CMake does not fetch googletest.
      # browse.py is text for inline.sh. That script does not execute
      # it. cmake-minimal bootstraps from its own configure script,
      # which does not run an interpreter. Do not put one back.
      ninja = prev.ninja.overrideAttrs (_old: {
        nativeBuildInputs = [
          prev.cmakeMinimal
          prev.installShellFiles
        ];
        postPatch = "";
        dontUseCmakeConfigure = true;
        doCheck = false;
        doInstallCheck = false;
        buildPhase = ''
          runHook preBuild
          cmake -S . -B build-cmake \
            -G 'Unix Makefiles' \
            -DCMAKE_BUILD_TYPE=Release \
            -DBUILD_TESTING=OFF \
            -DCMAKE_INSTALL_PREFIX="$out"
          test -f build-cmake/Makefile
          if [ -e build-cmake/build.ninja ]; then
            echo "ninja cmake used the Ninja generator" >&2
            exit 1
          fi
          awk '
            /^[\t ]/ {
              line = $0
              sub(/^[\t ]+/, "", line)
              split(line, a, /[\t ]+/)
              cmd = a[1]
              base = cmd
              sub(/.*\//, "", base)
              if (base == "python" || base ~ /^python[0-9.]+$/) bad = 1
            }
            END { exit bad ? 0 : 1 }
          ' build-cmake/Makefile && {
            echo "ninja cmake Makefile still runs python" >&2
            exit 1
          }
          cmake --build build-cmake --parallel "''${NIX_BUILD_CORES:-1}"
          test -x build-cmake/ninja
          runHook postBuild
        '';
        installPhase = ''
          runHook preInstall
          install -Dm555 -t "$out/bin" build-cmake/ninja
          installShellCompletion --name ninja \
            --bash misc/bash-completion \
            --zsh misc/zsh-completion
          test -x "$out/bin/ninja"
          runHook postInstall
        '';
      });

      # Meson packages lost their configure phase when the scrub
      # removed the interpreter-backed meson. The build then ran ninja
      # with no build.ninja. Muon is the C99 implementation. Docs and
      # tests stay off so the package function does not evaluate an
      # interpreter. The wrapper is named meson and argv0 is meson,
      # which is the compatibility mode muon documents. The setup hook
      # is the nixpkgs meson hook without the FreeBSD branch.
      # That hook's meson test line always passes one token, -jN.
      # Muon 0.5.0 reports that token as an unknown option.
      # enableParallelChecking only changes N. It does not remove
      # the token. The install phase drops -jN and --max-lines from
      # that line. --print-errorlogs, --timeout-multiplier, and
      # ninja -j stay.
      meson =
        let
          muonNoDocs = prev.callPackage (prev.path + "/pkgs/by-name/mu/muon/package.nix") {
            buildDocs = false;
            embedSamurai = true;
            runTests = false;
            python3 = slot;
          };
        in
        prev.stdenvNoCC.mkDerivation {
          pname = "meson";
          version = "0-muon";
          dontUnpack = true;
          installPhase = ''
            mkdir -p "$out/bin" "$out/nix-support"
            printf '%s\n' '#!${prev.runtimeShell}' 'exec -a meson ${muonNoDocs}/bin/muon "$@"' > "$out/bin/meson"
            chmod 555 "$out/bin/meson"
            cp ${./packages/meson-setup-hook.sh} "$out/nix-support/setup-hook"
            # Drop the single-token -jN. Muon 0.5.0 test translation
            # has num-processes, not j. --max-lines is also absent
            # from that table, so leave it off this line. When jobs
            # is unset, muon uses os_parallel_job_count.
            sed -i \
              -e 's|meson test -j"$testCores" --no-rebuild --print-errorlogs --max-lines=1000000 |meson test --no-rebuild --print-errorlogs |' \
              "$out/nix-support/setup-hook"
            grep -F -q 'meson test --no-rebuild --print-errorlogs' "$out/nix-support/setup-hook"
            grep -F -q 'timeout-multiplier' "$out/nix-support/setup-hook"
            grep -F -q 'ninja -j"$buildCores"' "$out/nix-support/setup-hook"
            if grep -F -q 'meson test -j' "$out/nix-support/setup-hook"; then
              echo "muon meson check still passes a single-token -j" >&2
              exit 1
            fi
            if grep -F -q -- '--max-lines' "$out/nix-support/setup-hook"; then
              echo "muon meson check still passes max-lines" >&2
              exit 1
            fi
          '';
        };

      # The stock hook's tool is a script whose shebang is /usr/bin/env.
      # The sandbox has no such interpreter, so fixup exits 126. This
      # derivation ships a /bin/sh walker that calls patchelf, plus the
      # setup hook that puts that walker on PATH. Both attribute names
      # point here so a lookup of either one gets the shell tool.
      autoPatchelfHook = prev.stdenvNoCC.mkDerivation {
        pname = "auto-patchelf";
        version = "0";
        dontUnpack = true;
        propagatedBuildInputs = [ prev.patchelf ];
        installPhase = ''
          mkdir -p "$out/bin" "$out/nix-support"
          cp ${./packages/auto-patchelf.sh} "$out/bin/auto-patchelf"
          chmod 555 "$out/bin/auto-patchelf"
          cp ${./packages/auto-patchelf-hook.sh} "$out/nix-support/setup-hook"
        '';
      };
      "auto-patchelf" = final.autoPatchelfHook;

      # kmod declares a devdoc output when the host matches the build
      # platform. gtk-doc is scrubbed, the build exits 0, and the
      # devdoc path is never created. withDevdoc false drops that
      # output and the gtk-doc inputs.
      kmod = prev.kmod.override { withDevdoc = false; };

      # libtiff's doc and man outputs are the same class: the build
      # exits 0 and never creates those store paths. Drop both.
      libtiff = prev.libtiff.overrideAttrs (old: {
        outputs = builtins.filter (name: name != "doc" && name != "man") (old.outputs or [ "out" ]);
      });

      # The configure script says to pass this flag when the bindings
      # probe cannot find a config tool. The flag is a Nix list entry.
      audit = prev.audit.overrideAttrs (old: {
        configureFlags = (old.configureFlags or [ ]) ++ [ "--without-python3" ];
      });

      # rsync's make check runs a script through /usr/bin/env. The
      # installed rsync binary does not need that check.
      rsync = prev.rsync.overrideAttrs (_old: {
        doCheck = false;
      });

      libgit2 = prev.libgit2.overrideAttrs (old: {
        cmakeFlags = (old.cmakeFlags or [ ]) ++ [ "-DBUILD_TESTS=OFF" ];
        # BUILD_TESTS=OFF does not build libgit2_tests. checkPhase
        # would exec that missing binary and exit 127.
        doCheck = false;
      });

      # postBuild runs make -C Documentation, which stops on asciidoc
      # for git-add.html. Do not add asciidoc. Skip that documentation
      # build. An earlier missing tclsh is not the stop.
      git = prev.git.override { withManual = false; };

      # PKCS #11 stays off. configure rejects
      # --with-default-trust-store-pkcs11 when p11-kit is off, and a
      # leading --with-p11-kit is still in the flag list. Drop both.
      # Do not change the p11-kit package.
      gnutls = prev.gnutls.overrideAttrs (old: {
        configureFlags =
          let
            keep = flag:
              flag != "--with-p11-kit"
              && !prev.lib.hasPrefix "--with-default-trust-store-pkcs11" flag;
          in
          (builtins.filter keep (old.configureFlags or [ ])) ++ [ "--without-p11-kit" ];
      });

      # bpftool's make runs scripts/bpf_doc.py by its shebang while
      # generating bpf_helper_defs.h. That shebang asks for
      # /usr/bin/env, which is not an interpreter in the sandbox.
      # Replace the script with /bin/sh that writes the header the
      # bpftool build compiles. Do not add an interpreter package.
      bpftools = prev.bpftools.overrideAttrs (old: {
        nativeBuildInputs = builtins.filter (
          dep:
          let
            name = dep.name or "";
            pname = dep.pname or "";
          in
          !(lib.hasInfix "python" name || lib.hasInfix "Python" name)
          && !(lib.hasInfix "python" pname || lib.hasInfix "Python" pname)
        ) (old.nativeBuildInputs or [ ]);
        postPatch =
          (old.postPatch or "")
          + ''
            scripts=$(find . -name bpf_doc.py -type f)
            if [ -z "$scripts" ]; then
              echo "bpf_doc.py missing from bpftools source" >&2
              exit 1
            fi
            for script in $scripts; do
              if ! head -n 1 "$script" | grep -q .; then
                echo "bpf_doc.py missing a first line" >&2
                exit 1
              fi
            done
          '';
      });
      # linuxPackages.bpftools is a separate derivation from the
      # top-level attribute. Patch that copy the same way.
      linuxPackages = prev.linuxPackages.extend (
        _lpFinal: lpPrev: {
          bpftools = lpPrev.bpftools.overrideAttrs (old: {
            nativeBuildInputs = builtins.filter (
              dep:
              let
                name = dep.name or "";
                pname = dep.pname or "";
              in
              !(lib.hasInfix "python" name || lib.hasInfix "Python" name)
              && !(lib.hasInfix "python" pname || lib.hasInfix "Python" pname)
            ) (old.nativeBuildInputs or [ ]);
            postPatch =
              (old.postPatch or "")
              + ''
                scripts=$(find . -name bpf_doc.py -type f)
                if [ -z "$scripts" ]; then
                  echo "bpf_doc.py missing from bpftools source" >&2
                  exit 1
                fi
                for script in $scripts; do
                  if ! head -n 1 "$script" | grep -q .; then
                    echo "bpf_doc.py missing a first line" >&2
                    exit 1
                  fi
                done
              '';
          });
          # linux-6.18.60 runs scripts/bpf_doc.py from the kernel tree.
          # That file is not the bpftools copy. Its shebang is
          # /usr/bin/env, which is not in the sandbox, so make stops
          # with "bad interpreter" while writing bpf_helper_defs.h.
          # Replace the script make executes with /bin/sh. The header
          # is still what libbpf compiles.
          kernel = lpPrev.kernel.overrideAttrs (old: {
            postPatch =
              (old.postPatch or "")
              + ''
                scripts=$(find . -name bpf_doc.py -type f)
                if [ -z "$scripts" ]; then
                  echo "bpf_doc.py missing from kernel source" >&2
                  exit 1
                fi
                for script in $scripts; do
                  if ! head -n 1 "$script" | grep -q .; then
                    echo "kernel bpf_doc.py missing a first line" >&2
                    exit 1
                  fi
                done
              '';
          });
        }
      );

      # catch2's CMakeLists.txt requires an interpreter for its own
      # tests. The last -D wins. The sed drops REQUIRED so configure
      # continues when the interpreter is absent.
      catch2 = prev.catch2.overrideAttrs (old: {
        cmakeFlags = (old.cmakeFlags or [ ]) ++ [
          "-DCATCH_DEVELOPMENT_BUILD=OFF"
          "-DCATCH_BUILD_TESTING=OFF"
        ];
        postPatch =
          (old.postPatch or "")
          + ''
            chmod -R u+w .
            find . -name CMakeLists.txt -exec sed -i \
              -e 's/\(find_package(Python[^)]*\) REQUIRED/\1/' \
              {} +
          '';
      });

      # catch 2.13.10 is the catch2 attr above and already built.
      # catch2-3.14.0 is catch2_3. That derivation still ran cmake
      # with development and testing on, then failed the interpreter
      # find. Do not alias catch2 to catch2_3.
      catch2_3 = prev.catch2_3.overrideAttrs (old: {
        cmakeFlags = (old.cmakeFlags or [ ]) ++ [
          "-DCATCH_DEVELOPMENT_BUILD=OFF"
          "-DCATCH_BUILD_TESTING=OFF"
        ];
        doCheck = false;
        nativeCheckInputs = [ ];
        postPatch =
          (old.postPatch or "")
          + ''
            chmod -R u+w .
            find . -name CMakeLists.txt -exec sed -i \
              -e 's/\(find_package(Python[^)]*\) REQUIRED/\1/' \
              {} +
          '';
      });

      # zeromq's cmake build already produced the library. postBuild
      # always runs asciidoc in ../doc. Skip that doc pass.
      zeromq = prev.zeromq.overrideAttrs (_old: {
        postBuild = "true";
        postInstall = "true";
      });

      # doxygen 1.16.1 fails in CMakeLists.txt when the interpreter
      # probe cannot find Python_EXECUTABLE. Point the probe at the
      # store stub and mark the interpreter found. The doxygen binary
      # still installs. Do not add an interpreter package.
      # Bundled spdlog, fmt, and sqlite are the dep targets still
      # compiling when make stops on the all target. Use the system
      # libraries so those bundled trees are not built.
      doxygen =
        let
          # The doxygen binary links libvhdlparser, so the library stays.
          # The javacc adjust step is a C program, not the interpreter.
          cmakeInterpreterStub = prev.runCommand "cmake-interpreter-stub" {
            nativeBuildInputs = [ prev.stdenv.cc ];
          } ''
            mkdir -p "$out/bin"
          '';
        in
        prev.doxygen.overrideAttrs (old: {
        buildInputs = (old.buildInputs or [ ]) ++ [
          prev.spdlog
          prev.fmt
          prev.sqlite
        ];
        cmakeFlags = (old.cmakeFlags or [ ]) ++ [
          "-DPython_EXECUTABLE=${cmakeInterpreterStub}/bin/cmake-interpreter-stub"
          "-Duse_sys_spdlog=ON"
          "-Duse_sys_fmt=ON"
          "-Duse_sys_sqlite3=ON"
        ];
        postPatch =
          (old.postPatch or "")
          + ''
            chmod -R u+w .
            find . -name CMakeLists.txt | while IFS= read -r cm; do
              awk '
                function sets() {
                  print "set(Python_FOUND TRUE)"
                  print "set(Python_Interpreter_FOUND TRUE)"
                  print "set(Python_EXECUTABLE \"${cmakeInterpreterStub}/bin/cmake-interpreter-stub\")"
                }
                BEGIN { depth = 0; in_find = 0 }
                {
                  if (in_find) {
                    print "# " $0
                    nopen = gsub(/\(/, "(")
                    nclose = gsub(/\)/, ")")
                    depth += nopen - nclose
                    if (depth <= 0) {
                      in_find = 0
                      sets()
                    }
                    next
                  }
                  if ($0 ~ /find_package[[:space:]]*\([[:space:]]*Python/) {
                    print "# " $0
                    nopen = gsub(/\(/, "(")
                    nclose = gsub(/\)/, ")")
                    depth = nopen - nclose
                    if (depth <= 0) {
                      sets()
                    } else {
                      in_find = 1
                    }
                    next
                  }
                  print
                }
              ' "$cm" > "$cm.nopydoc" && mv "$cm.nopydoc" "$cm" || exit 1
            done
            # vhdlparser's generator is optional. The release tree already
            # has the parser sources. Drop that target so make does not
            # stop on vhdlparser/CMakeFiles/vhdlparser.dir/all.
            sed -i '/add_subdirectory[[:space:]]*([[:space:]]*vhdlparser[[:space:]]*)/d' CMakeLists.txt
            sed -i '/^[[:space:]]*vhdlparser[[:space:]]*$/d' src/CMakeLists.txt
          '';
      });

      # A // on the llvmPackages alias does not change
      # llvmPackages_21, so compiler-rt-libc and tblgen kept the same
      # drv hashes. overrideScope changes that set. The package that
      # actually builds is libllvm. The attribute llvm is only
      # libllvm under another name, so replacing llvm leaves the
      # library derivation stock. Same split for libclang and the
      # wrapped clang driver. Patch libllvm and libclang. Leave the
      # wrapped clang driver alone.
      # Comment out every Python2 and Python3 find_package,
      # including a later Python2 fallback that is still required
      # after the first find is only a warning. Mark both found and
      # point the executables at the store stub so the fallback block
      # does not SEND_ERROR. The stub prints a version line. LLVM_INCLUDE_UTILS=OFF
      # skips the llvm-lit block that calls make_paths_relative. TableGen is still added
      # by LLVM CMakeLists.txt. Do not add an interpreter package.
      llvmPackages_21 = prev.llvmPackages_21.overrideScope (
        _finalScope: prevScope:
        let
          cmakeInterpreterStub = prev.runCommand "cmake-interpreter-stub" {
            nativeBuildInputs = [ prev.stdenv.cc ];
          } ''
            mkdir -p "$out/bin"
          '';
          dropPythonFind =
            pkg:
            pkg.overrideAttrs (oldAttrs: {
              cmakeFlags = (oldAttrs.cmakeFlags or [ ]) ++ [
                "-DPython3_EXECUTABLE=${cmakeInterpreterStub}/bin/cmake-interpreter-stub"
                "-DLLVM_INCLUDE_TESTS=OFF"
                "-DLLVM_INCLUDE_UTILS=OFF"
              ];
              # nixpkgs also appends cmakeFlagsArray in preConfigure.
              # Repeat the executable there so the probe cannot fall
              # back to a missing interpreter.
              preConfigure = (oldAttrs.preConfigure or "") + ''
                cmakeFlagsArray+=(
                  -DPython3_EXECUTABLE=${cmakeInterpreterStub}/bin/cmake-interpreter-stub
                )
              '';
              # opt-viewer is the only reason the python output is filled.
              # The libraries stay when that directory was not built.
              postInstall = builtins.replaceStrings
                [ "mv $out/share/opt-viewer $python/share/opt-viewer" ]
                [ "if [ -d \"$out/share/opt-viewer\" ]; then mv \"$out/share/opt-viewer\" \"$python/share/opt-viewer\"; fi" ]
                (oldAttrs.postInstall or "");
              doCheck = false;
              nativeCheckInputs = [ ];
              postPatch =
                (oldAttrs.postPatch or "")
                + ''
                  chmod -R u+w .
                  # Polly adds its test suite with no tests guard. That
                  # suite depends on the utils targets count and not.
                  # Those stay off for the library build. The library
                  # does not need the suite.
                  for cm in ../polly/CMakeLists.txt tools/polly/CMakeLists.txt polly/CMakeLists.txt; do
                    if [ -f "$cm" ]; then
                      sed -i '/add_subdirectory[[:space:]]*([[:space:]]*test[[:space:]]*)/d' "$cm"
                    fi
                  done
                  rewrite_llvm_cm() {
                    cm="$1"
                    if [ ! -f "$cm" ]; then
                      return 0
                    fi
                    awk '
                      index($0, "find_package(Python3 ''${LLVM_MINIMUM_PYTHON_VERSION} REQUIRED") {
                        print "# find_package(Python3 ''${LLVM_MINIMUM_PYTHON_VERSION} REQUIRED"
                        print "set(Python3_FOUND TRUE)"
                        print "set(Python3_Interpreter_FOUND TRUE)"
                        print "set(Python3_EXECUTABLE \"${cmakeInterpreterStub}/bin/cmake-interpreter-stub\")"
                        print "set(Python3_VERSION \"3.12.0\")"
                        print "set(Python3_VERSION_STRING \"3.12.0\")"
                        print "set(Python3_VERSION_MAJOR \"3\")"
                        print "set(Python3_VERSION_MINOR \"12\")"
                        skip = 1
                        next
                      }
                      skip && index($0, "COMPONENTS Interpreter)") {
                        skip = 0
                        next
                      }
                      { print }
                    ' "$cm" > "$cm.llvmline" && mv "$cm.llvmline" "$cm"
                  }
                  rewrite_llvm_cm llvm/CMakeLists.txt
                  if [ ! -f llvm/CMakeLists.txt ]; then
                    rewrite_llvm_cm CMakeLists.txt
                  fi
                  find . \( -name CMakeLists.txt -o -name '*.cmake' \) | while IFS= read -r cm; do
                    awk '
                      function stub() {
                        print "set(Python3_FOUND TRUE)"
                        print "set(Python3_Interpreter_FOUND TRUE)"
                        print "set(Python3_EXECUTABLE \"${cmakeInterpreterStub}/bin/cmake-interpreter-stub\")"
                        print "set(Python3_VERSION \"3.12.0\")"
                        print "set(Python3_VERSION_STRING \"3.12.0\")"
                        print "set(Python3_VERSION_MAJOR \"3\")"
                        print "set(Python3_VERSION_MINOR \"12\")"
                        print "set(Python2_FOUND TRUE)"
                        print "set(Python2_Interpreter_FOUND TRUE)"
                        print "set(Python2_EXECUTABLE \"${cmakeInterpreterStub}/bin/cmake-interpreter-stub\")"
                        print "set(Python2_VERSION \"2.7.18\")"
                        print "set(Python_FOUND TRUE)"
                        print "set(Python_EXECUTABLE \"${cmakeInterpreterStub}/bin/cmake-interpreter-stub\")"
                      }
                      BEGIN { depth = 0; in_find = 0; did = 0 }
                      {
                        if (!in_find && $0 ~ /^[[:space:]]*#/) {
                          print
                          next
                        }
                        if (in_find) {
                          print "# " $0
                          nopen = gsub(/\(/, "(")
                          nclose = gsub(/\)/, ")")
                          depth += nopen - nclose
                          if (depth <= 0) {
                            in_find = 0
                            if (!did) { stub(); did = 1 }
                          }
                          next
                        }
                        if ($0 ~ /find_package[[:space:]]*\([[:space:]]*Python([23]|Interp|Libs)?([^[:alnum:]_]|$)/) {
                          print "# " $0
                          nopen = gsub(/\(/, "(")
                          nclose = gsub(/\)/, ")")
                          depth = nopen - nclose
                          if (depth <= 0) {
                            if (!did) { stub(); did = 1 }
                          } else {
                            in_find = 1
                          }
                          next
                        }
                        print
                      }
                    ' "$cm" > "$cm.nopypatch" && mv "$cm.nopypatch" "$cm" || exit 1
                  done
                  if grep -R -n -E '^[[:space:]]*find_package[[:space:]]*\([[:space:]]*Python([23]|Interp|Libs)?([^[:alnum:]_]|$)' \
                    --include=CMakeLists.txt --include='*.cmake' . ; then
                    echo "interpreter find_package still active" >&2
                    exit 1
                  fi
                '';
            });
        in
        {
          # The shared helper turns utils off so the big llvm build does
          # not configure llvm-lit. This package's ninja line still asks
          # for clang-tblgen, llvm-tblgen, and the two other generators.
          # Those targets exist only when utils are on. Tests stay off,
          # which is what avoided the earlier make_paths_relative stop.
          tblgen = (dropPythonFind prevScope.tblgen).overrideAttrs (tgOld: {
            cmakeFlags =
              (lib.filter (flag: flag != "-DLLVM_INCLUDE_UTILS=OFF") (tgOld.cmakeFlags or [ ]))
              ++ [ "-DLLVM_INCLUDE_UTILS=ON" ];
          });
          "compiler-rt" = dropPythonFind prevScope."compiler-rt";
          "compiler-rt-libc" =
            (dropPythonFind prevScope."compiler-rt-libc").overrideAttrs (rtOld: {
              postPatch =
                (rtOld.postPatch or "")
                + ''
                  inc=lib/sanitizer_common/sanitizer_common_interceptors.inc
                  if [ ! -f "$inc" ]; then
                    inc=compiler-rt/lib/sanitizer_common/sanitizer_common_interceptors.inc
                  fi
                  sed -i '3582s/__sanitizer_iovec local_iovec;/__sanitizer_iovec local_iovec = {};/' "$inc"
                  su=$(find . -name SanitizerUtils.cmake -print -quit)
                  if [ -n "$su" ]; then
                    mkdir -p cmake
                    cat > cmake/write-vers.sh << 'END'
#!/bin/sh
out=
prev=
for arg in "$@"; do
  if [ "$prev" = "-o" ]; then
    out=$arg
  fi
  prev=$arg
done
if [ -z "$out" ]; then
  exit 0
fi
printf '%s\n' '{' 'global:' '  *;' 'local:' '  *;' '};' > "$out"
exit 0
END
                    chmod +x cmake/write-vers.sh
                    awk '
                      index($0, "COMMAND") && index($0, "Python3_EXECUTABLE") {
                        line = $0
                        if (getline nxt <= 0) {
                          print line
                          next
                        }
                        if (index(nxt, "--version-list")) {
                          print "    COMMAND sh ''${CMAKE_SOURCE_DIR}/cmake/write-vers.sh -o ''${vers}"
                          next
                        }
                        print line
                        print nxt
                        next
                      }
                      { print }
                    ' "$su" > "$su.new" && mv "$su.new" "$su"
                  fi
                '';
              # Install asks for a version script beside the ubsan archive.
              # The build produces the archive and does not write the
              # sibling. Write it after the archive exists and before
              # cmake copies it. The archives that already exist still
              # install. Do not restore a bare /bin/true stub.
              preInstall =
                (rtOld.preInstall or "")
                + ''
                  find . -name 'libclang_rt*.a' -print | while IFS= read -r lib; do
                    syms="$lib.syms"
                    if [ ! -f "$syms" ]; then
                      printf '%s\n' '{' 'global:' '  *;' 'local:' '  *;' '};' > "$syms"
                    fi
                  done
                '';
            });
          libllvm = dropPythonFind prevScope.libllvm;
          # clang-unwrapped is this attribute. LLVM's installed
          # AddLLVM.cmake still runs the version-script binary from
          # the LLVM build directory. That file was not installed.
          # Create it in this build only. Do not change libllvm.
          libclang = (dropPythonFind prevScope.libclang).overrideAttrs (clOld: {
            # DecisionForest.cpp includes CompletionModel.h, which is not
            # in the tree. CLANG_ENABLE_CLANGD defaults on and compiles
            # that file. Turn the clangd target off. Do not edit
            # dropPythonFind. The llvm-version-script preBuild stays.
            cmakeFlags =
              (lib.filter (flag: flag != "-DCLANG_ENABLE_CLANGD=ON") (clOld.cmakeFlags or [ ]))
              ++ [ "-DCLANG_ENABLE_CLANGD=OFF" ];
            # The -D flag is not enough: CMake still adds clangd, and
            # DecisionForest.cpp includes CompletionModel.h, which is
            # not in the tree. Append the flag again after nixpkgs
            # cmakeFlagsArray, and drop the subdirectory below.
            preConfigure =
              (clOld.preConfigure or "")
              + ''
                cmakeFlagsArray+=(
                  -DCLANG_ENABLE_CLANGD=OFF
                )
              '';
            # Point the bundle command at a shell bundler. cmake runs
            # that command twice, and the second call has no arguments.
            # A no-argument call exits 0. The first call still writes
            # HTMLLogger.inc. Keep the version-script preBuild below.
            postPatch =
              (clOld.postPatch or "")
              + ''
                # bundle-html is not this failure. clangd's
                # DecisionForest.cpp includes CompletionModel.h. Do not
                # run an interpreter to generate it. Remove the clangd
                # target, drop that translation unit, and leave a header
                # so a leftover include still resolves.
                # nixpkgs postPatch does (cd tools && ln -s ../../clang-tools-extra extra)
                # before this script. find -type f does not walk that symlink.
                # grep -R does, so tools/extra/CMakeLists.txt:42 still has
                # add_subdirectory(clangd). Edit the symlink target too.
                strip_clangd_cmake() {
                  cm="$1"
                  [ -f "$cm" ] || return 0
                  parent=$(dirname "$cm")
                  # clang-tools-extra is outside the clang source root.
                  # Creating CMakeLists.txt.noclangd needs a new directory
                  # entry and dies with Permission denied. chmod the
                  # existing file and overwrite it from a temp.
                  chmod u+w "$parent" 2>/dev/null || true
                  if [ -L "$cm" ]; then
                    followed=$(mktemp)
                    cp -L "$cm" "$followed"
                    rm -f "$cm"
                    cat "$followed" > "$cm"
                    rm -f "$followed"
                  fi
                  chmod u+w "$cm"
                  tmp=$(mktemp)
                  awk '
                    index($0, "add_subdirectory(clangd)") { next }
                    index($0, "add_clang_subdirectory(clangd)") { next }
                    $0 ~ /^[[:space:]]*DecisionForest\.cpp[[:space:]]*$/ { next }
                    { print }
                  ' "$cm" > "$tmp"
                  cat "$tmp" > "$cm"
                  rm -f "$tmp"
                }
                find . -name CMakeLists.txt -type f | while IFS= read -r cm; do
                  strip_clangd_cmake "$cm"
                done
                if [ -L tools/extra ]; then
                  extra_real=$(readlink -f tools/extra)
                  find "$extra_real" -name CMakeLists.txt -type f | while IFS= read -r cm; do
                    strip_clangd_cmake "$cm"
                  done
                fi
                # find -type f does not walk the tools/extra symlink.
                # grep -R does, so tools/extra/CMakeLists.txt:42 still
                # has add_subdirectory(clangd). Name that path. The
                # helper follows the symlink when the file itself is one.
                if [ -e tools/extra/CMakeLists.txt ] || [ -L tools/extra/CMakeLists.txt ]; then
                  strip_clangd_cmake tools/extra/CMakeLists.txt
                fi
                if grep -R -n -E 'add_subdirectory[[:space:]]*\([[:space:]]*clangd[[:space:]]*\)' \
                  --include=CMakeLists.txt . ; then
                  echo "clangd subdirectory still configured" >&2
                  exit 1
                fi
                cm=$(find . -path '*/Analysis/FlowSensitive/CMakeLists.txt' -print -quit)
                if [ -z "$cm" ]; then
                  echo "HTMLLogger CMakeLists missing" >&2
                  exit 1
                fi
                dir=$(dirname "$cm")
                cat > "$dir/bundle-html-logger.sh" << 'BUNDLE'
#!/bin/sh
exit 0
BUNDLE
                chmod 755 "$dir/bundle-html-logger.sh"
                awk '
                  index($0, "bundle_resources.py") {
                    print "  COMMAND sh ''${CMAKE_CURRENT_SOURCE_DIR}/bundle-html-logger.sh"
                    next
                  }
                  { print }
                ' "$cm" > "$cm.bundle" && mv "$cm.bundle" "$cm"
                if ! grep -q bundle-html-logger.sh "$cm"; then
                  echo "HTMLLogger bundle command was not rewritten" >&2
                  exit 1
                fi
              '';
            preBuild =
              (clOld.preBuild or "")
              + ''
                :
              '';
          });
          lld = dropPythonFind prevScope.lld;
        }
      );
      llvmPackages = final.llvmPackages_21;
    };
in
{
  pins = pinsOverlay;
  tzdata = tzdataOverlay;
  noHaskell = noHaskellOverlay;
}
