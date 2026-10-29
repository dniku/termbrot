//! Escape-time computation for the Mandelbrot set.
//!
//! Escape times are computed in fixed point so that a given `(re, im, max_iter)`
//! triple always produces the same result, on every machine and in every build.
//! That determinism is what lets the render pipeline be asserted against
//! hand-computed literals instead of golden images.

/// Fractional bits used by the fixed-point representation of a coordinate.
const SHIFT: u32 = 30;

/// One unit of the fixed-point representation, i.e. `1.0`.
const ONE: i64 = 1 << SHIFT;

/// Squared-magnitude bailout threshold, `2²`, in fixed point.
///
/// Orbits that leave the disk of radius 2 can never return, so this is the
/// classic bailout radius.
const ESCAPE_RADIUS_SQUARED: i64 = 4 * ONE * ONE;

/// Largest absolute coordinate accepted by [`escape_time`].
const MARGIN: i64 = 4;

/// Number of iterations an escaping point needed, or [`u32::MAX`] for interior
/// points.
///
/// The sentinel is `u32::MAX` rather than `max_iter` so a caller can tell a true
/// set point apart from one that escaped on the final permitted iteration.
///
/// Products are accumulated in `i128` and narrowed back to `i64` after each
/// shift. With coordinates clamped to [`MARGIN`] units the widest intermediate,
/// `2 · z_re · z_im`, peaks at `2 · (2·2³⁰)² = 2⁶⁴`, so an `i64` accumulator
/// would overflow exactly there; `i128` removes the bound rather than leaving
/// correctness resting on how close the worst case runs to it.
///
/// # Panics
///
/// Panics if either coordinate is non-finite or lies more than [`MARGIN`] units
/// outside the set's bounding box. Callers filter with [`is_worth_iterating`]
/// rather than letting [`f64::clamp`] silently relocate bad input.
pub fn escape_time(re: f64, im: f64, max_iter: u32) -> u32 {
    assert!(re.is_finite() && im.is_finite(), "coordinates must be finite");
    let c_re = to_fixed(re);
    let c_im = to_fixed(im);
    assert!(
        c_re.abs() <= MARGIN * ONE && c_im.abs() <= MARGIN * ONE,
        "coordinates out of fixed-point range: ({re}, {im})"
    );

    let mut z_re: i64 = 0;
    let mut z_im: i64 = 0;

    for iteration in 0..max_iter {
        let magnitude_squared = (z_re * z_re >> SHIFT) + (z_im * z_im >> SHIFT);
        if magnitude_squared > ESCAPE_RADIUS_SQUARED {
            return iteration;
        }
        // z = z² + c, narrowing each product back to fixed point.
        let next_re = (i128::from(z_re) * i128::from(z_re)
            - i128::from(z_im) * i128::from(z_im)
            >> SHIFT) as i64
            + c_re;
        let next_im =
            (2 * i128::from(z_re) * i128::from(z_im) >> SHIFT) as i64 + c_im;
        z_re = next_re;
        z_im = next_im;
    }
    u32::MAX
}

/// Whether `re`/`im` is finite and inside the set's bounding box.
///
/// Points outside it escape immediately and need not be iterated.
pub fn is_worth_iterating(re: f64, im: f64) -> bool {
    re.is_finite()
        && im.is_finite()
        && (-2.5..=1.0).contains(&re)
        && (-2.0..=2.0).contains(&im)
}

/// Round a real number into the fixed-point representation.
///
/// Rounding rather than truncating keeps the two ends of a sampled range
/// quantising symmetrically about zero.
fn to_fixed(value: f64) -> i64 {
    (value * ONE as f64).round() as i64
}
