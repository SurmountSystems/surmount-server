//! Discover DS1513 and DS3018xs on the laptop LAN. Never print IPv4.

use std::path::PathBuf;
use std::process::{Command, ExitCode};

use surmount_diskstation::strip_ipv4;

const USAGE: &str = "\
Usage:
  surmount-diskstation-discover [--help]

Find DS1513 (5-bay) and DS3018xs (6-bay) on the office LAN from the laptop.

Uses Avahi/Bonjour when present:
  avahi-browse _afpovertcp._tcp  (also _http._tcp and _smb._tcp)
and getent hosts for DS1513.local and DS3018xs.local.

Does not scan the office with nmap. Does not print IPv4 addresses.
Does not need last-known LAN addresses. Does not look up AFP passwords.

Match is by model string DS1513 / DS3018xs in the advertised name, the
mDNS hostname, or TXT. Two units both named DiskStation.local cannot be
told apart until you set Control Panel network name on each NAS.

If DSM still uses the generic DiskStation name:
  DSM Control Panel > Network > General (or Info Center > Network)
  Set Server Name / device name to DS1513 on the 5-bay and DS3018xs on
  the 6-bay so mDNS becomes DS1513.local and DS3018xs.local.

MailPlus share listing needs AFP auth. This command does not send a
password, so it will not claim the share is present.

Environment (tests):
  SURMOUNT_AVAHI_BROWSE   avahi-browse binary
  SURMOUNT_GETENT         getent binary
";

const RENAME: &str = "\
Two units both using a generic DiskStation.local name cannot be told apart.
Set each NAS Control Panel network name so mDNS is stable:
  DSM Control Panel > Network > General (or Info Center > Network)
  5-bay: Server Name / device name = DS1513
  6-bay: Server Name / device name = DS3018xs
Then mDNS should be DS1513.local and DS3018xs.local. Re-run:
  just diskstation-discover
";

#[derive(Default)]
struct State {
    host_1513: bool,
    mdns_1513: String,
    match_1513: String,
    svc_1513: String,
    host_3018: bool,
    mdns_3018: String,
    match_3018: String,
    svc_3018: String,
    generic: bool,
}

fn classify(blob: &str) -> Option<&'static str> {
    if blob.contains("DS3018xs") {
        Some("DS3018xs")
    } else if blob.contains("DS1513") {
        Some("DS1513")
    } else {
        None
    }
}

fn note(st: &mut State, id: &str, mdns: &str, how: &str, svc: &str) {
    if mdns.is_empty() {
        return;
    }
    match id {
        "DS1513" => {
            st.host_1513 = true;
            if st.mdns_1513.is_empty() || mdns == "DS1513.local" {
                st.mdns_1513 = mdns.to_string();
            }
            if st.match_1513.is_empty() {
                st.match_1513 = how.to_string();
            } else if !st.match_1513.contains(how) {
                st.match_1513 = format!("{},{how}", st.match_1513);
            }
            if !svc.is_empty() {
                st.svc_1513 = svc.to_string();
            }
        }
        "DS3018xs" => {
            st.host_3018 = true;
            if st.mdns_3018.is_empty() || mdns == "DS3018xs.local" {
                st.mdns_3018 = mdns.to_string();
            }
            if st.match_3018.is_empty() {
                st.match_3018 = how.to_string();
            } else if !st.match_3018.contains(how) {
                st.match_3018 = format!("{},{how}", st.match_3018);
            }
            if !svc.is_empty() {
                st.svc_3018 = svc.to_string();
            }
        }
        _ => {}
    }
}

fn ingest(st: &mut State, name: &str, svc: &str, hostname: &str, extra: &str) {
    let blob = format!("{name} {hostname} {extra}");
    let mut mdns = hostname.to_string();
    if mdns.is_empty() && !name.is_empty() {
        mdns = name.to_string();
    }
    if !mdns.is_empty() && !mdns.ends_with(".local") {
        mdns.push_str(".local");
    }
    if let Some(id) = classify(&blob) {
        let how = if extra.contains("DS1513") || extra.contains("DS3018xs") {
            "avahi-txt"
        } else if hostname == "DS1513.local" || hostname == "DS3018xs.local" {
            "avahi-hostname"
        } else if name.contains("DS1513") || name.contains("DS3018xs") {
            "avahi-name"
        } else {
            "avahi"
        };
        note(st, id, &mdns, how, svc);
        return;
    }
    if !mdns.is_empty() {
        println!(
            "{}",
            strip_ipv4(&format!(
                "unmatched mdns={mdns} service={}",
                if svc.is_empty() { "unknown" } else { svc }
            ))
        );
        if mdns.eq_ignore_ascii_case("DiskStation.local") {
            st.generic = true;
        }
    }
}

