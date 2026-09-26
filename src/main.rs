use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::process::{Child, Command};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting Go Watcher...");
    println!("👀 Watching for .go file changes in current directory and subdirectories...\n");

    let entry = resolve_entry();
    println!("🎯 Entry point: {entry}");
    let mut go_process = start_go_process(&entry);

    let (tx, rx) = channel();

    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;

    watcher.watch(Path::new("."), RecursiveMode::Recursive)?;

    let mut last_reload = Instant::now();

    for res in rx {
        if let Ok(event) = res {
            if is_go_file_change(&event) {
                if last_reload.elapsed() < Duration::from_millis(500) {
                    continue;
                }

                println!("\n🔄 .go file changed! restarting....");

                kill_process_group(&mut go_process);

                go_process = start_go_process(&entry);
                last_reload = Instant::now();
            }
        }
    }

    Ok(())
}

/// Resolve the `go run` target: CLI argument, or auto-detect
/// (`main.go` in the root, then `cmd/<name>/main.go` if there is only one).
fn resolve_entry() -> String {
    if let Some(arg) = std::env::args().nth(1) {
        return arg;
    }
    if Path::new("main.go").exists() {
        return "main.go".to_string();
    }
    let mut found: Vec<String> = std::fs::read_dir("cmd")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("main.go").exists())
        .map(|e| format!("./cmd/{}", e.file_name().to_string_lossy()))
        .collect();
    found.sort();
    match found.len() {
        1 => found.remove(0),
        0 => ".".to_string(),
        _ => {
            eprintln!("❌ Multiple entry points found: {found:?}");
            eprintln!("   Run: go_watcher <path>, e.g. go_watcher {}", found[0]);
            std::process::exit(1);
        }
    }
}

/// Location of main.go, used as the port detection fallback.
fn entry_main_file(entry: &str) -> std::path::PathBuf {
    let p = Path::new(entry);
    if p.extension().map_or(false, |e| e == "go") {
        p.to_path_buf()
    } else {
        p.join("main.go")
    }
}

/// Find the port from `.env` (any key ending in PORT), otherwise from `main.go`.
fn detect_port(entry: &str) -> Option<u16> {
    if let Ok(content) = std::fs::read_to_string(".env") {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                if key.trim().trim_start_matches("export ").trim().to_uppercase().ends_with("PORT") {
                    let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
                    if let Ok(port) = value.trim_start_matches(':').parse::<u16>() {
                        return Some(port);
                    }
                }
            }
        }
    }

    let source = std::fs::read_to_string(entry_main_file(entry)).ok()?;
    let bytes = source.as_bytes();
    // look for Go string literals such as ":8080" or "8080"
    for (i, w) in bytes.windows(2).enumerate() {
        if w[0] == b'"' {
            let rest = &source[i + 1..];
            let rest = rest.strip_prefix(':').unwrap_or(rest);
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if (2..=5).contains(&digits.len()) && rest[digits.len()..].starts_with('"') {
                if let Ok(port) = digits.parse::<u16>() {
                    if port >= 1024 {
                        return Some(port);
                    }
                }
            }
        }
    }
    None
}

/// If the port is in use (LISTEN), kill the process using it.
fn free_port(port: u16) {
    #[cfg(unix)]
    {
        let out = Command::new("lsof")
            .args(["-ti", &format!("tcp:{port}"), "-sTCP:LISTEN"])
            .output();
        if let Ok(out) = out {
            for pid in String::from_utf8_lossy(&out.stdout).split_whitespace() {
                println!("⚠️  Port {port} is in use (PID {pid}), killing...");
                let _ = Command::new("kill").args(["-9", pid]).status();
            }
        }
    }

    #[cfg(windows)]
    {
        if let Ok(out) = Command::new("netstat").args(["-ano", "-p", "tcp"]).output() {
            let needle = format!(":{port}");
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let cols: Vec<&str> = line.split_whitespace().collect();
                if cols.len() >= 5 && cols[1].ends_with(&needle) && cols[3] == "LISTENING" {
                    println!("⚠️  Port {port} is in use (PID {}), killing...", cols[4]);
                    let _ = Command::new("taskkill").args(["/F", "/T", "/PID", cols[4]]).output();
                }
            }
        }
    }
}

fn start_go_process(entry: &str) -> Child {
    match detect_port(entry) {
        Some(port) => {
            println!("🔌 Detected port: {port}");
            free_port(port);
        }
        None => println!("🔌 No port found in .env / {entry}, skipping port check"),
    }

    let mut cmd = Command::new("go");
    cmd.args(["run", entry]);

    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            libc::setpgid(0, 0);
            Ok(())
        });
    }

    cmd.spawn().unwrap_or_else(|_| panic!("Failed to run `go run {entry}`"))
}

fn kill_process_group(child: &mut Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
        let _ = child.wait();
    }

    #[cfg(windows)]
    {
        let pid_str = child.id().to_string();
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid_str])
            .output();
        let _ = child.wait();
    }
}

fn is_go_file_change(event: &Event) -> bool {
    let is_modified = matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    );

    is_modified && event.paths.iter().any(|path| {
        let path_str = path.to_string_lossy();

        if path_str.contains(".git") || path_str.contains("vendor") || path_str.contains("target") {
            return false;
        }

        path.extension().map_or(false, |ext| ext == "go")
    })
}