#![forbid(unsafe_code)]

mod app;

fn main() {
    if let Err(error) = app::run_stdio() {
        eprintln!("cli error: {error}");
        std::process::exit(1);
    }
}
