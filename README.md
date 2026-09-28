# go_watcher

A small hot-reload tool for Go projects, written in Rust. It watches the current directory for `.go` file changes and automatically restarts your app with `go run`.

## Features

- Recursively watches the current directory for created, modified, or removed `.go` files
- Restarts the app by killing the whole process group, so no orphaned child processes are left behind
- Debounces rapid changes (500 ms) to avoid multiple restarts on a single save
- Auto-detects the entry point, or accepts one as an argument
- Detects the app's port and frees it if another process is already listening
- Ignores changes inside `.git`, `vendor`, and `target`
- Works on Unix (macOS/Linux) and Windows

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) (edition 2024, i.e. Rust 1.85+)
- [Go](https://go.dev/dl/) available on your `PATH`
- Unix only: `lsof` (used for port detection/cleanup)

## Installation

```sh
git clone <repo-url>
cd go_watcher
cargo install --path .
```

Or build without installing:

```sh
cargo build --release
# binary: target/release/go_watcher
```

## Usage

Run it from the root of your Go project:

```sh
go_watcher            # auto-detect entry point
go_watcher <path>     # explicit entry point
```

Examples:

```sh
go_watcher                 # runs `go run main.go`
go_watcher ./cmd/api       # runs `go run ./cmd/api`
```

### Entry point resolution

1. The first CLI argument, if given
2. `main.go` in the current directory
3. `cmd/<name>/main.go`, if exactly one exists
4. Otherwise `.` (`go run .`)

If several `cmd/*/main.go` entries exist, the tool exits and asks you to pass one explicitly.

### Port detection

Before every (re)start, go_watcher looks for the port your app listens on:

1. `.env`: the first key ending in `PORT` (e.g. `PORT=8080`, `export APP_PORT=":3000"`)
2. The entry `main.go`: the first string literal that looks like a port (e.g. `":8080"`, `"8080"`, ports 1024 and above)

If a process is already listening on that port, it is killed (`kill -9` on Unix, `taskkill` on Windows) so the new instance can bind. If no port is found, this step is skipped.

> **Warning:** this kills whatever process is listening on the detected port, not just previous runs of your app.

## How it works

1. Starts `go run <entry>` in its own process group
2. Watches `.` recursively using [`notify`](https://crates.io/crates/notify)
3. On a relevant `.go` change, kills the process group, frees the port, and starts `go run` again

## Development

```sh
cargo run              # run against the current directory
cargo run -- ./cmd/api # pass an entry point
```