fn parse_avahi(st: &mut State, text: &str) {
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("=;") {
            let parts: Vec<&str> = rest.split(';').collect();
            // iface;proto;name;type;domain;hostname;address;port;txt
            let name = *parts.get(2).unwrap_or(&"");
            let typ = *parts.get(3).unwrap_or(&"");
            let hostname = *parts.get(5).unwrap_or(&"");
            let txt = *parts.get(8).unwrap_or(&"");
            ingest(st, name, typ, hostname, txt);
        } else if line.contains("hostname = [")
            && let Some(start) = line.find('[')
            && let Some(end) = line[start + 1..].find(']')
        {
            let h = &line[start + 1..start + 1 + end];
            ingest(st, "", "_afpovertcp._tcp", h, line);
        }
    }
}

fn tool(env_name: &str, default: &str) -> Option<PathBuf> {
    if let Ok(p) = std::env::var(env_name) {
        return Some(PathBuf::from(p));
    }
    surmount_diskstation::which(default)
}

fn main() -> ExitCode {
    let mut args = std::env::args();
    let _ = args.next();
    if let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            other if other.starts_with('-') => {
                eprintln!("diskstation-discover: unknown option: {other}");
                return ExitCode::from(1);
            }
            other => {
                eprintln!("diskstation-discover: unexpected argument: {other}");
                return ExitCode::from(1);
            }
        }
    }
    let mut st = State::default();
    let avahi = tool("SURMOUNT_AVAHI_BROWSE", "avahi-browse");
    if let Some(av) = &avahi {
        for svc in ["_afpovertcp._tcp", "_http._tcp", "_smb._tcp"] {
            if let Ok(out) = Command::new(av).args(["-prt", svc]).output() {
                parse_avahi(
                    &mut st,
                    &format!(
                        "{}{}",
                        String::from_utf8_lossy(&out.stdout),
                        String::from_utf8_lossy(&out.stderr)
                    ),
                );
            }
        }
    } else {
        eprintln!(
            "diskstation-discover: avahi-browse not on PATH; trying getent hosts for DS1513.local and DS3018xs.local"
        );
    }
    if let Some(ge) = tool("SURMOUNT_GETENT", "getent") {
        for id in ["DS1513", "DS3018xs"] {
            if let Ok(out) = Command::new(&ge)
                .args(["hosts", &format!("{id}.local")])
                .output()
                && out.status.success()
            {
                let text = String::from_utf8_lossy(&out.stdout);
                let name = text
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or(&format!("{id}.local"))
                    .to_string();
                note(&mut st, id, &name, "getent", "");
            }
        }
    }
    let mut body = String::new();
    if st.host_1513 {
        body.push_str(&format!(
            "host_id=DS1513 mdns={} match={} service={}\n",
            st.mdns_1513,
            st.match_1513,
            if st.svc_1513.is_empty() {
                "unknown"
            } else {
                &st.svc_1513
            }
        ));
    }
    if st.host_3018 {
        body.push_str(&format!(
            "host_id=DS3018xs mdns={} match={} service={}\n",
            st.mdns_3018,
            st.match_3018,
            if st.svc_3018.is_empty() {
                "unknown"
            } else {
                &st.svc_3018
            }
        ));
    }
    body.push_str("mailplus=unknown (share listing needs AFP auth; not attempted)\n");
    if st.generic && (!st.host_1513 || !st.host_3018) {
        body.push_str(RENAME);
    } else if !st.host_1513 && !st.host_3018 {
        body.push_str("No DS1513 or DS3018xs match from avahi or getent.\n");
        body.push_str(RENAME);
    }
    print!("{}", strip_ipv4(&body));
    if !st.host_1513 && !st.host_3018 && avahi.is_none() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
