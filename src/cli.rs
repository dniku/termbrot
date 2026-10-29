//! Command-line interface: parsing and dispatch.

use crate::render;
use crate::view::View;
use std::io::{self, Write};
use std::path::PathBuf;

/// Where to send the rendered image.
#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    /// Block-shade art on standard output.
    Terminal,
    /// PNG bytes at this path, or `-` for standard output.
    Png(PathBuf),
}

/// A fully parsed run.
#[derive(Debug, Clone, PartialEq)]
pub struct Arguments {
    pub width: u32,
    pub height: u32,
    pub max_iter: u32,
    pub view: View,
    pub transform: Transform,
    pub output: Output,
}

/// A transform applied to a view before rendering.
///
/// Zoom is expressed as a factor rather than a target width so that
/// `--zoom 2 --zoom 2` composes predictably, and so both spellings of a
/// location (explicit rectangle, or centre plus zoom) share one code path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub zoom: f64,
    pub center: Option<(f64, f64)>,
    pub pan: (f64, f64),
}

impl Default for Arguments {
    fn default() -> Self {
        Self {
            width: 120,
            height: 60,
            max_iter: 300,
            view: View::FULL,
            transform: Transform {
                zoom: 1.0,
                center: None,
                pan: (0.0, 0.0),
            },
            output: Output::Terminal,
        }
    }
}

/// Why a command line could not be interpreted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usage(String);

impl std::fmt::Display for Usage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

pub const USAGE: &str = "\
mandelbrot — render the Mandelbrot set

USAGE:
    mandelbrot [OPTIONS]

OPTIONS:
    -w, --width <N>      Columns or pixels          [default: 120]
    -h, --height <N>     Rows or pixels             [default: 60]
    -i, --max-iter <N>   Iteration budget           [default: 300]
    -o, --output <PATH>  Write a PNG here ('-' for stdout)
    -z, --zoom <F>       Magnification factor       [default: 1]
    -c, --center <R,I>   Centre the view on this point
    -p, --pan <DX,DY>    Shift by a fraction of the view size
        --help           Show this help

RECTANGLE:
    The whole set is rendered by default. Pass a rectangle as the first
    positional argument to render an exact region, as
    real_min,imaginary_min,real_max,imaginary_max:

        mandelbrot -0.77,-0.15,-0.73,-0.09

    Or combine --center and --zoom, which keeps the aspect ratio of the
    output grid and composes with --pan:

        mandelbrot -c -0.75,-0.1 -z 40 -w 200 -h 100
";

/// Parse `small` as a positive integer, with a message naming the option.
fn parse_positive(name: &str, value: &str) -> Result<u32, Usage> {
    let parsed: u32 = value.parse().map_err(|_| {
        Usage(format!("{name} expects a non-negative integer, got {value:?}"))
    })?;
    if parsed == 0 {
        return Err(Usage(format!("{name} must be at least 1")));
    }
    Ok(parsed)
}

/// Parse an iterator value, allowing a generous budget but refusing nonsense.
fn parse_max_iter(value: &str) -> Result<u32, Usage> {
    let parsed = parse_positive("--max-iter", value)?;
    // The u32::MAX sentinel marks interior points, so it is not usable here.
    const LIMIT: u32 = 100_000_000;
    if parsed >= LIMIT {
        return Err(Usage(format!(
            "--max-iter must be below {LIMIT}, got {value}"
        )));
    }
    Ok(parsed)
}

/// Parse a comma-separated pair of finite numbers.
///
/// Shared by `--center` and `--pan` so both reject the same malformed shapes.
fn parse_pair(name: &str, value: &str) -> Result<(f64, f64), Usage> {
    let parts: Vec<&str> = value.split(',').map(str::trim).collect();
    if parts.len() != 2 {
        return Err(Usage(format!(
            "{name} expects TWO,COMMA,SEPARATED numbers, got {value:?}"
        )));
    }
    let number = |text: &str| -> Result<f64, Usage> {
        text.parse::<f64>()
            .ok()
            .filter(|parsed| parsed.is_finite())
            .ok_or_else(|| Usage(format!("{name}: {text:?} is not a finite number")))
    };
    Ok((number(parts[0])?, number(parts[1])?))
}

