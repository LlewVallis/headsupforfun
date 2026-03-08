#![forbid(unsafe_code)]

use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};

type DynError = Box<dyn Error>;

const FAST_TIMEOUT_SECS: u64 = 60;
const SLOW_TIMEOUT_SECS: u64 = 300;
const SOLVER_SLOW_TIMEOUT_SECS: u64 = 300;
const WASM_TIMEOUT_SECS: u64 = 120;
const TRAIN_SMOKE_TIMEOUT_SECS: u64 = 60;
const TRAIN_DEV_TIMEOUT_SECS: u64 = 600;
const BENCH_SMOKE_TIMEOUT_SECS: u64 = 120;
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const WASM_TARGET: &str = "wasm32-unknown-unknown";

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), DynError> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        print_help();
        return Ok(());
    };

    let timeout = parse_timeout_secs(args.collect())?;
    let workspace_root = workspace_root()?;

    match command.as_str() {
        "test-fast" => run_cargo(
            &workspace_root,
            &["test", "--workspace"],
            timeout.unwrap_or(FAST_TIMEOUT_SECS),
        ),
        "test-slow" => run_cargo(
            &workspace_root,
            &["test", "--workspace", "--", "--ignored"],
            timeout.unwrap_or(SLOW_TIMEOUT_SECS),
        ),
        "test-solver-slow" => run_cargo(
            &workspace_root,
            &["test", "-p", "gto-solver", "--", "--ignored"],
            timeout.unwrap_or(SOLVER_SLOW_TIMEOUT_SECS),
        ),
        "check-wasm" => {
            ensure_wasm_target_installed(&workspace_root)?;
            run_cargo(
                &workspace_root,
                &[
                    "check",
                    "-p",
                    "gto-core",
                    "-p",
                    "gto-solver",
                    "--target",
                    WASM_TARGET,
                ],
                timeout.unwrap_or(WASM_TIMEOUT_SECS),
            )
        }
        "check-all" => {
            run_cargo(
                &workspace_root,
                &["test", "--workspace"],
                timeout.unwrap_or(FAST_TIMEOUT_SECS),
            )?;
            ensure_wasm_target_installed(&workspace_root)?;
            run_cargo(
                &workspace_root,
                &[
                    "check",
                    "-p",
                    "gto-core",
                    "-p",
                    "gto-solver",
                    "--target",
                    WASM_TARGET,
                ],
                timeout.unwrap_or(WASM_TIMEOUT_SECS),
            )
        }
        "train-smoke" => run_cargo(
            &workspace_root,
            &["run", "-p", "gto-cli", "--", "train-river-demo", "--profile", "smoke"],
            timeout.unwrap_or(TRAIN_SMOKE_TIMEOUT_SECS),
        )
        .and_then(|_| {
            run_cargo(
                &workspace_root,
                &["run", "-p", "gto-cli", "--", "train-turn-demo", "--profile", "smoke"],
                timeout.unwrap_or(TRAIN_SMOKE_TIMEOUT_SECS),
            )
        })
        .and_then(|_| {
            run_cargo(
                &workspace_root,
                &["run", "-p", "gto-cli", "--", "train-flop-demo", "--profile", "smoke"],
                timeout.unwrap_or(TRAIN_SMOKE_TIMEOUT_SECS),
            )
        }),
        "train-dev" => run_cargo(
            &workspace_root,
            &["run", "-p", "gto-cli", "--", "train-river-demo", "--profile", "dev"],
            timeout.unwrap_or(TRAIN_DEV_TIMEOUT_SECS),
        )
        .and_then(|_| {
            run_cargo(
                &workspace_root,
                &["run", "-p", "gto-cli", "--", "train-turn-demo", "--profile", "dev"],
                timeout.unwrap_or(TRAIN_DEV_TIMEOUT_SECS),
            )
        })
        .and_then(|_| {
            run_cargo(
                &workspace_root,
                &["run", "-p", "gto-cli", "--", "train-flop-demo", "--profile", "dev"],
                timeout.unwrap_or(TRAIN_DEV_TIMEOUT_SECS),
            )
        }),
        "train-river-smoke" => run_cargo(
            &workspace_root,
            &["run", "-p", "gto-cli", "--", "train-river-demo", "--profile", "smoke"],
            timeout.unwrap_or(TRAIN_SMOKE_TIMEOUT_SECS),
        ),
        "train-turn-smoke" => run_cargo(
            &workspace_root,
            &["run", "-p", "gto-cli", "--", "train-turn-demo", "--profile", "smoke"],
            timeout.unwrap_or(TRAIN_SMOKE_TIMEOUT_SECS),
        ),
        "train-flop-smoke" => run_cargo(
            &workspace_root,
            &["run", "-p", "gto-cli", "--", "train-flop-demo", "--profile", "smoke"],
            timeout.unwrap_or(TRAIN_SMOKE_TIMEOUT_SECS),
        ),
        "bench-smoke" => run_cargo(
            &workspace_root,
            &[
                "bench",
                "-p",
                "gto-core",
                "--bench",
                "hand_eval_smoke",
                "--",
                "--sample-size",
                "10",
                "--warm-up-time",
                "0.05",
                "--measurement-time",
                "0.05",
            ],
            timeout.unwrap_or(BENCH_SMOKE_TIMEOUT_SECS),
        )
        .and_then(|_| {
            run_cargo(
                &workspace_root,
                &[
                    "bench",
                    "-p",
                    "gto-solver",
                    "--bench",
                    "blueprint_smoke",
                    "--",
                    "--sample-size",
                    "10",
                    "--warm-up-time",
                    "0.05",
                    "--measurement-time",
                    "0.05",
                ],
                timeout.unwrap_or(BENCH_SMOKE_TIMEOUT_SECS),
            )
        }),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(Box::new(XtaskError::new(format!(
            "unknown xtask command `{other}`"
        )))),
    }
}

