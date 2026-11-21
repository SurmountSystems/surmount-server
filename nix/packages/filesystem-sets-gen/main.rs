//! Std-only generator for the filesystem-sets capture modes.
//! Edition 2021. Tables follow the filesystem-sets script output shape.
//! The header declares fs_type_to_string(statfs_f_type_t) and
//! fs_in_group(const struct statfs *, enum FilesystemGroups). The script
//! defines the second parameter as `st` and the group parameter as the
//! FilesystemGroups typedef. Those forms are the captured C text.

use std::collections::BTreeSet;
use std::io::{self, Write};
use std::process::ExitCode;

struct Fs {
    name: &'static str,
    magics: &'static [&'static str],
    obsolete: bool,
}

struct Group {
    name: &'static str,
    help: &'static str,
    members: &'static [&'static str],
}

const FILESYSTEMS: &[Fs] = &[
    Fs {
        name: "apparmorfs",
        magics: &["AAFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "adfs",
        magics: &["ADFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "affs",
        magics: &["AFFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "afs",
        magics: &["AFS_FS_MAGIC", "AFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "anon_inodefs",
        magics: &["ANON_INODE_FS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "autofs",
        magics: &["AUTOFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "balloon-kvm",
        magics: &["BALLOON_KVM_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "bcachefs",
        magics: &["BCACHEFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "bdev",
        magics: &["BDEVFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "binder",
        magics: &["BINDERFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "binfmt_misc",
        magics: &["BINFMTFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "bpf",
        magics: &["BPF_FS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "btrfs",
        magics: &["BTRFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "btrfs_test_fs",
        magics: &["BTRFS_TEST_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "cpuset",
        magics: &["CGROUP_SUPER_MAGIC"],
        obsolete: true,
    },
    Fs {
        name: "ceph",
        magics: &["CEPH_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "cgroup2",
        magics: &["CGROUP2_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "cgroup",
        magics: &["CGROUP_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "cifs",
        magics: &["CIFS_SUPER_MAGIC", "SMB2_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "coda",
        magics: &["CODA_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "configfs",
        magics: &["CONFIGFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "cramfs",
        magics: &["CRAMFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "dax",
        magics: &["DAXFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "debugfs",
        magics: &["DEBUGFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "devmem",
        magics: &["DEVMEM_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "devpts",
        magics: &["DEVPTS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "devtmpfs",
        magics: &["TMPFS_MAGIC"],
        obsolete: true,
    },
    Fs {
        name: "dmabuf",
        magics: &["DMA_BUF_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "ecryptfs",
        magics: &["ECRYPTFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "efivarfs",
        magics: &["EFIVARFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "efs",
        magics: &["EFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "erofs",
        magics: &["EROFS_SUPER_MAGIC_V1"],
        obsolete: false,
    },
    Fs {
        name: "ext2",
        magics: &["EXT2_SUPER_MAGIC"],
        obsolete: true,
    },
    Fs {
        name: "ext3",
        magics: &["EXT3_SUPER_MAGIC"],
        obsolete: true,
    },
    Fs {
        name: "ext4",
        magics: &["EXT4_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "exfat",
        magics: &["EXFAT_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "f2fs",
        magics: &["F2FS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "failfs",
        magics: &["FAIL_FS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "fuseblk",
        magics: &["FUSE_SUPER_MAGIC"],
        obsolete: true,
    },
    Fs {
        name: "fuse",
        magics: &["FUSE_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "fusectl",
        magics: &["FUSE_CTL_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "gfs",
        magics: &["GFS2_MAGIC"],
        obsolete: true,
    },
    Fs {
        name: "gfs2",
        magics: &["GFS2_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "gmem",
        magics: &["GUEST_MEMFD_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "hostfs",
        magics: &["HOSTFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "hpfs",
        magics: &["HPFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "hugetlbfs",
        magics: &["HUGETLBFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "iso9660",
        magics: &["ISOFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "jffs2",
        magics: &["JFFS2_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "minix",
        magics: &[
            "MINIX_SUPER_MAGIC",
            "MINIX_SUPER_MAGIC2",
            "MINIX2_SUPER_MAGIC",
            "MINIX2_SUPER_MAGIC2",
            "MINIX3_SUPER_MAGIC",
        ],
        obsolete: false,
    },
    Fs {
        name: "mqueue",
        magics: &["MQUEUE_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "msdos",
        magics: &["MSDOS_SUPER_MAGIC"],
        obsolete: true,
    },
    Fs {
        name: "ncp",
        magics: &["NCP_SUPER_MAGIC"],
        obsolete: true,
    },
    Fs {
        name: "ncpfs",
        magics: &["NCP_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "nfs",
        magics: &["NFS_SUPER_MAGIC"],
        obsolete: true,
    },
    Fs {
        name: "nfs4",
        magics: &["NFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "nilfs2",
        magics: &["NILFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "nsfs",
        magics: &["NSFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "ntfs",
        magics: &["NTFS_SB_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "ntfs3",
        magics: &["NTFS3_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "nullfs",
        magics: &["NULL_FS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "ocfs2",
        magics: &["OCFS2_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "openpromfs",
        magics: &["OPENPROM_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "orangefs",
        magics: &["ORANGEFS_DEVREQ_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "overlay",
        magics: &["OVERLAYFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "pidfs",
        magics: &["PID_FS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "pipefs",
        magics: &["PIPEFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "ppc-cmm",
        magics: &["PPC_CMM_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "proc",
        magics: &["PROC_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "pstore",
        magics: &["PSTOREFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "pvfs2",
        magics: &["ORANGEFS_DEVREQ_MAGIC"],
        obsolete: true,
    },
    Fs {
        name: "qnx4",
        magics: &["QNX4_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "qnx6",
        magics: &["QNX6_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "ramfs",
        magics: &["RAMFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "resctrl",
        magics: &["RDTGROUP_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "reiserfs",
        magics: &["REISERFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "rpc_pipefs",
        magics: &["RPC_PIPEFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "secretmem",
        magics: &["SECRETMEM_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "securityfs",
        magics: &["SECURITYFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "selinuxfs",
        magics: &["SELINUX_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "shiftfs",
        magics: &["SHIFTFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "smackfs",
        magics: &["SMACK_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "smb3",
        magics: &["CIFS_SUPER_MAGIC"],
        obsolete: true,
    },
    Fs {
        name: "smbfs",
        magics: &["SMB_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "sockfs",
        magics: &["SOCKFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "squashfs",
        magics: &["SQUASHFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "sysfs",
        magics: &["SYSFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "tmpfs",
        magics: &["TMPFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "tracefs",
        magics: &["TRACEFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "udf",
        magics: &["UDF_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "usbdevfs",
        magics: &["USBDEVICE_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "vboxsf",
        magics: &["VBOXSF_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "vfat",
        magics: &["MSDOS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "v9fs",
        magics: &["V9FS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "xenfs",
        magics: &["XENFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "xfs",
        magics: &["XFS_SUPER_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "z3fold",
        magics: &["Z3FOLD_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "zonefs",
        magics: &["ZONEFS_MAGIC"],
        obsolete: false,
    },
    Fs {
        name: "zsmalloc",
        magics: &["ZSMALLOC_MAGIC"],
        obsolete: false,
    },
];

const GROUPS: &[Group] = &[
    Group {
        name: "@basic-api",
        help: "Basic filesystem API",
        members: &[
            "cgroup", "cgroup2", "devpts", "devtmpfs", "mqueue", "proc", "sysfs",
        ],
    },
    Group {
        name: "@anonymous",
        help: "Anonymous inodes",
        members: &["anon_inodefs", "pipefs", "sockfs"],
    },
    Group {
        name: "@application",
        help: "Application virtual filesystems",
        members: &["autofs", "fuse", "overlay"],
    },
    Group {
        name: "@auxiliary-api",
        help: "Auxiliary filesystem API",
        members: &[
            "binfmt_misc",
            "configfs",
            "efivarfs",
            "fusectl",
            "hugetlbfs",
            "rpc_pipefs",
            "securityfs",
        ],
    },
    Group {
        name: "@common-block",
        help: "Common block device filesystems",
        members: &[
            "btrfs", "erofs", "exfat", "ext4", "f2fs", "iso9660", "ntfs3", "squashfs", "udf",
            "vfat", "xfs",
        ],
    },
    Group {
        name: "@historical-block",
        help: "Historical block device filesystems",
        members: &["ext2", "ext3", "minix"],
    },
    Group {
        name: "@network",
        help: "Well-known network filesystems",
        members: &[
            "afs", "ceph", "cifs", "gfs", "gfs2", "ncp", "ncpfs", "nfs", "nfs4", "ocfs2",
            "orangefs", "pvfs2", "smb3", "smbfs",
        ],
    },
    Group {
        name: "@privileged-api",
        help: "Privileged filesystem API",
        members: &["bpf", "debugfs", "pstore", "tracefs"],
    },
    Group {
        name: "@security",
        help: "Security/MAC API VFS",
        members: &["apparmorfs", "selinuxfs", "smackfs"],
    },
    Group {
        name: "@temporary",
        help: "Temporary filesystems",
        members: &["ramfs", "tmpfs"],
    },
    Group {
        name: "@known",
        help: "All known filesystems declared in the kernel",
        members: &[],
    },
];

const _: () = assert!(FILESYSTEMS.len() == 99);
const _: () = assert!(GROUPS.len() == 11);

fn magics_of(name: &str) -> &'static [&'static str] {
    for fs in FILESYSTEMS {
        if fs.name == name {
            return fs.magics;
        }
    }
    eprintln!("filesystem-sets: unknown member {name}");
    std::process::exit(2);
}

fn members_of(group: &Group) -> Vec<&'static str> {
    if group.name == "@known" {
        FILESYSTEMS.iter().map(|fs| fs.name).collect()
    } else {
        group.members.to_vec()
    }
}

fn enum_of(gname: &str) -> String {
    let rest = gname.trim_start_matches('@');
    format!(
        "FILESYSTEM_SET_{}",
        rest.to_ascii_uppercase().replace('-', "_")
    )
}

fn sorted_magics(members: &[&str]) -> Vec<&'static str> {
    let mut set = BTreeSet::new();
    for name in members {
        for magic in magics_of(name) {
            set.insert(*magic);
        }
    }
    set.into_iter().collect()
}

fn name_id_chars_ok(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Declaration gperf requires when --struct-type is set.
/// DNS type and protocol name tables use this one-line form.
fn emit_gperf_name_id_struct(out: &mut dyn Write, name: &str) -> io::Result<()> {
    if !name_id_chars_ok(name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "gperf-name-id: name is not an identifier",
        ));
    }
    writeln!(out, "struct {name}_name {{ const char* name; int id; }};")
}

fn generate_gperf(out: &mut dyn Write) -> io::Result<()> {
    writeln!(out, "/* SPDX-License-Identifier: LGPL-2.1-or-later */")?;
    writeln!(out, "%{{")?;
    writeln!(out, "#if __GNUC__ >= 15")?;
    writeln!(
        out,
        "_Pragma(\"GCC diagnostic ignored \\\"-Wzero-as-null-pointer-constant\\\"\")"
    )?;
    writeln!(out, "#endif")?;
    writeln!(out, "#include <linux/magic.h>")?;
    writeln!(out)?;
    writeln!(out, "#include \"filesystems.h\"")?;
    writeln!(out, "#include \"stat-util.h\"")?;
    writeln!(out)?;
    writeln!(out, "struct FilesystemMagic {{")?;
    writeln!(out, "        const char *name;")?;
    writeln!(out, "        statfs_f_type_t magic[FILESYSTEM_MAGIC_MAX];")?;
    writeln!(out, "}};")?;
    writeln!(out, "%}}")?;
    writeln!(out, "struct FilesystemMagic;")?;
    writeln!(out, "%language=ANSI-C")?;
    writeln!(out, "%define hash-function-name filesystems_gperf_hash")?;
    writeln!(out, "%define lookup-function-name filesystems_gperf_lookup")?;
    writeln!(out, "%define slot-name name")?;
    writeln!(out, "%readonly-tables")?;
    writeln!(out, "%omit-struct-type")?;
    writeln!(out, "%struct-type")?;
    writeln!(out, "%includes")?;
    writeln!(out, "%%")?;
    for fs in FILESYSTEMS {
        let label = format!("{},", fs.name);
        let joined = fs.magics.join(", ");
        writeln!(out, "{label:<16} {{{joined}}}")?;
    }
    Ok(())
}

fn generate_fs_type_to_string(out: &mut dyn Write) -> io::Result<()> {
    writeln!(out, "#include <linux/magic.h>")?;
    writeln!(out, "#include \"filesystems.h\"")?;
    writeln!(out)?;
    writeln!(
        out,
        "/* PROJECT_FILE, which is used by log_xyz() thus also used by assert_not_reached(), cannot be used in"
    )?;
    writeln!(
        out,
        " * generated files, as the build directory may be outside of the source directory. */"
    )?;
    writeln!(out, "#undef PROJECT_FILE")?;
    writeln!(out, "#define PROJECT_FILE __FILE__")?;
    writeln!(out)?;
    writeln!(
        out,
        "const char* fs_type_to_string(statfs_f_type_t magic) {{"
    )?;
    writeln!(out, "        switch (magic) {{")?;
    for fs in FILESYSTEMS {
        if fs.obsolete {
            continue;
        }
        for magic in fs.magics {
            writeln!(out, "        case (statfs_f_type_t) {magic}:")?;
        }
        writeln!(out, "                return \"{}\";", fs.name)?;
    }
    writeln!(out, "        }}")?;
    writeln!(out, "        return NULL;")?;
    writeln!(out, "}}")?;
    Ok(())
}

fn generate_filesystem_sets(out: &mut dyn Write) -> io::Result<()> {
    writeln!(
        out,
        "const FilesystemSet filesystem_sets[_FILESYSTEM_SET_MAX] = {{"
    )?;
    for group in GROUPS {
        let enum_name = enum_of(group.name);
        let members = members_of(group);
        writeln!(out, "        [{enum_name}] = {{")?;
        writeln!(out, "                .name = \"{}\",", group.name)?;
        writeln!(out, "                .help = \"{}\",", group.help)?;
        writeln!(out, "                .value =")?;
        for filesystem in members {
            writeln!(out, "                \"{filesystem}\\0\"")?;
        }
        writeln!(out, "        }},")?;
    }
    writeln!(out, "}};")?;
    Ok(())
}

fn generate_fs_in_group(out: &mut dyn Write) -> io::Result<()> {
    writeln!(
        out,
        "bool fs_in_group(const struct statfs *st, FilesystemGroups fs_group) {{"
    )?;
    writeln!(out, "        switch (fs_group) {{")?;
    for group in GROUPS {
        let members = members_of(group);
        let magics = sorted_magics(&members);
        let enum_name = enum_of(group.name);
        writeln!(out, "        case {enum_name}:")?;
        for (i, magic) in magics.iter().enumerate() {
            if i == 0 {
                write!(
                    out,
                    "                return F_TYPE_EQUAL(st->f_type, {magic})"
                )?;
            } else {
                write!(
                    out,
                    "\n                    || F_TYPE_EQUAL(st->f_type, {magic})"
                )?;
            }
        }
        writeln!(out, ";")?;
    }
    writeln!(out, "        default: assert_not_reached();")?;
    writeln!(out, "        }}")?;
    writeln!(out, "}}")?;
    Ok(())
}

fn is_input_path(arg: &str) -> bool {
    arg.contains('/') || arg.ends_with(".py") || arg.ends_with(".sh")
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!(
            "usage: filesystem-sets gperf|gperf-name-id|filesystem-sets|fs-type-to-string|fs-in-group|check"
        );
        return ExitCode::from(2);
    }
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut saw_mode = false;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "gperf-name-id" {
            let Some(name) = args.get(i + 1) else {
                eprintln!("filesystem-sets: gperf-name-id needs a name");
                return ExitCode::from(2);
            };
            saw_mode = true;
            if let Err(err) = emit_gperf_name_id_struct(&mut out, name) {
                eprintln!("filesystem-sets: write failed: {err}");
                return ExitCode::from(1);
            }
            i += 2;
            continue;
        }
        if is_input_path(&args[i]) {
            i += 1;
            continue;
        }
        saw_mode = true;
        let mode = args[i].clone();
        let wrote = match mode.as_str() {
            "gperf" => generate_gperf(&mut out),
            "fs-type-to-string" => generate_fs_type_to_string(&mut out),
            "filesystem-sets" => generate_filesystem_sets(&mut out),
            "fs-in-group" => generate_fs_in_group(&mut out),
            "check" => Ok(()),
            _ => {
                eprintln!("filesystem-sets: unknown mode {mode}");
                return ExitCode::from(2);
            }
        };
        if let Err(err) = wrote {
            eprintln!("filesystem-sets: write failed: {err}");
            return ExitCode::from(1);
        }
        i += 1;
    }
    if !saw_mode {
        eprintln!(
            "usage: filesystem-sets gperf|gperf-name-id|filesystem-sets|fs-type-to-string|fs-in-group|check"
        );
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}
