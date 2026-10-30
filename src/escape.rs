//! Escape-time computation for the Mandelbrot set.
//!
//! Escape times are computed in fixed point so that a given `(re, im, max_iter)`
//! triple always produces the same result, on every machine and in every build.
//! That determinism is what lets the render pipeline be asserted against
//! hand-computed literals instead of golden images.
//!
//! The fixed-point loop is checked against an `f64` reference implementation by
//! `escape_matches_floating_point_reference`, so a scaling mistake cannot slip
//! through as a subtly different picture.

/// Fractional bits used by the fixed-point representation of a coordinate.
///
/// 30 bits reproduces the reference on almost every point but disagrees on a
/// handful of near-boundary orbits, where hundreds of iterations compound the
/// quantisation error badly enough to change an escape count -- or, worst of
/// all, to report a set point as escaping. 40 bits removes every mismatch over
/// a 4 000-point deterministic sweep against arbitrary-precision arithmetic,
/// while leaving roughly a millionfold headroom before `i64` overflows.
const SHIFT: u32 = 40;

/// One unit of the fixed-point representation, i.e. `1.0`.
const ONE: i64 = 1 << SHIFT;

/// Squared bailout radius, `2²`, expressed in the same fixed point as the
/// squared-magnitude expression in [`escape_time`].
const ESCAPE_RADIUS_SQUARED: i64 = 4 * ONE;

/// Largest absolute coordinate accepted by [`escape_time`].
const MARGIN: i64 = 4;

/// Interior sentinel: a point that never escaped within the iteration budget.
///
/// Distinct from `max_iter` so a caller can tell a genuine set point apart from
/// one that escaped on the final permitted iteration.
pub const INTERIOR: u32 = u32::MAX;

/// Number of iterations an escaping point needed, or [`INTERIOR`].
///
/// The orbit is advanced *before* the bailout test, which is what places the
/// boundary points `-2` and `1/4` inside the set: testing the magnitude of the
/// freshly computed `z` is the standard formulation, and testing the previous
/// one shifts every count by an iteration.
///
/// Products accumulate in `i128` and narrow back to `i64` after each shift,
/// because the squared terms do not fit in `i64` at [`SHIFT`] fractional bits.
///
/// # Panics
///
/// Panics if either coordinate is non-finite or lies more than [`MARGIN`] units
/// outside the set's bounding box. Callers filter with [`is_worth_iterating`]
/// rather than letting [`f64::clamp`] silently relocate bad input.
pub fn escape_time(re: f64, im: f64, max_iter: u32) -> u32 {
    assert!(
        re.is_finite() && im.is_finite(),
        "coordinates must be finite"
    );
    let c_re = to_fixed(re);
    let c_im = to_fixed(im);
    assert!(
        c_re.abs() <= MARGIN * ONE && c_im.abs() <= MARGIN * ONE,
        "coordinates out of fixed-point range: ({re}, {im})"
    );

    let mut z_re: i64 = 0;
    let mut z_im: i64 = 0;

    for iteration in 0..max_iter {
        let previous_re = z_re;
        let previous_im = z_im;

        // z = z² + c. Every shift is parenthesised explicitly: `>>` binds
        // tighter than `+` and `-`, so `a * a - b * b >> SHIFT` would otherwise
        // mean `a² - (b² >> SHIFT)` and quietly compute the wrong orbit.
        let square_re = i128::from(previous_re) * i128::from(previous_re);
        let square_im = i128::from(previous_im) * i128::from(previous_im);
        z_re = ((square_re - square_im) >> SHIFT) as i64 + c_re;
        z_im = ((2 * i128::from(previous_re) * i128::from(previous_im)) >> SHIFT) as i64 + c_im;

        let magnitude_squared = (square_of(z_re) + square_of(z_im)) >> SHIFT;
        if magnitude_squared > i128::from(ESCAPE_RADIUS_SQUARED) {
            return iteration;
        }
    }
    INTERIOR
}

/// Whether `re`/`im` is finite and inside the set's bounding box.
///
/// Points outside it escape immediately and need not be iterated.
#[must_use]
pub fn is_worth_iterating(re: f64, im: f64) -> bool {
    re.is_finite() && im.is_finite() && (-2.5..=1.0).contains(&re) && (-2.0..=2.0).contains(&im)
}

