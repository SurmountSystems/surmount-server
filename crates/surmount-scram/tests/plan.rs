//! Hermetic scram plans. Fake proc tree. Never signals live PIDs.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use surmount_scram::{
    AVAIL_FLOOR_KIB, Cfg, Plan, Proc, is_hog, is_protected, parse_meminfo, parse_status,
    plan_scram, read_snapshot,
};

fn proc(pid: i32, uid: u32, rss_kib: u64, comm: &str) -> Proc {
    Proc {
        pid,
        uid,
        rss_kib,
        comm: comm.to_string(),
    }
}

#[test]
fn meminfo_reads_available() {
    let t = "MemTotal:       99999999 kB\nMemAvailable:    20000000 kB\n";
    assert_eq!(parse_meminfo(t), Some(20_000_000));
}

#[test]
fn status_reads_name_uid_rss() {
    let t = "Name:\tlean\nUid:\t1002\t1002\t1002\t1002\nVmRSS:\t    4096 kB\n";
    let p = parse_status(t, 9).unwrap();
    assert_eq!(p.comm, "lean");
    assert_eq!(p.uid, 1002);
    assert_eq!(p.rss_kib, 4096);
}

#[test]
fn protected_names_are_not_hogs() {
    let cfg = Cfg {
        avail_floor_kib: AVAIL_FLOOR_KIB,
        nixbuilder_uid: Some(1002),
    };
    let mail = proc(1, 0, 99_000_000, "stalwart-mail");
    let ssh = proc(2, 0, 99_000_000, "sshd");
    let ui = proc(3, 0, 99_000_000, "surmount-management-ui");
    let et = proc(4, 0, 99_000_000, "etserver");
    let selfp = proc(5, 0, 99_000_000, "surmount-scram");
    for p in [&mail, &ssh, &ui, &et, &selfp] {
        assert!(is_protected(p), "{}", p.comm);
        assert!(!is_hog(p, &cfg), "{}", p.comm);
    }
}

#[test]
fn idle_when_enough_ram() {
    let cfg = Cfg {
        avail_floor_kib: AVAIL_FLOOR_KIB,
        nixbuilder_uid: Some(1002),
    };
    let snap = surmount_scram::Snapshot {
        mem_available_kib: AVAIL_FLOOR_KIB + 1,
        procs: vec![proc(9, 1002, 80_000_000, "lean")],
    };
    assert_eq!(plan_scram(&snap, &cfg), Plan::default());
}

#[test]
fn pressure_kills_largest_hog_not_mail() {
    let cfg = Cfg {
        avail_floor_kib: AVAIL_FLOOR_KIB,
        nixbuilder_uid: Some(1002),
    };
    let snap = surmount_scram::Snapshot {
        mem_available_kib: AVAIL_FLOOR_KIB - 1,
        procs: vec![
            proc(10, 0, 50_000_000, "stalwart-mail"),
            proc(11, 1002, 10_000, "lean"),
            proc(12, 1002, 90_000_000, "lean"),
            proc(13, 30001, 5_000, "rustc"),
        ],
    };
    let plan = plan_scram(&snap, &cfg);
    assert_eq!(plan.pids, vec![12, 11, 13]);
    assert_eq!(plan.slice.as_deref(), Some("user-1002.slice"));
    assert!(!plan.pids.contains(&10));
}

#[test]
fn empty_hog_list_still_requests_slice() {
    let cfg = Cfg {
        avail_floor_kib: AVAIL_FLOOR_KIB,
        nixbuilder_uid: Some(1002),
    };
    let snap = surmount_scram::Snapshot {
        mem_available_kib: 100,
        procs: vec![proc(1, 0, 1, "sshd")],
    };
    let plan = plan_scram(&snap, &cfg);
    assert!(plan.pids.is_empty());
    assert_eq!(plan.slice.as_deref(), Some("user-1002.slice"));
}

fn scratch() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "surmount-scram-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn fake_proc_tree_reads() {
    let root = scratch();
    fs::write(root.join("meminfo"), "MemAvailable:        1000 kB\n").unwrap();
    let d = root.join("4242");
    fs::create_dir_all(&d).unwrap();
    fs::write(
        d.join("status"),
        "Name:\tlake\nUid:\t1002\t1002\nVmRSS:\t  2048 kB\n",
    )
    .unwrap();
    let cfg = Cfg::default();
    let snap = read_snapshot(&root, &cfg).unwrap();
    assert_eq!(snap.mem_available_kib, 1000);
    assert_eq!(snap.procs.len(), 1);
    assert_eq!(snap.procs[0].comm, "lake");
    assert_eq!(snap.procs[0].pid, 4242);
}
