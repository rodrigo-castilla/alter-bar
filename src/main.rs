//! alter-bar: switches the top bar between several named options.
//!
//! Two are always built in, `quickshell` (the Omarchy shell) and `shoji-bar-4`
//! (~/.config/shoji-bar-4), and more can be registered with `set-bar <name> <path>`: each one
//! is a shoji project (or an executable) with its own binary. `list` shows them with a `*` on
//! the chosen one and `toggle` rotates to the next one in the list.
//!
//! Only one of them runs at a time: choosing a shoji bar stops Quickshell (the shoji bar answers
//! what Omarchy sends to its shell, see the fork's `omarchy.rs`), and choosing Quickshell stops
//! shoji. While a shoji bar still lacks some Omarchy action, `ALTER_BAR_COEXIST=1` keeps the old
//! way: Quickshell stays alive with only its bar hidden. The chosen name is saved on disk and
//! `apply` restores it when the session starts (call it from Hyprland's autostart).

use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio, exit};
use std::thread::sleep;
use std::time::{Duration, Instant};

const QUICKSHELL: &str = "quickshell";
const SHOJI_ORIGINAL: &str = "shoji-bar-4";
/// Where the binary is inside a shoji project.
const SHOJI_BIN: &str = "target/release/shoji-bar-4";
/// Name of the process (every shoji variant has the same one).
const SHOJI_PROC: &str = "shoji-bar-4";

#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    Quickshell,
    /// A shoji project (a directory with `target/release/shoji-bar-4`) or an executable.
    Shoji(PathBuf),
}

#[derive(Clone, Debug)]
struct Bar {
    name: String,
    kind: Kind,
}

impl Bar {
    /// The executable of a shoji bar.
    fn exe(&self) -> Option<PathBuf> {
        match &self.kind {
            Kind::Quickshell => None,
            Kind::Shoji(path) => Some(shoji_exe(path)),
        }
    }
}

/// The cargo profile a project is rebuilt with: `fast` (no LTO, incremental: a change in
/// `plugins/` costs ~1 s instead of ~20 s) if the project defines it, `release` otherwise.
fn profile_of(project: &Path) -> &'static str {
    match fs::read_to_string(project.join("Cargo.toml")) {
        Ok(toml) if toml.contains("[profile.fast]") => "fast",
        _ => "release",
    }
}

fn shoji_exe(path: &Path) -> PathBuf {
    if !path.is_dir() {
        return path.to_path_buf();
    }
    let fast = path.join(SHOJI_BIN.replace("release", "fast"));
    let release = path.join(SHOJI_BIN);
    // Before the first `reload` only the release one may exist: it is good enough to start.
    if profile_of(path) == "fast" && (fast.is_file() || !release.is_file()) {
        fast
    } else {
        release
    }
}

fn home() -> PathBuf {
    PathBuf::from(env::var("HOME").expect("HOME is not set"))
}

/// Expands a leading `~/`, in case the path arrives without the shell having done it.
fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None if path == "~" => home(),
        None => PathBuf::from(path),
    }
}

fn state_file() -> PathBuf {
    let base = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/state"));
    base.join("alter-bar/mode")
}

fn registry_file() -> PathBuf {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"));
    base.join("alter-bar/bars")
}

/// Built-in bars the user has renamed away: `!name` lines in the registry.
fn read_hidden() -> Vec<String> {
    fs::read_to_string(registry_file())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().strip_prefix('!').map(|n| n.trim().to_owned()))
        .collect()
}

/// The bars registered by the user: `name = path` lines, in order.
fn read_registry() -> Vec<(String, PathBuf)> {
    let Ok(text) = fs::read_to_string(registry_file()) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (name, path) = line.split_once('=')?;
            Some((name.trim().to_owned(), PathBuf::from(path.trim())))
        })
        .collect()
}

fn write_registry(entries: &[(String, PathBuf)]) -> io::Result<()> {
    write_registry_with(entries, &read_hidden())
}

fn write_registry_with(entries: &[(String, PathBuf)], hidden: &[String]) -> io::Result<()> {
    let path = registry_file();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut text = String::from("# Bars registered with `alter-bar set-bar <name> <path>`\n");
    for (name, path) in entries {
        text.push_str(&format!("{name} = {}\n", path.display()));
    }
    for name in hidden {
        text.push_str(&format!("!{name}\n"));
    }
    fs::write(path, text)
}

