#![forbid(unsafe_code)]

mod app;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Err(error) = app::run_stdio_with_args(&args) {
        eprintln!("cli error: {error}");
        std::process::exit(1);
    }
}
