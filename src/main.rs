//! mandelbrot — render the Mandelbrot set as terminal art or a PNG.

mod cli;
mod escape;
mod palette;
mod png;
mod render;
mod view;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let status = cli::run(&args, &mut handle).unwrap_or_else(|error| {
        eprintln!("error: {error}");
        1
    });
    if status != 0 {
        std::process::exit(status);
    }
}