/// All the bars, in the order `toggle` rotates through them: the built-in ones, then the registered ones.
fn bars() -> Vec<Bar> {
    let mut list = vec![
        Bar {
            name: QUICKSHELL.into(),
            kind: Kind::Quickshell,
        },
        Bar {
            name: SHOJI_ORIGINAL.into(),
            kind: Kind::Shoji(home().join(".config/shoji-bar-4")),
        },
    ];
    if read_hidden().iter().any(|n| n == SHOJI_ORIGINAL) {
        list.retain(|b| b.name != SHOJI_ORIGINAL);
    }
    for (name, path) in read_registry() {
        if name == QUICKSHELL {
            continue;
        }
        match list.iter_mut().find(|b| b.name == name) {
            // Registering `shoji-bar-4` changes where the original is.
            Some(existing) => existing.kind = Kind::Shoji(path),
            None => list.push(Bar {
                name,
                kind: Kind::Shoji(path),
            }),
        }
    }
    list
}

fn current_name() -> String {
    let saved = fs::read_to_string(state_file()).unwrap_or_default();
    match saved.trim() {
        "" => QUICKSHELL.into(),
        // The name the first version gave the original.
        "shoji" => SHOJI_ORIGINAL.into(),
        other => other.to_owned(),
    }
}

/// The chosen bar; quickshell if it is no longer in the list.
fn current(list: &[Bar]) -> Bar {
    let name = current_name();
    list.iter()
        .find(|b| b.name == name)
        .unwrap_or(&list[0])
        .clone()
}

fn write_current(name: &str) -> io::Result<()> {
    let path = state_file();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, format!("{name}\n"))
}

/// Runs a command and returns whether it succeeded.
fn run(cmd: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> bool {
    Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn wait_until(mut cond: impl FnMut() -> bool, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if cond() {
            return true;
        }
        sleep(Duration::from_millis(100));
    }
    cond()
}

fn quickshell_running() -> bool {
    // The Omarchy shell: `quickshell -n -p $OMARCHY_PATH/shell`
    run("pgrep", &["-f", "quickshell .*omarchy/shell"])
}

/// Ready to take calls: it answers, and its socket is there (shoji's takeover of it needs that).
fn quickshell_ready() -> bool {
    let runtime = env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/run/user/1000"));
    let has_socket = fs::read_dir(runtime).is_ok_and(|d| {
        d.flatten().any(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.starts_with("omarchy-shell-") && n.ends_with(".sock")
        })
    });
    has_socket && run("omarchy-shell", &["shell", "ping"])
}

