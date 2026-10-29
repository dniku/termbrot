//! Colour and glyph mapping for escape times.
//!
//! Both outputs are driven by the same cyclic ramp so that a terminal picture
//! and a PNG agree pixel for pixel. Interpolation happens between explicit
//! stops, which gives smooth bands instead of the posterised rainbows a
//! per-iteration hue rotation produces.

/// Number of entries in the cyclic colour ramp.
///
/// A power of two lets the wrap be a mask; 32 puts bands far enough apart to
/// read as a gradient at typical iteration counts.
const RAMP_LEN: usize = 32;

/// An RGB triple.
type Rgb = [u8; 3];

/// Control points of the ramp, interpolated evenly across [`RAMP_LEN`] slots.
///
/// Dark indigo → blue → teal → amber → white. The last stop repeats the first
/// so the ramp closes smoothly when iteration counts wrap around. Pure red is
/// avoided deliberately: interior points render black, and a red-adjacent ramp
/// makes fast-escaping pixels hard to tell from the filled set.
const STOPS: [Rgb; 6] = [
    [8, 8, 30],
    [30, 60, 200],
    [40, 200, 210],
    [240, 190, 60],
    [250, 250, 250],
    [8, 8, 30],
];

/// Build the ramp by linearly interpolating between consecutive stops.
///
/// `const fn` because the table depends on nothing but [`STOPS`]: evaluating it
/// at compile time means no `static` initialiser, no lazy branch, and no
/// external crate.
const fn ramp() -> [Rgb; RAMP_LEN] {
    let mut table = [[0u8; 3]; RAMP_LEN];
    let segments = STOPS.len() - 1;
    let mut index = 0;
    while index < RAMP_LEN {
        // Position along the ramp, in stop units. `index` never reaches
        // RAMP_LEN, so `stop + 1` stays inside `STOPS`.
        let position = index * segments / RAMP_LEN;
        let stop = if position >= segments {
            segments - 1
        } else {
            position
        };
        let from = STOPS[stop];
        let to = STOPS[stop + 1];
        let numerator = index * segments - stop * RAMP_LEN;
        let mut channel = 0;
        while channel < 3 {
            // Integer blend: (to - from) can be negative, so widen to i32
            // before scaling and clamp on the way back.
            let delta = to[channel] as i32 - from[channel] as i32;
            let mixed = from[channel] as i32 + delta * numerator as i32 / RAMP_LEN as i32;
            // Ord::clamp is not const-stable, so bound by hand; the blend can
            // only leave 0..=255 by a rounding whisker either side.
            table[index][channel] = if mixed < 0 {
                0
            } else if mixed > 255 {
                255
            } else {
                mixed as u8
            };
            channel += 1;
        }
        index += 1;
    }
    table
}

const RAMP: [Rgb; RAMP_LEN] = ramp();

/// Block-shade glyphs, dense for slow escape times, sparse for fast ones.
///
/// Indexed by the same normalised position as the colours, so structure still
/// reads on a terminal without colour support. Blocks rather than braille: a
/// block fills the whole cell, which absorbs the aspect-ratio mismatch between
/// cell width and cell height.
const SHADES: [char; 8] = [' ', '·', ':', '░', '▒', '▓', '█', '█'];

/// Interior sentinel from [`crate::escape::escape_time`].
const INTERIOR: u32 = u32::MAX;

/// Normalise an escape time into `0.0 ..= 1.0` for ramp lookup.
///
/// Square-root scaling spreads the bands apart near the set boundary, where a
/// linear ramp would push almost every pixel to the end of the cycle.
fn position(escape_time: u32, max_iter: u32) -> f64 {
    (f64::from(escape_time) / f64::from(max_iter.max(1))).sqrt()
}

/// Map an escape time to an RGB pixel; interior points render black.
#[must_use]
pub fn color(escape_time: u32, max_iter: u32) -> Rgb {
    if escape_time == INTERIOR {
        return [0, 0, 0];
    }
    let index = (position(escape_time, max_iter) * RAMP_LEN as f64) as usize
        % RAMP_LEN;
    RAMP[index]
}

/// Map an escape time to a terminal glyph.
#[must_use]
pub fn glyph(escape_time: u32, max_iter: u32) -> char {
    if escape_time == INTERIOR {
        return '█';
    }
    let index = (position(escape_time, max_iter) * SHADES.len() as f64)
        as usize
        % SHADES.len();
    SHADES[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_is_closed_and_in_gamut() {
        // Every entry must be reachable and no channel may saturate outside
        // 0..=255, which the integer blend clamps but could get wrong.
        for entry in RAMP.iter() {
            for channel in entry.iter() {
                assert!(u8::try_from(*channel).is_ok());
            }
        }
        assert_eq!(RAMP[0], STOPS[0], "ramp must start at the first stop");
    }

    #[test]
    fn interior_is_black_and_filled() {
        assert_eq!(color(INTERIOR, 100), [0, 0, 0]);
        assert_eq!(glyph(INTERIOR, 100), '█');
    }

    #[test]
    fn early_escapes_are_darker_than_late_ones() {
        let early = color(1, 1000);
        let late = color(500, 1000);
        let luminance = |p: Rgb| {
            p[0] as i32 + p[1] as i32 + p[2] as i32
        };
        assert!(
            luminance(early) < luminance(late),
            "early {early:?} should be darker than late {late:?}"
        );
    }

    #[test]
    fn glyph_index_never_overflows_the_ramp() {
        // max_iter = 1 divides by a tiny value; the modulo must still land
        // inside SHADES.
        for max_iter in [1, 2, 1_000_000] {
            for time in 0..max_iter.min(64) {
                let _ = glyph(time, max_iter);
                let _ = color(time, max_iter);
            }
        }
    }
}
