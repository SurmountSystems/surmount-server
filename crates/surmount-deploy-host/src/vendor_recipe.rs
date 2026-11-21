//! `just vendor` as process commands.
//!
//! The justfile line stays `nix run .#vendor`. This module is the old
//! recipe: a local chroot store under `./vendor/nix-store`, locked flake
//! inputs, then fixed-output outputs only. It does not run a shell script,
//! and it does not build the mail host or run deploy.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::ToolError;

const NOTE: &str = "Ordinary source directories on the cwd disk. vendor/nix-store is a local chroot store whose files are under that directory.";
const FOD_NOTE: &str = "Fixed-output outputs exported without building a non-fixed-output derivation. If fodEvalErrors or fodSkipped is non-empty, this is not the full source set of packages and the mail host.";
const MAIL_TOPLEVEL: &str = ".#nixosConfigurations.mail-vps.config.system.build.toplevel";
const EMPTY_MANIFEST: &str = r#"{"inputs":[],"sources":[],"fodEvalErrors":[],"fodSkipped":[]}"#;

const LOCK_WALK: &str = r#"
def resolve_node($nodes; $path):
  reduce $path[] as $n (
    "root";
    . as $cur
    | ($nodes[$cur].inputs[$n] // error("missing \($n) under \($cur)")) as $next
    | if ($next|type) == "string" then $next
      elif ($next|type) == "array" then resolve_node($nodes; $next)
      else error("bad lock input")
      end
  );
def walk($nodes; $node; $prefix):
  ($nodes[$node].inputs // {}) | to_entries[]
  | .key as $name
  | .value as $raw
  | (if ($raw|type) == "string" then $raw
     elif ($raw|type) == "array" then resolve_node($nodes; $raw)
     else error("bad lock input")
     end) as $target
  | ($prefix + [$name]) as $p
  | ($nodes[$target]) as $t
  | {
      rel: ($p | join("/")),
      node: $target,
      lockedType: ($t.locked.type // ""),
      rev: ($t.locked.rev // ""),
      narHash: ($t.locked.narHash // ""),
      originalUrl: (
        if ($t.original.type // $t.locked.type) == "github" then
          "github:"
          + ($t.original.owner // $t.locked.owner // "")
          + "/"
          + ($t.original.repo // $t.locked.repo // "")
          + (if ($t.original.ref // "") != "" then "/" + $t.original.ref
             elif ($t.original.rev // "") != "" then "/" + $t.original.rev
             else "" end)
        elif ($t.original.url // $t.locked.url // "") != "" then
          ($t.original.url // $t.locked.url)
        else
          ($t.original.type // $t.locked.type // "")
        end
      )
    },
    walk($nodes; $target; $p);
.nodes as $nodes | walk($nodes; "root"; [])
"#;

const ARCHIVE_BY_REL: &str = r#"
def walk($prefix):
  (.inputs // {}) | to_entries[]
  | .key as $k
  | .value as $v
  | ($prefix + [$k]) as $p
  | { key: ($p | join("/")), value: ($v.path // "") },
    ($v | walk($p));
[ walk([]) ] | from_entries
"#;

const DRV_EXTRACT: &str = r#"
def as_store:
  if type != "string" or . == "" then empty
  elif startswith("/nix/store/") then .
  elif startswith("/") then .
  else "/nix/store/" + .
  end;
def drv_entries:
  if (type == "object")
    and ((.version | type) == "number")
    and ((.derivations | type) == "object")
  then .derivations | to_entries[]
  elif type == "object" then to_entries[]
  else error("derivation show is not an object")
  end;
drv_entries
| select(.value | type == "object")
| select((.value.outputs | type) == "object")
| .key as $raw
| .value as $d
| ($d.outputs) as $outs
| {
    drv: ($raw | as_store),
    isFod: (
      (([ $outs | to_entries[]
          | select(.value | type == "object")
          | (.value.hash // "")
          | select(type == "string" and . != "")
        ] | length) > 0)
      or (
        (($d.env.outputHash? // "") | type) == "string"
        and (($d.env.outputHash? // "") != "")
      )
    ),
    outPaths: [
      $outs | to_entries[]
      | select(.value | type == "object")
      | (.value.path // "")
      | select(type == "string" and . != "")
      | as_store
    ],
    inputDrvs: (
      if ($d.inputDrvs | type) == "object" then
        [ $d.inputDrvs | keys[] | as_store ]
      elif ($d.inputDrvs | type) == "array" then
        [ $d.inputDrvs[] | strings | as_store ]
      elif (($d.inputs? | type) == "object")
        and (($d.inputs.drvs | type) == "object") then
        [ $d.inputs.drvs | keys[] | as_store ]
      else [] end
    ),
    name: ($d.env.name? // $d.name? // "unknown"),
    builder: ($d.builder? // $d.env.builder? // ""),
    urls: ($d.env.urls? // $d.env.url? // ""),
    outputHash: (
      [ $outs | to_entries[]
        | select(.value | type == "object")
        | (.value.hash // "")
        | select(type == "string" and . != "")
      ][0]
      // ($d.env.outputHash? // "")
    ),
    outPath: (
      ($d.env.out? // "") as $envout
      | if ($envout | type) == "string" and ($envout | startswith("/nix/store/")) then
          $envout
        else
          (($outs.out? // {}) | if type == "object" then (.path // "") else "" end)
          | if type == "string" and . != "" then as_store else "" end
        end
    )
  }
| select(.drv != "")
"#;

const FOD_SELECT: &str = r#"
(map({key: .drv, value: .}) | from_entries) as $m
| def go($id; $seen):
    if $seen[$id] then empty
    else
      ($m[$id] // null) as $dep
      | if $dep == null then "__MISSING_DEP__"
        elif $dep.isFod then
            ($dep.inputDrvs // [])[] | go(.; $seen + {($id): true})
        else ($dep.outPaths // [])[]
        end
    end;
[ .[] | select(.isFod) | {
  drv,
  name,
  builder,
  urls,
  outputHash,
  outPath,
  need: ([ (.inputDrvs // [])[] | go(.; {}) ] | unique)
} ]
"#;

const DRY_DRVS: &str = r#"
def rows:
  if type == "array" then .[]
  elif type == "object" then (to_entries[] | .value)
  else empty end;
[ rows | .drvPath // .drv // empty ] | .[]
"#;

const NAR_LOOKUP: &str = r#"
def rows:
  if type == "array" then .[]
  elif type == "object" then
    to_entries[]
    | { path: .key }
      + (if (.value | type) == "object" then .value else {} end)
  else empty end;
[ rows | select(.narHash == $nar) | .path ] | map(select(. != null and . != "")) | first // empty
"#;

struct Ctx {
    root: PathBuf,
    vendor: PathBuf,
    store: PathBuf,
    inputs: PathBuf,
    sources: PathBuf,
    work: PathBuf,
    half: String,
}

struct Prev {
    path: PathBuf,
    stub_no_hash: bool,
}

struct Plan {
    need: HashMap<String, bool>,
    any_need: bool,
}

pub fn run() -> Result<(), ToolError> {
    ensure_niced()?;
    let ctx = prepare()?;
    let stamp = source_stamp(&ctx.root)?;
    let prev = load_prev(&ctx)?;
    write_lock_inputs(&ctx)?;
    let rels = jq_lines(&ctx.work.join("lock-inputs.json"), ".[].rel", &[])?;
    let plan = plan_needs(&ctx, &prev, &rels)?;
    if plan.any_need {
        fetch_archive(&ctx)?;
    } else {
        write_text(&ctx.work.join("archive-by-rel.json"), "{}\n")?;
        write_text(&ctx.work.join("path-info.json"), "[]\n")?;
    }
    let input_count = export_inputs(&ctx, &prev, &plan)?;
    eprintln!("just vendor: {input_count} flake inputs are real directories under vendor/inputs.");
    if try_early_exit(&ctx, &prev, &plan, &stamp)? {
        return Ok(());
    }
    realise_fixed_outputs(&ctx, &prev, &stamp, input_count)
}

fn ensure_niced() -> Result<(), ToolError> {
    if std::env::var("VENDOR_NICED").ok().as_deref() == Some("1") {
        return Ok(());
    }
    let argv0 = std::env::args().next().unwrap_or_default();
    if !Path::new(&argv0).is_file() {
        let pid = std::process::id().to_string();
        let _ = Command::new("renice").args(["-n", "19", &pid]).status();
        let _ = Command::new("ionice")
            .args(["-c", "3", "-p", &pid])
            .status();
        return Ok(());
    }
    let ionice_ok = Command::new("ionice")
        .args(["-c", "3", "--", "/bin/true"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let mut cmd = if ionice_ok {
        let mut c = Command::new("ionice");
        c.args(["-c", "3", "--", "nice", "-n", "19"]);
        c
    } else {
        let mut c = Command::new("nice");
        c.args(["-n", "19"]);
        c
    };
    cmd.arg(&argv0);
    cmd.args(std::env::args().skip(1));
    cmd.env("VENDOR_NICED", "1");
    let status = cmd.status()?;
    std::process::exit(status.code().unwrap_or(1));
}

fn prepare() -> Result<Ctx, ToolError> {
    let root = std::env::current_dir()?;
    if !root.join("flake.nix").is_file() || !root.join("flake.lock").is_file() {
        return Err(fail(
            2,
            "just vendor: run from the repo root (flake.nix and flake.lock).",
        ));
    }
    for tool in ["nix", "jq", "tar"] {
        if !command_on_path(tool) {
            return Err(fail(2, format!("just vendor: {tool} is not on PATH.")));
        }
    }
    let vendor = root.join("vendor");
    let store = vendor.join("nix-store");
    let inputs = vendor.join("inputs");
    let sources = vendor.join("sources");
    let work = vendor.join("tmp");
    if !path_under(&store, &vendor) {
        return Err(fail(
            2,
            "just vendor: refusing store path outside ./vendor.",
        ));
    }
    fs::create_dir_all(&inputs)?;
    fs::create_dir_all(&sources)?;
    fs::create_dir_all(&work)?;
    fs::create_dir_all(&store)?;
    Ok(Ctx {
        root,
        vendor,
        store,
        inputs,
        sources,
        work,
        half: half_jobs()?,
    })
}

fn half_jobs() -> Result<String, ToolError> {
    let out = Command::new("nproc").output()?;
    if !out.status.success() {
        return Err(fail(1, "just vendor: nproc failed"));
    }
    let cores: usize = String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(1);
    Ok((cores / 2).max(1).to_string())
}

impl Ctx {
    fn tool(&self, bin: &str) -> Command {
        let mut cmd = Command::new(bin);
        cmd.current_dir(&self.root);
        cmd.env("TMPDIR", &self.work);
        cmd.env("TEMP", &self.work);
        cmd.env("TMP", &self.work);
        cmd
    }

    fn nix(&self) -> Command {
        let mut cmd = self.tool("nix");
        cmd.arg("--store").arg(&self.store);
        cmd.args(["--option", "builders", ""]);
        cmd.arg("--option").arg("max-jobs").arg(&self.half);
        cmd.args(["--option", "warn-dirty", "false"]);
        cmd.args(["--option", "allow-import-from-derivation", "false"]);
        cmd
    }

    fn record_error(
        &self,
        label: &str,
        errfile: &Path,
        message: Option<&str>,
    ) -> Result<(), ToolError> {
        let message = message.unwrap_or("evaluation failed; this output was not built");
        let line = jq_nc(
            &[("output", label), ("message", message)],
            "{output:$output,message:$message}",
        )?;
        append_line(&self.work.join("fod-errors.ndjson"), &line)?;
        eprintln!(
            "just vendor: eval failed for {label} (recorded, not built). Detail: {}",
            errfile.display()
        );
        Ok(())
    }

    fn append_skip(&self, name: &str, drv: &str, reason: &str) -> Result<(), ToolError> {
        let line = jq_nc(
            &[("name", name), ("drv", drv), ("reason", reason)],
            "{name:$name,drv:$drv,reason:$reason}",
        )?;
        append_line(&self.work.join("fod-skipped.ndjson"), &line)
    }
}

fn write_lock_inputs(ctx: &Ctx) -> Result<(), ToolError> {
    let err1 = ctx.work.join("lock-walk.err");
    let err2 = ctx.work.join("lock-sort.err");
    let out = ctx.work.join("lock-inputs.json");
    let mut first = ctx.tool("jq");
    first.arg("-c");
    first.arg(LOCK_WALK);
    first.arg(ctx.root.join("flake.lock"));
    first.stdout(Stdio::piped());
    first.stderr(Stdio::from(File::create(&err1)?));
    let mut child = first.spawn()?;
    let piped = child
        .stdout
        .take()
        .ok_or_else(|| fail(1, "just vendor: jq pipe"))?;
    let mut second = ctx.tool("jq");
    second.args(["-s", r#"sort_by(.rel | split("/") | length)"#]);
    second.stdin(Stdio::from(piped));
    second.stdout(Stdio::from(File::create(&out)?));
    second.stderr(Stdio::from(File::create(&err2)?));
    let second_status = second.status()?;
    let first_status = child.wait()?;
    if !first_status.success() || !second_status.success() {
        return Err(fail(1, "just vendor: could not read flake.lock inputs."));
    }
    Ok(())
}

fn load_prev(ctx: &Ctx) -> Result<Prev, ToolError> {
    let manifest = ctx.vendor.join("manifest.json");
    let empty = ctx.work.join("empty-manifest.json");
    let use_empty = |warn: bool| -> Result<Prev, ToolError> {
        if warn {
            eprintln!(
                "just vendor: existing manifest.json is not JSON. Source trees are left in place."
            );
        }
        write_text(&empty, &format!("{EMPTY_MANIFEST}\n"))?;
        Ok(Prev {
            path: empty,
            stub_no_hash: true,
        })
    };
    let real_file = fs::symlink_metadata(&manifest)
        .map(|m| m.is_file())
        .unwrap_or(false);
    if !real_file {
        return use_empty(false);
    }
    if !jq_status(&["-e", ".", &p(&manifest)])? {
        return use_empty(true);
    }
    let has_hash = jq_status(&[
        "-e",
        r#"[.inputs[]? | .narHash // empty | select(. != "")] | length > 0"#,
        &p(&manifest),
    ])?;
    Ok(Prev {
        path: manifest,
        stub_no_hash: !has_hash,
    })
}

fn plan_needs(ctx: &Ctx, prev: &Prev, rels: &[String]) -> Result<Plan, ToolError> {
    let lock = ctx.work.join("lock-inputs.json");
    let mut need = HashMap::new();
    for rel in rels {
        let nar = jq_r(
            &lock,
            r#".[] | select(.rel==$rel) | .narHash"#,
            &[("rel", rel.as_str())],
        )?;
        let have = jq_r(
            &prev.path,
            r#"[.inputs[]? | select(.name==$rel) | .narHash // empty][0] // """#,
            &[("rel", rel.as_str())],
        )
        .unwrap_or_default();
        let dest = ctx.inputs.join(rel);
        let real = is_real_dir(&dest);
        let fetch = if real && prev.stub_no_hash {
            false
        } else if real && have == nar && !nar.is_empty() {
            false
        } else {
            true
        };
        need.insert(rel.clone(), fetch);
    }
    for rel in rels {
        if need.get(rel) == Some(&true) {
            let prefix = format!("{rel}/");
            for child in rels {
                if child.starts_with(&prefix) {
                    need.insert(child.clone(), true);
                }
            }
        }
    }
    let any_need = need.values().any(|v| *v);
    Ok(Plan { need, any_need })
}

fn fetch_archive(ctx: &Ctx) -> Result<(), ToolError> {
    eprintln!(
        "just vendor: fetching locked flake inputs into {}",
        ctx.store.display()
    );
    let json = ctx.work.join("flake-archive.json");
    let err = ctx.work.join("flake-archive.err");
    let mut cmd = ctx.nix();
    cmd.args([
        "flake",
        "archive",
        "--json",
        "--no-write-lock-file",
        "--no-update-lock-file",
        "--print-build-logs",
        ".",
    ]);
    cmd.stdout(Stdio::from(File::create(&json)?));
    cmd.stderr(Stdio::from(File::create(&err)?));
    let status = cmd.status()?;
    if !status.success() {
        let tail = tail_lines(&err, 40);
        if !tail.is_empty() {
            eprintln!("{tail}");
        }
        return Err(fail(1, "just vendor: flake archive failed."));
    }
    let by_rel = ctx.work.join("archive-by-rel.json");
    if jq_status(&["-e", ".", &p(&json)])? {
        jq_to_file(&["-c", ARCHIVE_BY_REL, &p(&json)], &by_rel)?;
    } else {
        eprintln!(
            "just vendor: flake archive did not print JSON. Matching store objects by narHash."
        );
        write_text(&by_rel, "{}\n")?;
    }
    let info = ctx.work.join("path-info.json");
    let info_err = ctx.work.join("path-info.err");
    let mut info_cmd = ctx.nix();
    info_cmd.args(["path-info", "--json", "--all"]);
    info_cmd.stdout(Stdio::from(File::create(&info)?));
    info_cmd.stderr(Stdio::from(File::create(&info_err)?));
    let info_status = info_cmd.status()?;
    if !info_status.success() {
        write_text(&info, "[]\n")?;
    }
    Ok(())
}

fn export_inputs(ctx: &Ctx, prev: &Prev, plan: &Plan) -> Result<usize, ToolError> {
    let rows = jq_lines(&ctx.work.join("lock-inputs.json"), ".[]", &[])?;
    let ndjson = ctx.work.join("inputs.ndjson");
    write_text(&ndjson, "")?;
    let mut input_fail = false;
    for row in &rows {
        let rel = jq_stdin_r(".rel", row)?;
        let nar = jq_stdin_r(".narHash", row)?;
        let locked_type = jq_stdin_r(".lockedType", row)?;
        let rev = jq_stdin_r(".rev", row)?;
        let original = redact_url(&jq_stdin_r(".originalUrl", row)?);
        let dest = ctx.inputs.join(&rel);
        if plan.need.get(&rel) == Some(&true) && !export_one_input(ctx, &rel, &nar, &dest)? {
            input_fail = true;
            continue;
        }
        if !is_real_dir(&dest) {
            eprintln!("just vendor: input {rel} is not a real directory.");
            input_fail = true;
            continue;
        }
        let vendor_path = format!("vendor/inputs/{rel}");
        let line = jq_nc(
            &[
                ("name", rel.as_str()),
                ("lockedType", locked_type.as_str()),
                ("rev", rev.as_str()),
                ("narHash", nar.as_str()),
                ("originalUrl", original.as_str()),
                ("vendorPath", vendor_path.as_str()),
            ],
            "{name:$name,lockedType:$lockedType,rev:$rev,narHash:$narHash,originalUrl:$originalUrl,vendorPath:$vendorPath}",
        )?;
        append_line(&ndjson, &line)?;
        let _ = prev;
    }
    if input_fail {
        return Err(fail(
            1,
            "just vendor: flake input export failed. Fixed-output export was not started.",
        ));
    }
    let count = jq_r(&ndjson, "length", &[])?;
    // `jq length` on a stream needs -s. Count lines of objects instead when
    // the file is ndjson. `jq -s length` matches the old recipe.
    let _ = count;
    jq_s_length(&ndjson)
}

fn export_one_input(ctx: &Ctx, rel: &str, nar: &str, dest: &Path) -> Result<bool, ToolError> {
    let mut logical = jq_r(
        &ctx.work.join("archive-by-rel.json"),
        r#".[$rel] // empty"#,
        &[("rel", rel)],
    )?;
    if logical.is_empty() || logical == "null" {
        logical = jq_r(
            &ctx.work.join("path-info.json"),
            NAR_LOOKUP,
            &[("nar", nar)],
        )?;
    }
    let phys = resolve_store_dir(ctx, &logical);
    let Some(phys) = phys else {
        eprintln!("just vendor: no store directory for input {rel}");
        return Ok(false);
    };
    if export_tree(&phys, dest, &ctx.inputs, &ctx.sources).is_err() {
        return Ok(false);
    }
    Ok(true)
}

fn resolve_store_dir(ctx: &Ctx, logical: &str) -> Option<PathBuf> {
    if logical.is_empty() {
        return None;
    }
    let phys = physical_of(&ctx.store, logical);
    if let Some(phys) = phys {
        if phys.is_dir() {
            return Some(phys);
        }
        if Path::new(logical).is_dir() {
            return Some(PathBuf::from(logical));
        }
        return None;
    }
    if Path::new(logical).is_dir() {
        return Some(PathBuf::from(logical));
    }
    None
}

fn try_early_exit(ctx: &Ctx, prev: &Prev, plan: &Plan, stamp: &str) -> Result<bool, ToolError> {
    if plan.any_need {
        return Ok(false);
    }
    let prev_stamp = jq_r(&prev.path, r#".stamp // """#, &[])?;
    let prev_err = jq_r(&prev.path, r#"(.fodEvalErrors // []) | length"#, &[])?;
    let prev_skip = jq_r(&prev.path, r#"(.fodSkipped // []) | length"#, &[])?;
    if prev_stamp != stamp || prev_err != "0" || prev_skip != "0" || !sources_ok(ctx, &prev.path)? {
        return Ok(false);
    }
    let newp = ctx.vendor.join("manifest.json.new");
    let mut cmd = ctx.tool("jq");
    cmd.args(["--arg", "stamp", stamp]);
    cmd.arg("--slurpfile")
        .arg("inputs")
        .arg(ctx.work.join("inputs.ndjson"));
    cmd.arg(r#".inputs = $inputs | .stamp = $stamp"#);
    cmd.arg(&prev.path);
    cmd.stdout(Stdio::from(File::create(&newp)?));
    let status = cmd.status()?;
    if !status.success() {
        return Err(fail(1, "just vendor: could not refresh manifest.json."));
    }
    fs::rename(&newp, ctx.vendor.join("manifest.json"))?;
    eprintln!("just vendor: inputs match the lock. Left existing directories in place.");
    Ok(true)
}

fn sources_ok(ctx: &Ctx, prev: &Path) -> Result<bool, ToolError> {
    let paths = jq_lines(prev, ".sources[]?.vendorPath", &[])?;
    for spath in paths {
        if spath.is_empty() {
            continue;
        }
        if !is_real_dir(&ctx.root.join(spath)) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn realise_fixed_outputs(
    ctx: &Ctx,
    prev: &Prev,
    stamp: &str,
    input_count: usize,
) -> Result<(), ToolError> {
    for name in [
        "sources.ndjson",
        "fod-errors.ndjson",
        "fod-skipped.ndjson",
        "drv-index.ndjson",
    ] {
        write_text(&ctx.work.join(name), "")?;
    }
    let sys = current_system(ctx)?;
    show_installables(ctx, MAIL_TOPLEVEL, &[MAIL_TOPLEVEL.to_string()])?;
    if sys.is_empty() {
        eprintln!("just vendor: package outputs were not evaluated because currentSystem failed.");
    } else {
        show_packages(ctx, &sys)?;
    }
    select_fods(ctx)?;
    let mut seen = HashSet::new();
    let mut allowed = HashSet::new();
    let index = ctx.work.join("drv-index.json");
    if let Ok(rows) = jq_lines(&index, r#".[] | select(.isFod)"#, &[]) {
        for row in rows {
            if let Ok(drv) = jq_stdin_r(".drv", &row) {
                if !drv.is_empty() {
                    allowed.insert(drv);
                }
            }
        }
    }
    let copy_list = ctx.work.join("to-copy.txt");
    let build_list = ctx.work.join("to-build.txt");
    let pending = ctx.work.join("pending-sources.ndjson");
    write_text(&copy_list, "")?;
    write_text(&build_list, "")?;
    write_text(&pending, "")?;
    let fods = jq_lines(&ctx.work.join("fods.json"), ".[]", &[])?;
    for fod in &fods {
        stage_fod(ctx, fod, &mut seen, &copy_list, &build_list, &pending)?;
    }
    copy_present_deps(ctx, &copy_list, &build_list, &pending)?;
    if file_nonempty(&build_list)? {
        eprintln!(
            "just vendor: realising fixed-output outputs that do not need a new non-fixed-output build."
        );
        realise_lines(ctx, &build_list, &allowed)?;
    }
    export_pending(ctx, &pending, &mut seen)?;
    keep_previous_sources(ctx, &prev.path, &mut seen)?;
    write_final_manifest(ctx, stamp)?;
    assert_real_vendor_dirs(ctx)?;
    let manifest = ctx.vendor.join("manifest.json");
    let src_count = jq_r(&manifest, ".sources | length", &[])?;
    let err_count = jq_r(&manifest, ".fodEvalErrors | length", &[])?;
    let skip_count = jq_r(&manifest, ".fodSkipped | length", &[])?;
    eprintln!("just vendor: {input_count} inputs, {src_count} fixed-output source directories.");
    eprintln!("just vendor: fod eval errors {err_count}, fod skipped {skip_count}.");
    if err_count != "0" || skip_count != "0" {
        eprintln!(
            "just vendor: fixed-output export is not the full source set. See vendor/manifest.json fodEvalErrors and fodSkipped."
        );
    }
    Ok(())
}

fn current_system(ctx: &Ctx) -> Result<String, ToolError> {
    let err = ctx.work.join("system.err");
    let mut cmd = ctx.tool("nix");
    cmd.args([
        "eval",
        "--impure",
        "--raw",
        "--expr",
        "builtins.currentSystem",
    ]);
    cmd.stderr(Stdio::from(File::create(&err)?));
    let out = cmd.output()?;
    if !out.status.success() {
        ctx.record_error("builtins.currentSystem", &err, None)?;
        return Ok(String::new());
    }
    Ok(trim_nl(&String::from_utf8_lossy(&out.stdout)))
}

fn show_packages(ctx: &Ctx, sys: &str) -> Result<(), ToolError> {
    let names_path = ctx.work.join("pkg-names.json");
    let err = ctx.work.join("pkg-names.err");
    let label = format!(".#packages.{sys}");
    let mut cmd = ctx.nix();
    cmd.args(["eval", "--json", &label, "--apply", "builtins.attrNames"]);
    cmd.stdout(Stdio::from(File::create(&names_path)?));
    cmd.stderr(Stdio::from(File::create(&err)?));
    let status = cmd.status()?;
    if !status.success() {
        ctx.record_error(&label, &err, None)?;
        return Ok(());
    }
    let names = jq_lines(&names_path, ".[]", &[])?;
    let installables: Vec<String> = names
        .iter()
        .map(|name| format!(".#packages.{sys}.{name}"))
        .collect();
    let json = ctx.work.join("show.json");
    let show_err = ctx.work.join("show.err");
    let mut show = ctx.nix();
    show.args([
        "derivation",
        "show",
        "--recursive",
        "--no-write-lock-file",
        "--no-update-lock-file",
    ]);
    show.args(&installables);
    show.stdout(Stdio::from(File::create(&json)?));
    show.stderr(Stdio::from(File::create(&show_err)?));
    let show_status = show.status()?;
    if !show_status.success() {
        eprintln!("just vendor: package set eval failed as one group. Trying each package.");
        let _ = fs::remove_file(&json);
        for inst in &installables {
            show_installables(ctx, inst, std::slice::from_ref(inst))?;
        }
        return Ok(());
    }
    if jq_status(&["-e", ".", &p(&json)])? {
        extract_drvs(ctx, &json, &label)?;
    } else {
        write_text(&show_err, "package derivation show did not print JSON\n")?;
        ctx.record_error(&label, &show_err, None)?;
        let _ = fs::remove_file(&json);
    }
    Ok(())
}

fn show_installables(ctx: &Ctx, label: &str, installables: &[String]) -> Result<(), ToolError> {
    if installables.is_empty() {
        return Ok(());
    }
    let json = ctx.work.join("show.json");
    let err = ctx.work.join("show.err");
    let mut cmd = ctx.nix();
    cmd.args([
        "derivation",
        "show",
        "--recursive",
        "--no-write-lock-file",
        "--no-update-lock-file",
    ]);
    cmd.args(installables);
    cmd.stdout(Stdio::from(File::create(&json)?));
    cmd.stderr(Stdio::from(File::create(&err)?));
    let status = cmd.status()?;
    if !status.success() {
        ctx.record_error(label, &err, None)?;
        let _ = fs::remove_file(&json);
        return Ok(());
    }
    if !jq_status(&["-e", ".", &p(&json)])? {
        write_text(&err, "derivation show did not print JSON\n")?;
        ctx.record_error(label, &err, None)?;
        let _ = fs::remove_file(&json);
        return Ok(());
    }
    extract_drvs(ctx, &json, label)
}

fn extract_drvs(ctx: &Ctx, json: &Path, label: &str) -> Result<(), ToolError> {
    let parsed = ctx.work.join("drv-parsed.ndjson");
    let _ = fs::remove_file(&parsed);
    let err = ctx.work.join("show.err");
    let ok = jq_to_file(&["-c", DRV_EXTRACT, &p(json)], &parsed).is_ok();
    if !ok {
        write_text(&err, "derivation show JSON could not be parsed\n")?;
        ctx.record_error(
            label,
            &err,
            Some("derivation show JSON could not be parsed; this output was not built"),
        )?;
        let _ = fs::remove_file(json);
        let _ = fs::remove_file(&parsed);
        return Ok(());
    }
    if file_nonempty(&parsed)? {
        let body = fs::read(&parsed)?;
        let mut out = OpenOptions::new()
            .create(true)
            .append(true)
            .open(ctx.work.join("drv-index.ndjson"))?;
        out.write_all(&body)?;
        if !body.ends_with(b"\n") {
            writeln!(out)?;
        }
    }
    let _ = fs::remove_file(json);
    let _ = fs::remove_file(&parsed);
    Ok(())
}

fn select_fods(ctx: &Ctx) -> Result<(), ToolError> {
    let ndjson = ctx.work.join("drv-index.ndjson");
    let index = ctx.work.join("drv-index.json");
    let fods = ctx.work.join("fods.json");
    if !file_nonempty(&ndjson)? {
        write_text(&index, "[]\n")?;
        write_text(&fods, "[]\n")?;
        return Ok(());
    }
    if jq_to_file(&["-s", "unique_by(.drv)", &p(&ndjson)], &index).is_err() {
        let err = ctx.work.join("show.err");
        write_text(&err, "derivation index could not be parsed\n")?;
        ctx.record_error(
            "derivation-index",
            &err,
            Some("derivation index could not be parsed; fixed-output outputs were not selected"),
        )?;
        write_text(&index, "[]\n")?;
        write_text(&fods, "[]\n")?;
        return Ok(());
    }
    if jq_to_file(&["-c", FOD_SELECT, &p(&index)], &fods).is_err() {
        let err = ctx.work.join("show.err");
        write_text(&err, "fixed-output selection could not be parsed\n")?;
        ctx.record_error(
            "fixed-output-selection",
            &err,
            Some(
                "fixed-output selection could not be parsed; fixed-output outputs were not selected",
            ),
        )?;
        write_text(&fods, "[]\n")?;
    }
    Ok(())
}

fn stage_fod(
    ctx: &Ctx,
    fod: &str,
    seen: &mut HashSet<String>,
    copy_list: &Path,
    build_list: &Path,
    pending: &Path,
) -> Result<(), ToolError> {
    let drv = jq_stdin_r(".drv", fod)?;
    let name = jq_stdin_r(".name", fod)?;
    let builder = jq_stdin_r(".builder", fod)?;
    let urls = redact_url(&jq_stdin_r(".urls", fod)?);
    let output_hash = jq_stdin_r(r#".outputHash // """#, fod)?;
    let out_path = jq_stdin_r(r#".outPath // """#, fod)?;
    if output_hash.is_empty() || output_hash == "null" {
        ctx.append_skip(&name, &drv, "no output hash")?;
        return Ok(());
    }
    let safe = safe_source_name(&output_hash, &name);
    let dest_rel = format!("vendor/sources/{safe}");
    let dest = ctx.root.join(&dest_rel);
    if is_real_dir(&dest) {
        seen.insert(output_hash.clone());
        let line = source_record(&name, &output_hash, &urls, &dest_rel)?;
        append_line(&ctx.work.join("sources.ndjson"), &line)?;
        return Ok(());
    }
    let mut missing = false;
    if !builder.starts_with("builtin:") {
        let needs = jq_stdin_lines(r#".need[]?"#, fod)?;
        for need in needs {
            if need.is_empty() {
                continue;
            }
            if need == "__MISSING_DEP__" {
                missing = true;
                continue;
            }
            let in_store = PathBuf::from(format!("{}{need}", ctx.store.display()));
            if in_store.exists() {
                continue;
            }
            if Path::new(&need).exists() {
                append_line(copy_list, &need)?;
            } else {
                missing = true;
            }
        }
    }
    if missing {
        ctx.append_skip(
            &name,
            &drv,
            "non-fixed-output dependency is not already present, so it was not built",
        )?;
        return Ok(());
    }
    append_line(build_list, &format!("{drv}^*"))?;
    let line = jq_nc(
        &[
            ("name", name.as_str()),
            ("lockedType", "fixed-output"),
            ("rev", ""),
            ("narHash", output_hash.as_str()),
            ("originalUrl", urls.as_str()),
            ("vendorPath", dest_rel.as_str()),
            ("outPath", out_path.as_str()),
            ("builder", builder.as_str()),
            ("drv", drv.as_str()),
        ],
        "{name:$name,lockedType:$lockedType,rev:$rev,narHash:$narHash,originalUrl:$originalUrl,vendorPath:$vendorPath,outPath:$outPath,builder:$builder,drv:$drv}",
    )?;
    append_line(pending, &line)
}

fn source_record(name: &str, hash: &str, urls: &str, dest_rel: &str) -> Result<String, ToolError> {
    jq_nc(
        &[
            ("name", name),
            ("lockedType", "fixed-output"),
            ("rev", ""),
            ("narHash", hash),
            ("originalUrl", urls),
            ("vendorPath", dest_rel),
        ],
        "{name:$name,lockedType:$lockedType,rev:$rev,narHash:$narHash,originalUrl:$originalUrl,vendorPath:$vendorPath}",
    )
}

fn copy_present_deps(
    ctx: &Ctx,
    copy_list: &Path,
    build_list: &Path,
    pending: &Path,
) -> Result<(), ToolError> {
    let mut paths = read_lines(copy_list)?;
    if paths.is_empty() {
        return Ok(());
    }
    paths.sort();
    paths.dedup();
    let sorted = ctx.work.join("to-copy-sorted.txt");
    write_lines(&sorted, &paths)?;
    let mut cmd = ctx.tool("nix");
    cmd.args(["copy", "--option", "builders", "", "--to"]);
    cmd.arg(&ctx.store);
    cmd.args(["--no-check-sigs", "--stdin"]);
    cmd.stdin(Stdio::from(File::open(&sorted)?));
    let status = cmd.status()?;
    if status.success() {
        return Ok(());
    }
    eprintln!("just vendor: copying already-present dependencies into the cwd store failed.");
    let kept_build = ctx.work.join("build-kept.txt");
    let kept_pending = ctx.work.join("pending-kept.ndjson");
    write_text(&kept_build, "")?;
    write_text(&kept_pending, "")?;
    for row in read_lines(pending)? {
        if row.is_empty() {
            continue;
        }
        let builder = jq_stdin_r(".builder", &row).unwrap_or_default();
        if builder.starts_with("builtin:") {
            let drv = jq_stdin_r(".drv", &row).unwrap_or_default();
            append_line(&kept_build, &format!("{drv}^*"))?;
            append_line(&kept_pending, &row)?;
        } else {
            let name = jq_stdin_r(".name", &row).unwrap_or_default();
            let drv = jq_stdin_r(".drv", &row).unwrap_or_default();
            ctx.append_skip(
                &name,
                &drv,
                "could not copy an existing dependency into the cwd store",
            )?;
        }
    }
    fs::rename(&kept_build, build_list)?;
    fs::rename(&kept_pending, pending)?;
    Ok(())
}

fn realise_lines(ctx: &Ctx, list: &Path, allowed: &HashSet<String>) -> Result<(), ToolError> {
    let lines = read_lines(list)?;
    if lines.is_empty() {
        return Ok(());
    }
    let dry = ctx.work.join("dry.json");
    let err = ctx.work.join("realise.err");
    let dry_ok = nix_stdin(
        ctx,
        &["build", "--stdin", "--dry-run", "--json", "--no-link"],
        list,
        Some(&dry),
        Some(&err),
    )?;
    if !dry_ok {
        if lines.len() <= 1 {
            let label = format!(
                "fixed-output dry-run {}",
                lines.first().map(String::as_str).unwrap_or("")
            );
            ctx.record_error(&label, &err, None)?;
            return Ok(());
        }
        return split_realise(ctx, &lines, allowed);
    }
    let mut unexpected = String::new();
    if jq_status(&["-e", ".", &p(&dry)])? {
        for dpath in jq_lines(&dry, DRY_DRVS, &[])? {
            if dpath.is_empty() {
                continue;
            }
            let normalized = normalize_drv(&dpath, &ctx.store.display().to_string());
            if !allowed.contains(&normalized) {
                unexpected = normalized;
                break;
            }
        }
    }
    if !unexpected.is_empty() {
        if lines.len() <= 1 {
            let one = lines.first().cloned().unwrap_or_default();
            let reason = format!(
                "dry-run wanted a derivation that is not fixed-output ({unexpected}); not built"
            );
            ctx.append_skip(&one, &one, &reason)?;
            return Ok(());
        }
        return split_realise(ctx, &lines, allowed);
    }
    let built = ctx.work.join("built-paths.txt");
    let built_ok = nix_stdin(
        ctx,
        &["build", "--stdin", "--no-link", "--print-build-logs"],
        list,
        Some(&built),
        Some(&err),
    )?;
    if !built_ok {
        if lines.len() <= 1 {
            let label = format!(
                "fixed-output build {}",
                lines.first().map(String::as_str).unwrap_or("")
            );
            ctx.record_error(&label, &err, None)?;
            return Ok(());
        }
        return split_realise(ctx, &lines, allowed);
    }
    Ok(())
}

fn split_realise(ctx: &Ctx, lines: &[String], allowed: &HashSet<String>) -> Result<(), ToolError> {
    static SPLIT_ID: AtomicU64 = AtomicU64::new(0);
    let id = SPLIT_ID.fetch_add(1, Ordering::Relaxed);
    let half = lines.len() / 2;
    let left = ctx.work.join(format!("build-left-{id}"));
    let right = ctx.work.join(format!("build-right-{id}"));
    write_lines(&left, &lines[..half])?;
    write_lines(&right, &lines[half..])?;
    realise_lines(ctx, &left, allowed)?;
    realise_lines(ctx, &right, allowed)
}

fn nix_stdin(
    ctx: &Ctx,
    args: &[&str],
    stdin: &Path,
    stdout: Option<&Path>,
    stderr: Option<&Path>,
) -> Result<bool, ToolError> {
    let mut cmd = ctx.nix();
    cmd.args(args);
    cmd.stdin(Stdio::from(File::open(stdin)?));
    if let Some(path) = stdout {
        cmd.stdout(Stdio::from(File::create(path)?));
    }
    if let Some(path) = stderr {
        cmd.stderr(Stdio::from(File::create(path)?));
    }
    Ok(cmd.status()?.success())
}

fn export_pending(ctx: &Ctx, pending: &Path, seen: &mut HashSet<String>) -> Result<(), ToolError> {
    if !file_nonempty(pending)? {
        return Ok(());
    }
    for row in read_lines(pending)? {
        if row.is_empty() {
            continue;
        }
        let out_path = jq_stdin_r(".outPath", &row)?;
        let dest_rel = jq_stdin_r(".vendorPath", &row)?;
        let dest = ctx.root.join(&dest_rel);
        let name = jq_stdin_r(".name", &row)?;
        let output_hash = jq_stdin_r(".narHash", &row)?;
        if out_path.is_empty() {
            ctx.append_skip(&name, "", "no output path after realisation")?;
            continue;
        }
        if export_output(&ctx.store, &ctx.sources, &out_path, &dest).is_err() {
            ctx.append_skip(
                &name,
                "",
                "fixed-output output was not in the cwd store after realisation",
            )?;
            continue;
        }
        seen.insert(output_hash);
        let kept = jq_stdin_r("del(.outPath, .builder, .drv)", &row)?;
        append_line(&ctx.work.join("sources.ndjson"), &kept)?;
    }
    Ok(())
}

fn keep_previous_sources(
    ctx: &Ctx,
    prev: &Path,
    seen: &mut HashSet<String>,
) -> Result<(), ToolError> {
    let rows = jq_lines(prev, ".sources[]?", &[]).unwrap_or_default();
    for row in rows {
        if row.is_empty() {
            continue;
        }
        let hash = jq_stdin_r(r#".narHash // """#, &row).unwrap_or_default();
        let path = jq_stdin_r(r#".vendorPath // """#, &row).unwrap_or_default();
        if hash.is_empty() || path.is_empty() || seen.contains(&hash) {
            continue;
        }
        if is_real_dir(&ctx.root.join(&path)) {
            append_line(&ctx.work.join("sources.ndjson"), &row)?;
            seen.insert(hash);
        }
    }
    Ok(())
}

fn write_final_manifest(ctx: &Ctx, stamp: &str) -> Result<(), ToolError> {
    let newp = ctx.vendor.join("manifest.json.new");
    let mut cmd = ctx.tool("jq");
    cmd.arg("-n");
    cmd.args(["--arg", "note", NOTE]);
    cmd.args(["--arg", "stamp", stamp]);
    cmd.args(["--arg", "storeRoot", "vendor/nix-store"]);
    cmd.args(["--arg", "fodNote", FOD_NOTE]);
    cmd.arg("--slurpfile")
        .arg("inputs")
        .arg(ctx.work.join("inputs.ndjson"));
    cmd.arg("--slurpfile")
        .arg("sources")
        .arg(ctx.work.join("sources.ndjson"));
    cmd.arg("--slurpfile")
        .arg("fodEvalErrors")
        .arg(ctx.work.join("fod-errors.ndjson"));
    cmd.arg("--slurpfile")
        .arg("fodSkipped")
        .arg(ctx.work.join("fod-skipped.ndjson"));
    cmd.arg(
        r#"{note:$note,storeRoot:$storeRoot,stamp:$stamp,inputs:$inputs,sources:($sources|unique_by(.narHash)),fodEvalErrors:$fodEvalErrors,fodSkipped:$fodSkipped,fodNote:$fodNote}"#,
    );
    cmd.stdout(Stdio::from(File::create(&newp)?));
    let status = cmd.status()?;
    if !status.success() {
        return Err(fail(1, "just vendor: could not write manifest.json."));
    }
    fs::rename(&newp, ctx.vendor.join("manifest.json"))?;
    Ok(())
}

fn assert_real_vendor_dirs(ctx: &Ctx) -> Result<(), ToolError> {
    let manifest = ctx.vendor.join("manifest.json");
    let paths = jq_lines(
        &manifest,
        ".inputs[].vendorPath, .sources[].vendorPath",
        &[],
    )?;
    for path in paths {
        if path.is_empty() {
            continue;
        }
        let full = ctx.root.join(&path);
        let symlink = fs::symlink_metadata(&full)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        if symlink {
            return Err(fail(
                1,
                format!("just vendor: {path} is a symlink. Refusing."),
            ));
        }
        if !is_real_dir(&full) {
            return Err(fail(1, format!("just vendor: {path} is not a directory.")));
        }
    }
    Ok(())
}

fn export_tree(src: &Path, dest: &Path, inputs: &Path, sources: &Path) -> Result<(), ToolError> {
    if !path_under(dest, inputs) && !path_under(dest, sources) {
        eprintln!("just vendor: refusing export outside vendor/inputs or vendor/sources.");
        return Err(fail(
            1,
            "just vendor: refusing export outside vendor/inputs or vendor/sources.",
        ));
    }
    let mut src_real = src.to_path_buf();
    if is_symlink(&src_real) {
        if let Ok(target) = readlink_f(&src_real) {
            src_real = target;
        }
    }
    if !src_real.is_dir() {
        eprintln!(
            "just vendor: missing source directory {}",
            src_real.display()
        );
        return Err(fail(1, "just vendor: missing source directory"));
    }
    if let Some(parent) = dest.parent() {
        let _ = fs::create_dir_all(parent);
        chmod_u_w(parent);
    }
    let _ = rm_rf(dest);
    if !cp_status(&["-a", "--link", "--"], &src_real, dest) {
        let _ = rm_rf(dest);
        let _ = cp_status(&["-a", "--"], &src_real, dest);
    }
    chmod_u_w(dest);
    if is_symlink(dest) || !is_real_dir(dest) {
        eprintln!("just vendor: {} is not a real directory.", dest.display());
        return Err(fail(1, "just vendor: export is not a real directory"));
    }
    Ok(())
}

fn export_output(
    store: &Path,
    sources: &Path,
    logical: &str,
    dest: &Path,
) -> Result<(), ToolError> {
    if !path_under(dest, sources) {
        eprintln!("just vendor: refusing source export outside vendor/sources.");
        return Err(fail(
            1,
            "just vendor: refusing source export outside vendor/sources.",
        ));
    }
    let mut phys = physical_of(store, logical).filter(|p| path_exists(p));
    if phys.is_none() && path_exists(Path::new(logical)) {
        phys = Some(PathBuf::from(logical));
    }
    let Some(mut phys) = phys else {
        return Err(fail(1, "just vendor: fixed-output output is missing"));
    };
    if is_symlink(&phys) {
        if let Ok(target) = readlink_f(&phys) {
            phys = target;
        }
    }
    let _ = rm_rf(dest);
    let _ = fs::create_dir_all(dest);
    if phys.is_dir() {
        let from = format!("{}/.", phys.display());
        let to = format!("{}/", dest.display());
        if !cp_raw(&["-a", "--link", "--"], &from, &to) {
            let _ = rm_rf(dest);
            let _ = fs::create_dir_all(dest);
            let _ = cp_raw(&["-a", "--"], &from, &to);
        }
    } else if phys.is_file() {
        unpack_archive(&phys, dest)?;
    } else {
        eprintln!(
            "just vendor: output is not a file or directory: {}",
            phys.display()
        );
        return Err(fail(1, "just vendor: output is not a file or directory"));
    }
    if is_symlink(dest) || !is_real_dir(dest) {
        eprintln!("just vendor: {} is not a real directory.", dest.display());
        return Err(fail(
            1,
            "just vendor: source export is not a real directory",
        ));
    }
    Ok(())
}

fn unpack_archive(src: &Path, dest: &Path) -> Result<(), ToolError> {
    let base = src
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("archive");
    if base.ends_with(".nar") || base.ends_with(".narinfo") {
        return cp_into(src, dest);
    }
    if base.ends_with(".zip") {
        if command_on_path("unzip") {
            let status = Command::new("unzip")
                .args(["-q", "-o"])
                .arg(src)
                .arg("-d")
                .arg(dest)
                .status()?;
            if !status.success() {
                return Err(fail(1, "just vendor: unzip failed"));
            }
            return Ok(());
        }
        if command_on_path("bsdtar") {
            let status = Command::new("bsdtar")
                .args(["-xf"])
                .arg(src)
                .arg("-C")
                .arg(dest)
                .status()?;
            if !status.success() {
                return Err(fail(1, "just vendor: bsdtar failed"));
            }
            return Ok(());
        }
        cp_into(src, dest)?;
        eprintln!("just vendor: left {base} packed (no unzip on PATH).");
        return Ok(());
    }
    if is_tar_name(base) {
        let status = Command::new("tar")
            .args(["-xf"])
            .arg(src)
            .arg("-C")
            .arg(dest)
            .status()?;
        if !status.success() {
            cp_into(src, dest)?;
            eprintln!("just vendor: could not unpack {base}; left the archive file.");
        }
        return Ok(());
    }
    cp_into(src, dest)
}

fn is_tar_name(base: &str) -> bool {
    base.ends_with(".tar")
        || base.ends_with(".tar.gz")
        || base.ends_with(".tgz")
        || base.ends_with(".tar.bz2")
        || base.ends_with(".tbz2")
        || base.ends_with(".tar.xz")
        || base.ends_with(".txz")
        || base.ends_with(".tar.zst")
}

fn source_stamp(root: &Path) -> Result<String, ToolError> {
    let mut rels = Vec::new();
    for start in ["flake.nix", "flake.lock", "nix", "modules", "hosts"] {
        let path = root.join(start);
        if !path.exists() {
            return Err(fail(
                1,
                format!("just vendor: missing {start} for the source stamp."),
            ));
        }
        collect_files(&path, root, &mut rels)?;
    }
    rels.sort();
    let mut combined = String::new();
    for chunk in rels.chunks(64) {
        let mut cmd = Command::new("sha256sum");
        cmd.current_dir(root);
        cmd.args(chunk);
        let out = cmd.output()?;
        if !out.status.success() {
            return Err(fail(1, "just vendor: sha256sum failed"));
        }
        combined.push_str(&String::from_utf8_lossy(&out.stdout));
    }
    sha256_stdin(combined.as_bytes())
}

fn collect_files(path: &Path, root: &Path, out: &mut Vec<String>) -> Result<(), ToolError> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    if meta.is_file() {
        let rel = path.strip_prefix(root).unwrap_or(path);
        out.push(rel.display().to_string());
        return Ok(());
    }
    if meta.is_dir() {
        for ent in fs::read_dir(path)? {
            collect_files(&ent?.path(), root, out)?;
        }
    }
    Ok(())
}

fn sha256_stdin(bytes: &[u8]) -> Result<String, ToolError> {
    let mut cmd = Command::new("sha256sum");
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    if let Some(mut sin) = child.stdin.take() {
        sin.write_all(bytes)?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        return Err(fail(1, "just vendor: sha256sum failed"));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let field = text.split_whitespace().next().unwrap_or("");
    if field.is_empty() {
        return Err(fail(1, "just vendor: sha256sum printed no hash"));
    }
    Ok(field.to_string())
}

fn redact_url(url: &str) -> String {
    let mut out = String::new();
    for part in url.split(' ') {
        if part.is_empty() {
            continue;
        }
        let redacted = if part.contains("://")
            && part.split_once("://").is_some_and(|(s, _)| s == "file")
        {
            "(local file url omitted)".to_string()
        } else if part.contains("token=") || part.contains("password=") || part.contains("secret=")
        {
            "(url omitted)".to_string()
        } else {
            part.to_string()
        };
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&redacted);
    }
    out
}

fn safe_source_name(output_hash: &str, name: &str) -> String {
    format!("{output_hash}-{name}")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn physical_of(store: &Path, logical: &str) -> Option<PathBuf> {
    let owned = format!("{}/nix/store/", store.display());
    if logical.starts_with(&owned) {
        return Some(PathBuf::from(logical));
    }
    if logical.starts_with("/nix/store/") {
        return Some(PathBuf::from(format!("{}{logical}", store.display())));
    }
    None
}

fn path_under(path: &Path, dir: &Path) -> bool {
    let prefix = format!("{}/", dir.display());
    path.display().to_string().starts_with(&prefix)
}

fn normalize_drv(dpath: &str, store: &str) -> String {
    let stripped = dpath.strip_prefix(store).unwrap_or(dpath);
    match stripped.find('^') {
        Some(i) => stripped[..i].to_string(),
        None => stripped.to_string(),
    }
}

fn command_on_path(name: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(name).is_file())
}

fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.is_dir())
        .unwrap_or(false)
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

fn path_exists(path: &Path) -> bool {
    path.exists()
}

fn chmod_u_w(path: &Path) {
    let _ = Command::new("chmod").arg("u+w").arg(path).status();
}

fn rm_rf(path: &Path) -> bool {
    Command::new("rm")
        .args(["-rf", "--"])
        .arg(path)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn cp_status(args: &[&str], src: &Path, dest: &Path) -> bool {
    Command::new("cp")
        .args(args)
        .arg(src)
        .arg(dest)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn cp_raw(args: &[&str], src: &str, dest: &str) -> bool {
    Command::new("cp")
        .args(args)
        .arg(src)
        .arg(dest)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn cp_into(src: &Path, dest_dir: &Path) -> Result<(), ToolError> {
    let dest = format!("{}/", dest_dir.display());
    if cp_raw(&["-a", "--"], &src.display().to_string(), &dest) {
        Ok(())
    } else {
        Err(fail(1, "just vendor: cp failed"))
    }
}

fn readlink_f(path: &Path) -> Result<PathBuf, ToolError> {
    let out = Command::new("readlink")
        .args(["-f", "--"])
        .arg(path)
        .output()?;
    if !out.status.success() {
        return Err(fail(1, "just vendor: readlink failed"));
    }
    let text = trim_nl(&String::from_utf8_lossy(&out.stdout));
    if text.is_empty() {
        return Err(fail(1, "just vendor: readlink was empty"));
    }
    Ok(PathBuf::from(text))
}

fn write_text(path: &Path, text: &str) -> Result<(), ToolError> {
    fs::write(path, text)?;
    Ok(())
}

fn append_line(path: &Path, line: &str) -> Result<(), ToolError> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", line.trim_end_matches('\n'))?;
    Ok(())
}

fn write_lines(path: &Path, lines: &[String]) -> Result<(), ToolError> {
    let mut file = File::create(path)?;
    for line in lines {
        writeln!(file, "{line}")?;
    }
    Ok(())
}

fn read_lines(path: &Path) -> Result<Vec<String>, ToolError> {
    let text = fs::read_to_string(path).unwrap_or_default();
    Ok(text
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

fn file_nonempty(path: &Path) -> Result<bool, ToolError> {
    let len = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Ok(len > 0)
}

fn tail_lines(path: &Path, n: usize) -> String {
    let text = fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

fn trim_nl(text: &str) -> String {
    text.trim_end_matches('\n').to_string()
}

fn p(path: &Path) -> String {
    path.display().to_string()
}

fn fail(code: i32, message: impl Into<String>) -> ToolError {
    ToolError {
        message: message.into(),
        exit_code: code,
    }
}

fn jq_status(args: &[&str]) -> Result<bool, ToolError> {
    Ok(Command::new("jq").args(args).status()?.success())
}

fn jq_to_file(args: &[&str], dest: &Path) -> Result<(), ToolError> {
    let mut cmd = Command::new("jq");
    cmd.args(args);
    cmd.stdout(Stdio::from(File::create(dest)?));
    let status = cmd.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(fail(1, "just vendor: jq failed"))
    }
}

fn jq_r(file: &Path, filter: &str, args: &[(&str, &str)]) -> Result<String, ToolError> {
    let mut cmd = Command::new("jq");
    cmd.arg("-r");
    for (key, value) in args {
        cmd.arg("--arg").arg(key).arg(value);
    }
    cmd.arg(filter);
    cmd.arg(file);
    let out = cmd.output()?;
    if !out.status.success() {
        return Err(fail(
            1,
            format!(
                "just vendor: jq failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        ));
    }
    Ok(trim_nl(&String::from_utf8_lossy(&out.stdout)))
}

fn jq_lines(file: &Path, filter: &str, args: &[(&str, &str)]) -> Result<Vec<String>, ToolError> {
    let text = jq_r(file, filter, args)?;
    if text.is_empty() {
        return Ok(Vec::new());
    }
    Ok(text.lines().map(str::to_string).collect())
}

fn jq_s_length(file: &Path) -> Result<usize, ToolError> {
    let text = jq_r_args(&["-s", "length", &p(file)])?;
    text.trim()
        .parse()
        .map_err(|_| fail(1, "just vendor: jq length was not a number"))
}

fn jq_r_args(args: &[&str]) -> Result<String, ToolError> {
    let out = Command::new("jq").args(args).output()?;
    if !out.status.success() {
        return Err(fail(1, "just vendor: jq failed"));
    }
    Ok(trim_nl(&String::from_utf8_lossy(&out.stdout)))
}

fn jq_nc(args: &[(&str, &str)], filter: &str) -> Result<String, ToolError> {
    let mut cmd = Command::new("jq");
    cmd.arg("-nc");
    for (key, value) in args {
        cmd.arg("--arg").arg(key).arg(value);
    }
    cmd.arg(filter);
    let out = cmd.output()?;
    if !out.status.success() {
        return Err(fail(1, "just vendor: jq -n failed"));
    }
    Ok(trim_nl(&String::from_utf8_lossy(&out.stdout)))
}

fn jq_stdin_r(filter: &str, json: &str) -> Result<String, ToolError> {
    let mut cmd = Command::new("jq");
    cmd.args(["-r", filter]);
    cmd.stdin(Stdio::piped());
    let mut child = cmd.spawn()?;
    if let Some(mut sin) = child.stdin.take() {
        sin.write_all(json.as_bytes())?;
        sin.write_all(b"\n")?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        return Err(fail(
            1,
            format!(
                "just vendor: jq failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        ));
    }
    Ok(trim_nl(&String::from_utf8_lossy(&out.stdout)))
}

fn jq_stdin_lines(filter: &str, json: &str) -> Result<Vec<String>, ToolError> {
    let text = jq_stdin_r(filter, json)?;
    if text.is_empty() {
        return Ok(Vec::new());
    }
    Ok(text.lines().map(str::to_string).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_file_url_and_token() {
        assert_eq!(
            redact_url("file:///var/lib/x keep"),
            "(local file url omitted) keep"
        );
        assert_eq!(redact_url("https://ex.test/a?token=s"), "(url omitted)");
        assert_eq!(
            redact_url("https://ex.test/passwordless"),
            "https://ex.test/passwordless"
        );
    }

    #[test]
    fn safe_name_replaces_other_bytes() {
        assert_eq!(
            safe_source_name("sha256-abc", "foo bar"),
            "sha256-abc-foo_bar"
        );
    }

    #[test]
    fn physical_paths_stay_in_the_chroot_store() {
        let store = Path::new("/work/vendor/nix-store");
        assert_eq!(
            physical_of(store, "/nix/store/abc").unwrap(),
            PathBuf::from("/work/vendor/nix-store/nix/store/abc")
        );
        assert_eq!(
            physical_of(store, "/work/vendor/nix-store/nix/store/abc").unwrap(),
            PathBuf::from("/work/vendor/nix-store/nix/store/abc")
        );
        assert!(physical_of(store, "/etc/passwd").is_none());
    }

    #[test]
    fn store_must_sit_under_vendor() {
        let vendor = Path::new("/work/vendor");
        assert!(path_under(Path::new("/work/vendor/nix-store"), vendor));
        assert!(!path_under(Path::new("/work/vendor"), vendor));
        assert!(!path_under(Path::new("/nix/store"), vendor));
    }

    #[test]
    fn drv_prefix_strip_matches_the_chroot_store() {
        let store = "/work/vendor/nix-store";
        assert_eq!(
            normalize_drv(&format!("{store}/nix/store/abc.drv^*"), store),
            "/nix/store/abc.drv"
        );
        assert_eq!(
            normalize_drv("/nix/store/abc.drv", store),
            "/nix/store/abc.drv"
        );
    }
}