fn start_quickshell() {
    // Like omarchy-restart-shell: Hyprland launches it so that it inherits the session's
    // canonical environment.
    if !run(
        "hyprctl",
        &["dispatch", "hl.dsp.exec_cmd(\"omarchy-launch-shell\")"],
    ) {
        let _ = Command::new("setsid")
            .args(["-f", "omarchy-launch-shell"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

/// Whether Quickshell stays alive under a shoji bar (`ALTER_BAR_COEXIST=1`), with only its bar hidden.
fn coexist() -> bool {
    env::var("ALTER_BAR_COEXIST").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// Hides or shows only Omarchy's bar. The flag lives on disk, so it survives restarts.
fn omarchy_bar_hidden(hidden: bool) {
    run("omarchy-toggle-bar", &[if hidden { "on" } else { "off" }]);
}

/// Stops Quickshell: its own `qs kill` first, a TERM if that does not do it.
fn stop_quickshell() {
    if !quickshell_running() {
        return;
    }
    let shell = env::var("OMARCHY_PATH").unwrap_or_else(|_| "/usr/share/omarchy".into());
    run("qs", &["kill", "-n", "-p", &format!("{shell}/shell")]);
    if !wait_until(|| !quickshell_running(), Duration::from_secs(3)) {
        run("pkill", &["-TERM", "-f", "quickshell .*omarchy/shell"]);
        wait_until(|| !quickshell_running(), Duration::from_secs(3));
    }
}

/// The shoji processes that are running, with the executable of each.
fn shoji_processes() -> Vec<(u32, PathBuf)> {
    let Ok(out) = Command::new("pgrep").args(["-x", SHOJI_PROC]).output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .filter_map(|pid| {
            let exe = fs::read_link(format!("/proc/{pid}/exe")).ok()?;
            // A binary rebuilt while it was running shows up as "... (deleted)".
            let text = exe.to_string_lossy();
            let clean = text.strip_suffix(" (deleted)").unwrap_or(&text);
            Some((pid, PathBuf::from(clean)))
        })
        .collect()
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Whether the executable `exe` is the one running as shoji.
fn shoji_running(exe: &Path) -> bool {
    shoji_processes().iter().any(|(_, e)| same_file(e, exe))
}

fn pid_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

/// Stops every shoji that is running, except the executable `keep` if one is given.
fn stop_shoji(keep: Option<&Path>) {
    let all = shoji_processes();
    let alone = all.len() == 1;
    for (pid, exe) in all {
        if keep.is_some_and(|k| same_file(&exe, k)) {
            continue;
        }
        // `msg quit` closes it cleanly, but it talks to whichever shoji listens on the socket:
        // it only works if this is the only one running.
        if alone {
            run(&exe, &["msg", "quit"]);
            wait_until(|| !pid_alive(pid), Duration::from_secs(3));
        }
        if pid_alive(pid) {
            run("kill", &["-TERM", &pid.to_string()]);
            wait_until(|| !pid_alive(pid), Duration::from_secs(3));
        }
        if pid_alive(pid) {
            run("kill", &["-KILL", &pid.to_string()]);
        }
    }
}

fn start_shoji(exe: &Path) -> io::Result<()> {
    // setsid: so that it outlives this process and the terminal it is launched from.
    Command::new("setsid")
        .arg("-f")
        .arg(exe)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?
        .wait()?;
    Ok(())
}

/// Leaves only the requested bar: the others are stopped (Quickshell too, unless
/// `ALTER_BAR_COEXIST` is set, which hides its bar and keeps it alive).
fn activate(bar: &Bar) -> io::Result<()> {
    match bar.exe() {
        None => {
            stop_shoji(None);
            if !quickshell_running() {
                start_quickshell();
            }
            wait_until(quickshell_ready, Duration::from_secs(60));
            omarchy_bar_hidden(false);
        }
        Some(exe) => {
            if !exe.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "{}: {} does not exist (is `alter-bar reload` missing?)",
                        bar.name,
                        exe.display()
                    ),
                ));
            }
            // Any other shoji variant goes away; if this one is already running, it stays.
            stop_shoji(Some(&exe));
            if coexist() {
                if !quickshell_running() {
                    start_quickshell();
                }
                // shoji must start AFTER quickshell: whichever gets there first owns the D-Bus
                // notifications name, and here we want Omarchy's.
                wait_until(quickshell_ready, Duration::from_secs(60));
                omarchy_bar_hidden(true);
            } else {
                // Quickshell goes first so that shoji is the one that owns the notifications
                // name and the shell's socket. Its bar flag is put back to visible, so that the
                // next time Quickshell starts it shows its bar.
                stop_quickshell();
                omarchy_bar_hidden(false);
            }
            if !shoji_running(&exe) {
                start_shoji(&exe)?;
            }
        }
    }
    Ok(())
}

/// The result of building a shoji project.
enum Build {
    /// Cargo rebuilt and the binary changed.
    Changed,
    /// There was nothing to build.
    Unchanged,
}

fn modified(path: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// State shared between the thread that reads cargo's output and the one that draws.
#[derive(Default)]
struct Progress {
    compiled: usize,
    current: String,
    errors: Vec<String>,
    warnings: Vec<String>,
}

fn history_file(project: &Path) -> PathBuf {
    let key: String = project
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    home().join(".local/state/alter-bar").join(format!("build-{key}"))
}

fn fmt_secs(d: Duration) -> String {
    format!("{:.1}s", d.as_secs_f32())
}

/// Draws `[=====>    ] 3/7 12.3s ⠹ building shoji-bar-4`. With an unknown (or exceeded) total the
/// bar is a stretch that goes back and forth.
fn render(state: &Progress, expected: usize, tick: usize, elapsed: Duration) -> String {
    const SPIN: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    const W: usize = 24;
    let known = expected > 0 && state.compiled <= expected;
    let bar: String = if known {
        let filled = (state.compiled * W / expected).min(W - 1);
        (0..W)
            .map(|i| if i < filled { '=' } else if i == filled { '>' } else { ' ' })
            .collect()
    } else {
        let period = 2 * (W - 6);
        let t = tick % period;
        let at = if t < W - 6 { t } else { period - t };
        (0..W).map(|i| if i >= at && i < at + 6 { '=' } else { ' ' }).collect()
    };
    let count = if known {
        format!("{}/{}", state.compiled, expected)
    } else {
        format!("{}", state.compiled)
    };
    format!(
        "[{bar}] {count} {} {} {}",
        fmt_secs(elapsed),
        SPIN[tick % SPIN.len()],
        if state.current.is_empty() {
            "preparing".to_owned()
        } else {
            format!("building {}", state.current)
        }
    )
}

/// Runs `cargo build --profile <profile>` in the project. Cargo decides what to rebuild (with the
/// `fast` profile and its incremental cache, only the crate that changed); if there is nothing to
/// do it ends at once. Whether anything changed is told by whether the binary is a different one.
///
/// While it builds it shows a progress bar, the time and a spinner. The total comes from the last
/// build of this project (kept in `~/.local/state/alter-bar`).
///
/// Cargo's output is captured: every build repeats the hundreds of warnings of the dependencies,
/// so only the errors (with file and line) are shown, and only if the build fails.
fn build(project: &Path, exe: &Path) -> Result<Build, String> {
    use std::io::{BufRead, BufReader, IsTerminal, Write};
    use std::sync::{Arc, Mutex};

    let before = modified(exe);
    let profile = profile_of(project);
    let spawn = |program: PathBuf| {
        Command::new(program)
            .args(["build", "--profile", profile, "--message-format", "short"])
            .current_dir(project)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
    };
    let mut child = spawn(PathBuf::from("cargo"))
        .or_else(|e| {
            // From a Hyprland keybinding the PATH may not include ~/.cargo/bin.
            if e.kind() == io::ErrorKind::NotFound {
                spawn(home().join(".cargo/bin/cargo"))
            } else {
                Err(e)
            }
        })
        .map_err(|e| format!("could not run cargo: {e}"))?;

    let state = Arc::new(Mutex::new(Progress::default()));
    let reader = {
        let state = Arc::clone(&state);
        let stderr = child.stderr.take().expect("stderr captured");
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let mut st = state.lock().unwrap();
                let word = line.trim_start();
                if let Some(rest) = word.strip_prefix("Compiling ") {
                    st.compiled += 1;
                    st.current = rest.split_whitespace().next().unwrap_or("").to_owned();
                } else if line.starts_with("warning: shoji-bar-4@") {
                    // The project's own warnings (build.rs: "init-default.rs does not define patch:
                    // ignoring it") do matter, even if the build succeeds.
                    st.warnings.push(line);
                } else if line.contains(": error") || line.starts_with("error") {
                    st.errors.push(line);
                }
            }
        })
    };

    let expected: usize = fs::read_to_string(history_file(project))
        .ok()
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0);
    let tty = io::stdout().is_terminal();
    let started = Instant::now();
    let mut tick = 0;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => return Err(format!("cargo: {e}")),
        }
        if tty {
            let line = render(&state.lock().unwrap(), expected, tick, started.elapsed());
            print!("\r\x1b[2K{line}");
            let _ = io::stdout().flush();
        }
        tick += 1;
        std::thread::sleep(Duration::from_millis(80));
    };
    let _ = reader.join();
    let st = state.lock().unwrap();
    if tty {
        print!("\r\x1b[2K");
        let _ = io::stdout().flush();
    }
    for w in &st.warnings {
        eprintln!("{w}");
    }
    if !status.success() {
        for e in &st.errors {
            eprintln!("{e}");
        }
        return Err("the build failed: the bar is left as it was".into());
    }
    if st.compiled > 0 {
        let _ = fs::create_dir_all(history_file(project).parent().unwrap());
        let _ = fs::write(history_file(project), st.compiled.to_string());
    }
    println!(
        "built in {} ({} crates, profile {profile})",
        fmt_secs(started.elapsed()),
        st.compiled
    );
    if !exe.is_file() {
        return Err(format!("the build did not leave {}", exe.display()));
    }
    Ok(if modified(exe) != before {
        Build::Changed
    } else {
        Build::Unchanged
    })
}