fn parse_timeout_secs(arguments: Vec<String>) -> Result<Option<u64>, DynError> {
    if arguments.is_empty() {
        return Ok(None);
    }

    if arguments.len() != 2 || arguments[0] != "--timeout-secs" {
        return Err(Box::new(XtaskError::new(
            "expected optional arguments in the form `--timeout-secs <seconds>`",
        )));
    }

    let seconds = arguments[1]
        .parse::<u64>()
        .map_err(|_| XtaskError::new("timeout value must be a positive integer"))?;

    if seconds == 0 {
        return Err(Box::new(XtaskError::new(
            "timeout value must be greater than zero",
        )));
    }

    Ok(Some(seconds))
}

fn workspace_root() -> Result<PathBuf, DynError> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().map(Path::to_path_buf).ok_or_else(|| {
        Box::new(XtaskError::new("could not determine workspace root")) as DynError
    })
}

fn ensure_wasm_target_installed(workspace_root: &Path) -> Result<(), DynError> {
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .current_dir(workspace_root)
        .output()?;

    if !output.status.success() {
        return Err(Box::new(XtaskError::new(
            "failed to query installed rust targets",
        )));
    }

    let stdout = String::from_utf8(output.stdout)?;
    if stdout.lines().any(|line| line.trim() == WASM_TARGET) {
        return Ok(());
    }

    Err(Box::new(XtaskError::new(format!(
        "required target `{WASM_TARGET}` is not installed; run `rustup target add {WASM_TARGET}`"
    ))))
}

fn run_cargo(workspace_root: &Path, cargo_args: &[&str], timeout_secs: u64) -> Result<(), DynError> {
    let mut command = Command::new("cargo");
    command.args(cargo_args).current_dir(workspace_root);

    run_command(command, timeout_secs).map(|_| ())
}

fn run_command(mut command: Command, timeout_secs: u64) -> Result<ExitStatus, DynError> {
    let program = command.get_program().to_os_string();
    let arguments: Vec<OsString> = command.get_args().map(OsString::from).collect();
    let timeout = Duration::from_secs(timeout_secs);
    let started_at = Instant::now();
    let mut child = command.spawn()?;

    loop {
        if let Some(status) = child.try_wait()? {
            if status.success() {
                return Ok(status);
            }

            return Err(Box::new(XtaskError::new(format!(
                "command `{}` exited with status {status}",
                format_command(&program, &arguments),
            ))));
        }

        if started_at.elapsed() >= timeout {
            child.kill()?;
            let _ = child.wait();
            return Err(Box::new(XtaskError::new(format!(
                "command `{}` timed out after {}s",
                format_command(&program, &arguments),
                timeout_secs,
            ))));
        }

        thread::sleep(POLL_INTERVAL);
    }
}

