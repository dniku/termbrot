//! Turn a [`View`] into escape times, glyphs, or pixels.

use crate::escape::{escape_time, is_worth_iterating};
use crate::palette;
use crate::view::View;

/// Escape time for the pixel at `column`/`row` of a grid sampling `view`.
///
/// Points outside the set's bounding box short-circuit to zero iterations,
/// which keeps a large canvas centred on empty space cheap.
#[must_use]
pub fn sample(view: &View, column: u32, row: u32, width: u32, height: u32, max_iter: u32) -> u32 {
    let re = view.real_at(column, width);
    let im = view.imaginary_at(row, height);
    if is_worth_iterating(re, im) {
        escape_time(re, im, max_iter)
    } else {
        0
    }
}

/// Render `view` to a vector of RGB bytes, three per pixel, row-major.
#[must_use]
pub fn pixels(view: &View, width: u32, height: u32, max_iter: u32) -> Vec<u8> {
    let mut buffer = Vec::with_capacity(width as usize * height as usize * 3);
    for row in 0..height {
        for column in 0..width {
            let time = sample(view, column, row, width, height, max_iter);
            buffer.extend_from_slice(&palette::color(time, max_iter));
        }
    }
    buffer
}

/// Render `view` to terminal text: one line per row, joined by newlines.
#[must_use]
pub fn text(view: &View, width: u32, height: u32, max_iter: u32) -> String {
    let mut out = String::with_capacity((width as usize + 1) * height as usize);
    for row in 0..height {
        for column in 0..width {
            let time = sample(view, column, row, width, height, max_iter);
            out.push(palette::glyph(time, max_iter));
        }
        if row + 1 < height {
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
use crate::escape;

    #[test]
    fn known_points_classify_correctly() {
        // Real axis: [-2, 1/4] lies in the set.
        assert_eq!(escape_time(0.0, 0.0, 100), escape::INTERIOR);
        assert_eq!(escape_time(-1.0, 0.0, 200), escape::INTERIOR);
        assert_eq!(escape_time(-2.0, 0.0, 200), escape::INTERIOR);
        // 1+i leaves the bailout disk as soon as |z| exceeds 2, which the
        // reference and the fixed-point loop agree is iteration 1.
        assert_eq!(escape_time(1.0, 1.0, 100), 1);
        assert_ne!(escape_time(0.5, 0.0, 200), escape::INTERIOR);
    }

    #[test]
    fn full_view_contains_a_plausible_amount_of_set() {
        // Count interior pixels directly: the set should cover a small but
        // clearly nonzero share of its own frame. Anything near 0 or 1 means
        // the geometry or the interior sentinel is wrong.
        let buffer = pixels(&View::FULL, 120, 80, 300);
        let interior = buffer
            .chunks(3)
            .filter(|pixel| *pixel == [0, 0, 0])
            .count();
        let fraction = interior as f64 / (120 * 80) as f64;
        assert!(
            (0.02..0.20).contains(&fraction),
            "interior fraction {fraction} outside plausible range"
        );
    }

    #[test]
    fn rendering_is_deterministic() {
        let first = pixels(&View::FULL, 40, 30, 100);
        let second = pixels(&View::FULL, 40, 30, 100);
        assert_eq!(first, second, "same inputs must give identical bytes");
    }

    #[test]
    fn pixel_buffer_has_the_right_length() {
        let buffer = pixels(&View::FULL, 13, 7, 50);
        assert_eq!(buffer.len(), 13 * 7 * 3);
    }

    #[test]
    fn text_output_has_one_line_per_row() {
        let rendered = text(&View::FULL, 10, 4, 50);
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines.len(), 4);
        for line in &lines {
            assert_eq!(line.chars().count(), 10);
        }
    }

    #[test]
    fn zooming_into_seahorse_valley_shows_structure() {
        // A zoom into a known busy region must still find set pixels; a bug in
        // the row flip or aspect handling tends to produce a blank frame here.
        let view = View {
            real_min: -0.75 - 0.02,
            real_max: -0.75 + 0.02,
            imaginary_min: -0.1 - 0.01,
            imaginary_max: -0.1 + 0.01,
        };
        let buffer = pixels(&view, 80, 60, 800);
        let interior = buffer.chunks(3).filter(|p| *p == [0, 0, 0]).count();
        assert!(interior > 0, "expected structure in the seahorse valley");
    }

    #[test]
    fn points_outside_the_frame_are_not_iterated() {
        let view = View {
            real_min: 5.0,
            real_max: 6.0,
            imaginary_min: 5.0,
            imaginary_max: 6.0,
        };
        let buffer = pixels(&view, 8, 8, 100);
        // Everything escapes immediately, so nothing may render as interior.
        assert!(buffer.chunks(3).all(|pixel| *pixel != [0, 0, 0]));
    }
}