/// Reloads the chosen bar. If it is a shoji project it is built first when needed (that is how
/// changes in `plugins/` are picked up); if the build fails, the running bar is left alone.
fn reload(bar: &Bar) -> Result<(), String> {
    match &bar.kind {
        Kind::Quickshell => {
            run("omarchy-restart-shell", &[]);
            Ok(())
        }
        Kind::Shoji(path) => {
            if path.is_dir() && path.join("Cargo.toml").is_file() {
                let exe = shoji_exe(path);
                println!("building {} (profile {})", path.display(), profile_of(path));
                match build(path, &exe) {
                    Ok(Build::Changed) => println!("built: there are changes"),
                    Ok(Build::Unchanged) => println!("nothing to build"),
                    Err(e) => {
                        run(
                            "notify-send",
                            &["-u", "critical", "alter-bar", &format!("{}: {e}", bar.name)],
                        );
                        return Err(e);
                    }
                }
            }
            stop_shoji(None);
            activate(bar).map_err(|e| e.to_string())?;
            println!("{} reloaded", bar.name);
            Ok(())
        }
    }
}

fn find<'a>(list: &'a [Bar], name: &str) -> Result<&'a Bar, String> {
    // `shoji` was the name the first version gave the original.
    let name = if name == "shoji" { SHOJI_ORIGINAL } else { name };
    list.iter().find(|b| b.name == name).ok_or_else(|| {
        let names: Vec<&str> = list.iter().map(|b| b.name.as_str()).collect();
        format!(
            "there is no bar called `{name}` (there are: {})",
            names.join(", ")
        )
    })
}

