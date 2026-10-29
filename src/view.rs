//! The complex-plane rectangle a render covers, and its sampling grid.

/// A rectangle in the complex plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub real_min: f64,
    pub real_max: f64,
    pub imaginary_min: f64,
    pub imaginary_max: f64,
}

impl View {
    /// The whole set, framed with a little breathing room.
    pub const FULL: Self = Self {
        real_min: -2.5,
        real_max: 1.0,
        imaginary_min: -1.25,
        imaginary_max: 1.25,
    };

    /// Coordinate of column `column`, sampling cell centres rather than edges.
    ///
    /// Cell centres matter: sampling the left edge of every column shifts the
    /// picture half a pixel and makes a symmetric render look skewed.
    #[must_use]
    pub fn real_at(&self, column: u32, width: u32) -> f64 {
        let fraction = (column as f64 + 0.5) / width as f64;
        self.real_min + (self.real_max - self.real_min) * fraction
    }

    /// Coordinate of row `row`, with row `0` at the top of the image.
    #[must_use]
    pub fn imaginary_at(&self, row: u32, height: u32) -> f64 {
        let fraction = (row as f64 + 0.5) / height as f64;
        self.imaginary_max - (self.imaginary_max - self.imaginary_min) * fraction
    }

    #[must_use]
    pub fn width(&self) -> f64 {
        self.real_max - self.real_min
    }

    #[must_use]
    pub fn height(&self) -> f64 {
        self.imaginary_max - self.imaginary_min
    }

    /// Zoom by `factor` about `center`, keeping `aspect` (width ÷ height).
    ///
    /// Re-deriving both spans from the aspect ratio rather than scaling each
    /// independently is what stops a window resize from stretching the set.
    #[must_use = "zooming returns a new view; the receiver is unchanged"]
    pub fn zoomed(&self, factor: f64, center: (f64, f64), aspect: f64) -> Self {
        let new_width = self.width() * factor;
        let new_height = new_width / aspect;
        Self {
            real_min: center.0 - new_width / 2.0,
            real_max: center.0 + new_width / 2.0,
            imaginary_min: center.1 - new_height / 2.0,
            imaginary_max: center.1 + new_height / 2.0,
        }
    }

    /// Translate the view by a fraction of its own size.
    #[must_use]
    pub fn panned(&self, horizontal: f64, vertical: f64) -> Self {
        let dx = self.width() * horizontal;
        let dy = self.height() * vertical;
        Self {
            real_min: self.real_min + dx,
            real_max: self.real_max + dx,
            imaginary_min: self.imaginary_min + dy,
            imaginary_max: self.imaginary_max + dy,
        }
    }

    /// Parse a `real_min,imaginary_min,real_max,imaginary_max` rectangle.
    ///
    /// Ordered by imaginary part so it can be typed without counting which of
    /// the four numbers is which.
    ///
    /// # Errors
    ///
    /// Returns a description of the first malformed field, or of a rectangle
    /// whose extent is not strictly positive.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let parts: Vec<&str> = spec.split(',').map(str::trim).collect();
        if parts.len() != 4 {
            return Err(format!(
                "expected 4 comma-separated numbers, got {}: {spec}",
                parts.len()
            ));
        }
        let number = |text: &str| -> Result<f64, String> {
            text.parse::<f64>()
                .map_err(|_| format!("{text:?} is not a number"))
        };
        let view = Self {
            real_min: number(parts[0])?,
            imaginary_min: number(parts[1])?,
            real_max: number(parts[2])?,
            imaginary_max: number(parts[3])?,
        };
        if !(view.real_min < view.real_max && view.imaginary_min < view.imaginary_max) {
            return Err(format!(
                "rectangle must have positive extent in both axes: {spec}"
            ));
        }
        if !(view.real_max - view.real_min).is_finite() {
            return Err(format!("rectangle extent must be finite: {spec}"));
        }
        Ok(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_view_spans_the_set() {
        assert!(View::FULL.real_min <= -2.0 && View::FULL.real_max >= 0.5);
        assert!(View::FULL.imaginary_min <= -1.0 && View::FULL.imaginary_max >= 1.0);
    }

    #[test]
    fn samples_land_at_cell_centres() {
        let view = View {
            real_min: 0.0,
            real_max: 4.0,
            ..View::FULL
        };
        assert_eq!(view.real_at(0, 4), 0.5);
        assert_eq!(view.real_at(3, 4), 3.5);
    }

    #[test]
    fn rows_are_flipped_top_down() {
        let view = View::FULL;
        assert!(
            view.imaginary_at(0, 10) > view.imaginary_at(9, 10),
            "row 0 must be the top of the image"
        );
    }

    #[test]
    fn zoom_shrinks_about_the_center() {
        let view = View::FULL;
        let center = (0.0, 0.0);
        let zoomed = view.zoomed(0.5, center, 2.0);
        assert!((zoomed.width() - view.width() * 0.5).abs() < 1e-12);
        assert!(zoomed.real_min.abs() < 1e-12 && zoomed.real_max.abs() < 1e-12);
        assert!(
            (zoomed.width() / zoomed.height() - 2.0).abs() < 1e-9,
            "aspect ratio must be preserved"
        );
    }

    #[test]
    fn pan_moves_by_fractions() {
        let view = View::FULL.panned(0.5, 0.0);
        assert!(
            (view.real_min - (View::FULL.real_min + View::FULL.width() * 0.5)).abs()
                < 1e-12
        );
    }

    #[test]
    fn parse_accepts_a_rectangle() {
        let view = View::parse("-0.5,-0.5,0.5,0.5").expect("valid");
        assert_eq!(view.real_min, -0.5);
        assert_eq!(view.imaginary_max, 0.5);
    }

    #[test]
    fn parse_rejects_malformed_input() {
        for spec in [
            "0,1,2",
            "0,1,2,3,4",
            "a,0,1,1",
            "1,1,0,0",
            "0,0,0,1",
            "inf,0,1,1",
        ] {
            assert!(View::parse(spec).is_err(), "{spec} should be rejected");
        }
    }
}