/// Parse an argument list, excluding the program name.
///
/// # Errors
///
/// Returns a [`Usage`] describing the first problem, so the caller can print
/// the message together with the help text.
pub fn parse(args: &[String]) -> Result<Option<Arguments>, Usage> {
    let mut arguments = Arguments::default();
    let mut positionals: Vec<String> = Vec::new();
    let mut index = 0;

    // Consume a flag's value, supporting both `--width 10` and `--width=10`.
    let take_value = |index: &mut usize,
                          flag: &str,
                          inline: Option<&str>|
     -> Result<String, Usage> {
        if let Some(value) = inline {
            return Ok(value.to_string());
        }
        *index += 1;
        args.get(*index).cloned().ok_or_else(|| {
            Usage(format!("{flag} expects a value"))
        })
    };

    while index < args.len() {
        let raw = &args[index];
        let (flag, inline) = match raw.split_once('=') {
            Some((flag, value)) => (flag, Some(value)),
            None => (raw.as_str(), None),
        };
        match flag {
            "--help" => return Ok(None),
            "-w" | "--width" => {
                let value = take_value(&mut index, flag, inline)?;
                arguments.width = parse_positive(flag, &value)?;
            }
            "-h" | "--height" => {
                let value = take_value(&mut index, flag, inline)?;
                arguments.height = parse_positive(flag, &value)?;
            }
            "-i" | "--max-iter" => {
                let value = take_value(&mut index, flag, inline)?;
                arguments.max_iter = parse_max_iter(&value)?;
            }
            "-z" | "--zoom" => {
                let value = take_value(&mut index, flag, inline)?;
                let zoom: f64 = value.parse().map_err(|_| {
                    Usage(format!("{flag} expects a number, got {value:?}"))
                })?;
                if !(zoom.is_finite() && zoom > 0.0) {
                    return Err(Usage(format!(
                        "{flag} must be a positive finite number, got {value}"
                    )));
                }
                arguments.transform.zoom = zoom;
            }
            "-c" | "--center" => {
                let value = take_value(&mut index, flag, inline)?;
                arguments.transform.center = Some(parse_pair(flag, &value)?);
            }
            "-p" | "--pan" => {
                let value = take_value(&mut index, flag, inline)?;
                arguments.transform.pan = parse_pair(flag, &value)?;
            }
            "-o" | "--output" => {
                let value = take_value(&mut index, flag, inline)?;
                arguments.output = Output::Png(PathBuf::from(value));
            }
            other => {
                if other.starts_with('-') && other != "-" {
                    return Err(Usage(format!("unknown option {other}")));
                }
                positionals.push(raw.clone());
            }
        }
        index += 1;
    }

    match positionals.as_slice() {
        [] => {}
        [spec] => arguments.view = View::parse(spec).map_err(Usage)?,
        extra => {
            return Err(Usage(format!(
                "expected at most one rectangle, got {}",
                extra.len()
            )));
        }
    }

    // An explicit rectangle is taken literally; --zoom is then applied about
    // its centre, so `-c`/`-z` and a positional rectangle compose rather than
    // fight. Aspect comes from the output grid, which is what stops a taller
    // terminal from stretching the set.
    let aspect = f64::from(arguments.width) / f64::from(arguments.height);
    let view = arguments.view;
    // Without --center, magnify about the middle of whatever rectangle we have,
    // so an explicit rectangle and a --zoom compose instead of fighting.
    let center = arguments.transform.center.unwrap_or((
        (view.real_min + view.real_max) / 2.0,
        (view.imaginary_min + view.imaginary_max) / 2.0,
    ));
    arguments.view = view
        .zoomed(arguments.transform.zoom, center, aspect)
        .panned(arguments.transform.pan.0, arguments.transform.pan.1);

    Ok(Some(arguments))
}