fn select(bar: &Bar) -> io::Result<()> {
    write_current(&bar.name)?;
    activate(bar)?;
    println!("{}", bar.name);
    Ok(())
}

/// Rotates `step` positions in the list (1 next, -1 previous).
fn rotate(step: isize) -> io::Result<()> {
    let list = bars();
    let now = current(&list);
    let at = list.iter().position(|b| b.name == now.name).unwrap_or(0) as isize;
    let next = &list[(at + step).rem_euclid(list.len() as isize) as usize];
    select(next)
}

fn list() {
    let list = bars();
    let now = current(&list).name;
    let width = list.iter().map(|b| b.name.len()).max().unwrap_or(0);
    for bar in &list {
        let mark = if bar.name == now { '*' } else { ' ' };
        let detail = match (&bar.kind, bar.exe()) {
            (Kind::Quickshell, _) => "built in (Omarchy shell)".to_owned(),
            (Kind::Shoji(path), Some(exe)) if !exe.is_file() => {
                format!("{}  [no binary]", path.display())
            }
            (Kind::Shoji(path), _) => match plugin_in(path) {
                Some(file) => format!("{}  [plugins/{file}]", path.display()),
                None => path.display().to_string(),
            },
        };
        println!("{mark} {:<width$}  {detail}", bar.name);
    }
}

/// The plugin that customises a shoji project, if it has one in its `plugins/` folder.
fn plugin_in(project: &Path) -> Option<&'static str> {
    // Only the refactored fork reads `plugins/`: the build.rs that picks them gives it away.
    if !project.join("crates/shoji-bar-4/build.rs").is_file() {
        return None;
    }
    ["init.rs", "init-default.rs"]
        .into_iter()
        .find(|f| project.join("plugins").join(f).is_file())
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn set_bar(name: &str, path: &str) -> Result<(), String> {
    if !valid_name(name) {
        return Err(format!(
            "invalid name `{name}`: only letters, digits, `-` and `_`"
        ));
    }
    if name == QUICKSHELL {
        return Err("`quickshell` is a built-in bar and takes no path".into());
    }
    let path = expand(path);
    let path = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut entries = read_registry();
    match entries.iter_mut().find(|(n, _)| n == name) {
        Some(entry) => entry.1 = path.clone(),
        None => entries.push((name.to_owned(), path.clone())),
    }
    let mut hidden = read_hidden();
    hidden.retain(|n| n != name);
    write_registry_with(&entries, &hidden).map_err(|e| e.to_string())?;
    let exe = shoji_exe(&path);
    println!("{name} -> {}", path.display());
    if !exe.is_file() {
        println!(
            "warning: {} does not exist yet (alter-bar reload)",
            exe.display()
        );
    }
    Ok(())
}

