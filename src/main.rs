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

    let mut go_process = start_go_process();

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

                go_process = start_go_process();
                last_reload = Instant::now();
            }
        }
    }

    Ok(())
}

fn start_go_process() -> Child {
    let mut cmd = Command::new("go");
    cmd.args(["run", "main.go"]);

    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            libc::setpgid(0, 0);
            Ok(())
        });
    }

    cmd.spawn().expect("Failed to run `go run main.go`")
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