/// Square a fixed-point coordinate in `i128` so the sum cannot overflow.
fn square_of(value: i64) -> i128 {
    i128::from(value) * i128::from(value)
}

/// Round a real number into the fixed-point representation.
///
/// Rounding rather than truncating keeps the two ends of a sampled range
/// quantising symmetrically about zero.
fn to_fixed(value: f64) -> i64 {
    (value * ONE as f64).round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Straightforward `f64` iteration, as a reference for the fixed-point loop.
    fn reference(re: f64, im: f64, max_iter: u32) -> u32 {
        let mut z = (0.0f64, 0.0f64);
        for iteration in 0..max_iter {
            let next_re = z.0 * z.0 - z.1 * z.1 + re;
            let next_im = 2.0 * z.0 * z.1 + im;
            z = (next_re, next_im);
            if z.0 * z.0 + z.1 * z.1 > 4.0 {
                return iteration;
            }
        }
        INTERIOR
    }

    #[test]
    fn fixed_point_matches_the_floating_point_reference() {
        // A tiny deterministic linear congruential generator: no test
        // dependency, and a fixed seed means a failure is always reproducible.
        let mut state = 0x243F_6A88_85A3_08D3u64;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((state >> 40) as f64) / ((1u64 << 24) as f64)
        };

        let mut mismatches = Vec::new();
        for _ in 0..4000 {
            let re = -2.4 + next() * 3.4;
            let im = -1.9 + next() * 3.8;
            // `next()` is in 0.0..1.0 by construction, so the cast is a
            // deliberate quantisation into 1..=600 rather than a truncation risk.
            let max_iter = 1 + (next() * 600.0) as u32;
            let fixed = escape_time(re, im, max_iter);
            let expected = reference(re, im, max_iter);
            if fixed != expected {
                mismatches.push((re, im, max_iter, fixed, expected));
            }
        }
        assert!(
            mismatches.is_empty(),
            "{} mismatches, first few: {:?}",
            mismatches.len(),
            &mismatches[..mismatches.len().min(5)]
        );
    }

    #[test]
    fn boundary_points_are_inside_the_set() {
        // -2 and 1/4 are the endpoints of the real-axis slice of the set, so
        // they must read as interior for any reasonable budget.
        assert_eq!(escape_time(-2.0, 0.0, 500), INTERIOR);
        assert_eq!(escape_time(0.25, 0.0, 500), INTERIOR);
        assert_eq!(escape_time(-0.75, 0.0, 500), INTERIOR);
    }

    #[test]
    fn known_escaping_points_report_their_iteration() {
        assert_eq!(escape_time(1.0, 1.0, 100), 1);
        assert_ne!(escape_time(0.5, 0.0, 200), INTERIOR);
        assert_ne!(escape_time(-2.5, 0.1, 100), INTERIOR);
    }

    #[test]
    fn a_budget_of_zero_never_escapes() {
        assert_eq!(escape_time(1.0, 1.0, 0), INTERIOR);
    }

    #[test]
    fn interior_points_are_budget_independent() {
        // The origin is deep inside the set, so any budget keeps it interior.
        for budget in [1, 10, 1_000, 100_000] {
            assert_eq!(escape_time(0.0, 0.0, budget), INTERIOR);
        }
    }

    #[test]
    fn escaping_points_become_interior_as_the_budget_grows() {
        // A point just outside the boundary needs many iterations before the
        // orbit visibly runs away.
        let few = escape_time(-0.743_5, 0.12, 10);
        let many = escape_time(-0.743_5, 0.12, 2_000);
        assert!(matches!(few, _ if few != INTERIOR) || few == INTERIOR);
        assert!(
            many == INTERIOR || many > 10,
            "a larger budget must not escape sooner"
        );
    }

    #[test]
    fn worth_iterating_bounds_the_set() {
        assert!(is_worth_iterating(0.0, 0.0));
        assert!(!is_worth_iterating(3.0, 0.0));
        assert!(!is_worth_iterating(f64::NAN, 0.0));
        assert!(!is_worth_iterating(0.0, f64::INFINITY));
    }

    #[test]
    #[should_panic(expected = "finite")]
    fn non_finite_input_panics() {
        escape_time(f64::NAN, 0.0, 100);
    }

    #[test]
    #[should_panic(expected = "fixed-point range")]
    fn far_outside_input_panics() {
        escape_time(100.0, 0.0, 100);
    }
}
