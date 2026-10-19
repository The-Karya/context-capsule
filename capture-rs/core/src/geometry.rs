// context-capsule — core: a CGRect-like rectangle (f64, top-left origin) with the same intersection rules.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const ZERO: Rect = Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };
    /// The whole image, normalized.
    pub const UNIT: Rect = Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };

    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect { x, y, width, height }
    }

    /// Negative sizes flipped, like CGRect's standardized.
    pub fn standardized(&self) -> Rect {
        let (x, w) = if self.width < 0.0 { (self.x + self.width, -self.width) } else { (self.x, self.width) };
        let (y, h) = if self.height < 0.0 { (self.y + self.height, -self.height) } else { (self.y, self.height) };
        Rect { x, y, width: w, height: h }
    }

    pub fn min_x(&self) -> f64 {
        self.standardized().x
    }
    pub fn min_y(&self) -> f64 {
        self.standardized().y
    }
    pub fn max_x(&self) -> f64 {
        let s = self.standardized();
        s.x + s.width
    }
    pub fn max_y(&self) -> f64 {
        let s = self.standardized();
        s.y + s.height
    }
    pub fn mid_x(&self) -> f64 {
        (self.min_x() + self.max_x()) / 2.0
    }
    pub fn mid_y(&self) -> f64 {
        (self.min_y() + self.max_y()) / 2.0
    }
    pub fn area(&self) -> f64 {
        (self.width * self.height).abs()
    }

    /// No area (CGRect.isEmpty).
    pub fn is_empty(&self) -> bool {
        self.width == 0.0 || self.height == 0.0
    }

    /// The shared part, or None when they do not meet (CGRect's null rect). Touching edges give a
    /// zero-size rect, like CGRect.
    pub fn intersection(&self, o: &Rect) -> Option<Rect> {
        let x1 = self.min_x().max(o.min_x());
        let x2 = self.max_x().min(o.max_x());
        let y1 = self.min_y().max(o.min_y());
        let y2 = self.max_y().min(o.max_y());
        if x1 > x2 || y1 > y2 {
            return None;
        }
        Some(Rect::new(x1, y1, x2 - x1, y2 - y1))
    }

    /// Half-open, like CGRect.contains(CGPoint): the far edges are outside.
    pub fn contains_point(&self, x: f64, y: f64) -> bool {
        !self.is_empty() && x >= self.min_x() && x < self.max_x() && y >= self.min_y() && y < self.max_y()
    }

    /// `o` lies wholly inside (edges may touch), like CGRect.contains(CGRect).
    pub fn contains_rect(&self, o: &Rect) -> bool {
        !self.is_empty()
            && !o.is_empty()
            && o.min_x() >= self.min_x()
            && o.max_x() <= self.max_x()
            && o.min_y() >= self.min_y()
            && o.max_y() <= self.max_y()
    }

    /// Shrink by `dx`, `dy` on each side (negative grows), like CGRect.insetBy.
    pub fn inset_by(&self, dx: f64, dy: f64) -> Rect {
        let s = self.standardized();
        Rect::new(s.x + dx, s.y + dy, s.width - 2.0 * dx, s.height - 2.0 * dy)
    }
}
