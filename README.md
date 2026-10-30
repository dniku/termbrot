# mandelbrot

Renders the Mandelbrot set as block-shade terminal art or a PNG, with no runtime dependencies outside the Rust standard library.

Built and tested entirely through Nix, so `nix test` works on a machine with no
`cargo`, no `rustc`, and no network.

## Usage

```console
$ mandelbrot                                  # whole set, 120x60 blocks
$ mandelbrot -c -0.75,-0.1 -z 40 -w 200       # seahorse valley
$ mandelbrot -0.77,-0.15,-0.73,-0.09 -i 2000  # exact rectangle, more detail
$ mandelbrot -w 1200 -h 800 -o mandelbrot.png # PNG
$ mandelbrot -o - > frame.png                 # PNG to stdout
```

The first positional argument is a rectangle, `real_min,imaginary_min,real_max,imaginary_max`.
Alternatively give a centre and a magnification, which fits the region to the aspect ratio of the output grid:

```console
$ mandelbrot -c -0.745,0.113 -z 250 -w 160 -h 80
```

| Option | Meaning | Default |
| --- | --- | --- |
| `-w`, `--width` | Columns or pixels | 120 |
| `-h`, `--height` | Rows or pixels | 60 |
| `-i`, `--max-iter` | Iteration budget | 300 |
| `-z`, `--zoom` | Magnification factor; larger shows less, in more detail | 1 |
| `-c`, `--center` | Centre the view on `real,imaginary` | view centre |
| `-p`, `--pan` | Shift by a fraction of the view size, without cropping | 0,0 |
| `-o`, `--output` | PNG destination; `-` means stdout | terminal |

## Nix

```console
$ nix build .#mandelbrot   # or just `nix build`
$ nix run . -- -c -0.75,-0.1 -z 40
$ nix test                 # unit tests, inside the sandbox
$ nix flake check
```

The crate has no dependencies outside the standard library, so the build never
contacts crates.io. `buildRustPackage` still requires one of
`cargoHash`/`cargoVendorDir`/`cargoDeps`/`cargoLock`, so the package points
`cargoVendorDir` at a checked-in empty `cargo-vendor/` directory: that states
"nothing to vendor" honestly instead of inventing a hash nobody could check. Note
that `cargoVendorDir` is read relative to the source root, not as a store path.

## How it works

Escape times are computed in **fixed point**, not `f64`. A given
`(real, imaginary, max_iter)` triple therefore yields the same iteration count on
every machine and every build, which is what lets the rendering tests assert
against hand-computed literals rather than golden images.

The scale is 40 fractional bits, chosen by measurement rather than taste. At 30
bits the orbit matches the reference almost everywhere, but a handful of
near-boundary points compound the quantisation error over hundreds of iterations
badly enough to change an escape count — and in one case to report a genuine set
point as escaping. A deterministic 4 000-point sweep against arbitrary-precision
arithmetic is clean at 40, and leaves roughly a millionfold margin before `i64`
overflows. Products still accumulate in `i128`, since the squared terms alone
exceed `i64` at that scale.

| Module | Responsibility |
| --- | --- |
| `src/escape.rs` | Fixed-point iteration and the interior sentinel |
| `src/palette.rs` | Cyclic colour ramp and shading glyphs |
| `src/view.rs` | Complex-plane rectangle, sampling grid, zoom and pan |
| `src/render.rs` | Grid sampling into glyphs or RGB bytes |
| `src/png.rs` | PNG encoder |
| `src/cli.rs` | Argument parsing and dispatch |

`--zoom` and `--pan` are deliberately different operations. Magnifying reframes
the view to the output grid's aspect ratio, which crops the rectangle; panning is
a pure translation. Coupling them, as an earlier revision did, meant a horizontal
`--pan` silently clipped the vertical extent.

Two design choices are worth knowing about:

- **Interior points are `u32::MAX`, not `max_iter`.** Using the budget as the
  marker would make a genuine set point indistinguishable from one that escaped
  on the final permitted iteration.
- **The PNG uses stored DEFLATE blocks**, i.e. no compression. A Huffman encoder
  is a few hundred lines this project does not need, and the output stays
  conformant, so `file`, ImageMagick, and browsers all read it.

Terminal output uses block glyphs rather than braille, because a block fills the
whole cell and so absorbs the aspect-ratio mismatch between cell width and
height.

## Development

```console
$ nix develop
$ cargo test                     # or cargo clippy --all-targets
$ cargo fmt --check
```

Flakes read the **committed** tree, not the working tree: an uncommitted change
is invisible to `nix build`, so commit before validating or you will watch a stale
derivation fail in the same way every time. `cargo check` reads the working tree,
which makes the two disagree confusingly; `git status` should be clean when a Nix
build is meant to reflect your edits.

`rustfmt` and `clippy` run over the sources in CI-style checks:

```console
$ nix build .#check
$ cargo clippy --all-targets -- -D warnings
```