/// Gives a bar another name, keeping where it is and whether it is the chosen one.
fn rename_bar(old: &str, new: &str) -> Result<(), String> {
    let old = if old == "shoji" { SHOJI_ORIGINAL } else { old };
    if !valid_name(new) {
        return Err(format!("invalid name `{new}`: only letters, digits, `-` and `_`"));
    }
    if old == QUICKSHELL || new == QUICKSHELL {
        return Err("`quickshell` is a built-in bar and keeps its name".into());
    }
    let list = bars();
    let bar = find(&list, old)?.clone();
    if list.iter().any(|b| b.name == new) {
        return Err(format!("there is already a bar called `{new}`"));
    }
    let Kind::Shoji(path) = bar.kind else { unreachable!("quickshell was refused above") };
    let mut entries = read_registry();
    let mut hidden = read_hidden();
    hidden.retain(|n| n != new);
    match entries.iter_mut().find(|(n, _)| n == old) {
        Some(entry) => entry.0 = new.to_owned(),
        None => {
            // The built-in original: the new name takes its path and the old one is hidden.
            entries.push((new.to_owned(), path));
            hidden.push(old.to_owned());
        }
    }
    write_registry_with(&entries, &hidden).map_err(|e| e.to_string())?;
    if current_name() == old {
        write_current(new).map_err(|e| e.to_string())?;
    }
    println!("{old} -> {new}");
    Ok(())
}

fn remove_bar(name: &str) -> Result<(), String> {
    let mut entries = read_registry();
    let before = entries.len();
    entries.retain(|(n, _)| n != name);
    if entries.len() == before {
        return Err(format!(
            "`{name}` is not registered (built-in bars cannot be removed)"
        ));
    }
    write_registry(&entries).map_err(|e| e.to_string())
}

fn status() {
    let list = bars();
    println!("bar: {}", current(&list).name);
    println!("quickshell: {}", quickshell_running());
    let running = shoji_processes();
    if running.is_empty() {
        println!("shoji: not running");
    }
    for (pid, exe) in running {
        let name = list
            .iter()
            .find(|b| b.exe().is_some_and(|e| same_file(&e, &exe)))
            .map_or("?", |b| b.name.as_str());
        println!("shoji: {name} (pid {pid}, {})", exe.display());
    }
}

fn usage() -> ! {
    eprintln!(
        "usage: alter-bar <command>\n\
         \n  list                    list the bars; `*` marks the chosen one\n  \
         toggle | next           switch to the next one in the list (rotates)\n  \
         prev                    switch to the previous one\n  \
         set <name>              choose a bar\n  \
         set-bar <name> <path>   register a shoji bar (a project or an executable)\n  \
         remove-bar <name>       remove a registered bar\n  \
         rename <old> <new>      give a bar another name\n  \
         reload                  reload the chosen one; a shoji project is built first if it changed\n  \
         status                  show the chosen bar and what is running\n  \
         apply                   activate the chosen bar (for login)\n  \
         version | --version | -V  show the version"
    );
    exit(2);
}

fn fail(msg: impl fmt::Display) -> ! {
    eprintln!("alter-bar: {msg}");
    exit(1);
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["list" | "ls"] => {
            list();
            Ok(())
        }
        ["toggle" | "next"] => rotate(1),
        ["prev"] => rotate(-1),
        ["set", name] => {
            let list = bars();
            match find(&list, name) {
                Ok(bar) => select(bar),
                Err(e) => fail(e),
            }
        }
        ["set-bar", name, path] => {
            if let Err(e) = set_bar(name, path) {
                fail(e);
            }
            Ok(())
        }
        ["rename", old, new] => {
            if let Err(e) = rename_bar(old, new) {
                fail(e);
            }
            Ok(())
        }
        ["remove-bar", name] => {
            if let Err(e) = remove_bar(name) {
                fail(e);
            }
            Ok(())
        }
        ["reload"] => {
            let list = bars();
            reload(&current(&list)).map_err(io::Error::other)
        }
        ["status"] => {
            status();
            Ok(())
        }
        ["--version" | "-V" | "version"] => {
            println!("alter-bar {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        ["apply"] => {
            let list = bars();
            activate(&current(&list))
        }
        _ => usage(),
    };
    if let Err(e) = result {
        fail(e);
    }
}