/// Run the program, returning the process exit status.
///
/// # Errors
///
/// Propagates write failures; usage problems are reported and yield `2`.
pub fn run(args: &[String], stdout: &mut impl Write) -> Result<i32, io::Error> {
    let arguments = match parse(args) {
        Ok(Some(arguments)) => arguments,
        Ok(None) => {
            stdout.write_all(USAGE.as_bytes())?;
            return Ok(0);
        }
        Err(problem) => {
            let mut stderr = io::stderr();
            writeln!(stderr, "error: {problem}\n")?;
            stdout.write_all(USAGE.as_bytes())?;
            return Ok(2);
        }
    };

    match arguments.output {
        Output::Terminal => {
            let rendered = render::text(
                &arguments.view,
                arguments.width,
                arguments.height,
                arguments.max_iter,
            );
            writeln!(stdout, "{rendered}")?;
        }
        Output::Png(ref path) => {
            let pixels = render::pixels(
                &arguments.view,
                arguments.width,
                arguments.height,
                arguments.max_iter,
            );
            if path.to_str() == Some("-") {
                crate::png::encode(stdout, arguments.width, arguments.height, &pixels)?;
            } else {
                let mut file = std::fs::File::create(path)?;
                crate::png::encode(&mut file, arguments.width, arguments.height, &pixels)?;
                writeln!(stdout, "wrote {}", path.display())?;
            }
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn defaults_render_the_whole_set_to_the_terminal() {
        let parsed = parse(&[]).expect("valid").expect("not help");
        assert_eq!(parsed.width, 120);
        assert_eq!(parsed.height, 60);
        assert_eq!(parsed.view, View::FULL);
        assert_eq!(parsed.output, Output::Terminal);
    }

    #[test]
    fn flags_accept_both_separate_and_inline_values() {
        let separate = parse(&args(&["-w", "80", "--height", "40"]))
            .expect("valid")
            .expect("not help");
        let inline = parse(&args(&["--width=80", "-h=40"]))
            .expect("valid")
            .expect("not help");
        assert_eq!(separate, inline);
        assert_eq!(separate.width, 80);
        assert_eq!(separate.height, 40);
    }

    #[test]
    fn help_short_circuits_to_none() {
        assert!(parse(&args(&["--help"])).expect("valid").is_none());
    }

    #[test]
    fn positional_argument_sets_the_view() {
        let parsed = parse(&args(&["-0.77,-0.15,-0.73,-0.09"]))
            .expect("valid")
            .expect("not help");
        assert_eq!(parsed.view.real_min, -0.77);
        assert_eq!(parsed.view.imaginary_max, -0.09);
    }

    #[test]
    fn output_destination_is_recorded() {
        let parsed = parse(&args(&["-o", "out.png"])).expect("valid").expect("not help");
        assert_eq!(parsed.output, Output::Png(PathBuf::from("out.png")));
        let stdout = parse(&args(&["--output=-"])).expect("valid").expect("not help");
        assert_eq!(stdout.output, Output::Png(PathBuf::from("-")));
    }

    #[test]
    fn bad_input_is_rejected_with_a_message() {
        for bad in [
            args(&["-w", "0"]),
            args(&["-w", "-5"]),
            args(&["-w", "abc"]),
            args(&["--max-iter", "0"]),
            args(&["-o"]),
            args(&["--width"]),
            args(&["--nope"]),
            args(&["0", "1"]),
            args(&["not-a-rectangle"]),
            args(&["0,0,1"]),
        ] {
            let problem = parse(&bad).expect_err("should fail");
            assert!(!problem.0.is_empty(), "{bad:?} produced no message");
        }
    }

    #[test]
    fn max_iter_limit_rejects_the_interior_sentinel() {
        // 4294967295 is u32::MAX, which escape_time reserves for set points.
        assert!(parse(&args(&["-i", "4294967295"])).is_err());
        assert!(parse(&args(&["-i", "1000"])).is_ok());
    }

    #[test]
    fn zoom_shrinks_the_view_about_its_centre() {
        let parsed = parse(&args(&["-z", "2"])).expect("valid").expect("not help");
        let full = View::FULL;
        assert!(
            (parsed.view.width() - full.width() / 2.0).abs() < 1e-12,
            "zoom 2 should halve the width, got {}",
            parsed.view.width()
        );
        // Centred on the original midpoint, which is not the origin.
        let center_real = (parsed.view.real_min + parsed.view.real_max) / 2.0;
        let original_center = (full.real_min + full.real_max) / 2.0;
        assert!((center_real - original_center).abs() < 1e-12);
    }

    #[test]
    fn zoom_honours_an_explicit_centre() {
        let parsed = parse(&args(&["-c", "0,0", "-z", "10"]))
            .expect("valid")
            .expect("not help");
        assert!(parsed.view.real_min.abs() < 1e-9 && parsed.view.real_max.abs() < 1e-9);
        assert!(parsed.view.imaginary_min.abs() < 1e-9);
    }

    #[test]
    fn zoom_preserves_the_grid_aspect() {
        let parsed = parse(&args(&["-w", "200", "-h", "100", "-z", "3"]))
            .expect("valid")
            .expect("not help");
        let aspect = parsed.view.width() / parsed.view.height();
        assert!(
            (aspect - 2.0).abs() < 1e-9,
            "expected grid aspect 2.0, got {aspect}"
        );
    }

    #[test]
    fn pan_shifts_the_view() {
        let parsed = parse(&args(&["-p", "0.25,0.0"]))
            .expect("valid")
            .expect("not help");
        assert!(parsed.view.real_min > View::FULL.real_min);
        assert!(
            (parsed.view.imaginary_min - View::FULL.imaginary_min).abs() < 1e-12,
            "vertical pan was requested as zero"
        );
    }

    #[test]
    fn rectangle_and_zoom_compose() {
        // A rectangle plus a magnification about its own centre.
        let parsed = parse(&args(&["-0.8,-0.2,-0.6,0.0", "-z", "2"]))
            .expect("valid")
            .expect("not help");
        assert!((parsed.view.width() - 0.1).abs() < 1e-12);
        assert!((parsed.view.real_min + 0.75).abs() < 1e-12);
    }

    #[test]
    fn bad_transforms_are_rejected() {
        for bad in [
            args(&["-z", "0"]),
            args(&["-z", "-1"]),
            args(&["-z", "nan"]),
            args(&["-z", "inf"]),
            args(&["-c", "1"]),
            args(&["-c", "1,2,3"]),
            args(&["-c", "nan,1"]),
            args(&["-p", "x,y"]),
        ] {
            assert!(parse(&bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn run_writes_a_picture_to_stdout() {
        let mut out = Vec::new();
        let status = run(&args(&["-w", "20", "-h", "5", "-i", "30"]), &mut out)
            .expect("io");
        assert_eq!(status, 0);
        let text = String::from_utf8(out).expect("utf-8");
        assert_eq!(text.lines().count(), 5);
    }

    #[test]
    fn run_reports_usage_errors_with_status_two() {
        let mut out = Vec::new();
        let status = run(&args(&["-w", "0"]), &mut out).expect("io");
        assert_eq!(status, 2);
        assert!(String::from_utf8_lossy(&out).contains("USAGE"));
    }
}