fn format_command(program: &OsString, arguments: &[OsString]) -> String {
    let mut parts = Vec::with_capacity(arguments.len() + 1);
    parts.push(program.to_string_lossy().into_owned());
    parts.extend(arguments.iter().map(|arg| arg.to_string_lossy().into_owned()));
    parts.join(" ")
}

fn print_help() {
    println!(
        "\
Usage:
  cargo xtask test-fast [--timeout-secs <seconds>]
  cargo xtask test-slow [--timeout-secs <seconds>]
  cargo xtask test-solver-slow [--timeout-secs <seconds>]
  cargo xtask check-wasm [--timeout-secs <seconds>]
  cargo xtask check-all [--timeout-secs <seconds>]
  cargo xtask train-smoke [--timeout-secs <seconds>]
  cargo xtask train-dev [--timeout-secs <seconds>]
  cargo xtask train-river-smoke [--timeout-secs <seconds>]
  cargo xtask train-turn-smoke [--timeout-secs <seconds>]
  cargo xtask train-flop-smoke [--timeout-secs <seconds>]
  cargo xtask bench-smoke [--timeout-secs <seconds>]

Commands:
  test-fast   Run the fast workspace test suite.
  test-slow   Run ignored tests intended for opt-in slow coverage.
  test-solver-slow Run ignored tests for gto-solver only.
  check-wasm  Compile-check gto-core and gto-solver for wasm32-unknown-unknown.
  check-all   Run test-fast and check-wasm in sequence.
  train-smoke Train the bundled river, turn, and flop demo artifacts with smoke profiles.
  train-dev   Train the bundled river, turn, and flop demo artifacts with dev profiles.
  train-river-smoke Train only the bundled river demo artifact with the smoke profile.
  train-turn-smoke  Train only the bundled turn demo artifact with the smoke profile.
  train-flop-smoke  Train only the bundled flop demo artifact with the smoke profile.
  bench-smoke Run small criterion benchmark slices for evaluator and blueprint lookup hot paths.
"
    );
}

#[derive(Debug)]
struct XtaskError {
    message: String,
}

impl XtaskError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for XtaskError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for XtaskError {}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{format_command, parse_timeout_secs, workspace_root};

    #[test]
    fn parse_timeout_secs_accepts_empty_arguments() {
        assert_eq!(parse_timeout_secs(Vec::new()).unwrap(), None);
    }

    #[test]
    fn parse_timeout_secs_accepts_valid_override() {
        assert_eq!(
            parse_timeout_secs(vec!["--timeout-secs".into(), "15".into()]).unwrap(),
            Some(15)
        );
    }

    #[test]
    fn parse_timeout_secs_rejects_zero_and_bad_shapes() {
        assert_eq!(
            parse_timeout_secs(vec!["--timeout-secs".into(), "0".into()])
                .unwrap_err()
                .to_string(),
            "timeout value must be greater than zero"
        );
        assert_eq!(
            parse_timeout_secs(vec!["--bad".into(), "10".into()])
                .unwrap_err()
                .to_string(),
            "expected optional arguments in the form `--timeout-secs <seconds>`"
        );
        assert_eq!(
            parse_timeout_secs(vec!["--timeout-secs".into(), "abc".into()])
                .unwrap_err()
                .to_string(),
            "timeout value must be a positive integer"
        );
    }

    #[test]
    fn format_command_renders_program_and_arguments() {
        let rendered = format_command(
            &OsString::from("cargo"),
            &[OsString::from("test"), OsString::from("--workspace")],
        );
        assert_eq!(rendered, "cargo test --workspace");
    }

    #[test]
    fn workspace_root_points_at_the_repository_root() {
        let root = workspace_root().unwrap();
        assert!(root.join("Cargo.toml").exists());
        assert!(root.join("PLAN.md").exists());
    }
}
