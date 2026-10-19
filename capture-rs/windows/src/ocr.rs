// context-capsule — windows: on-device OCR with Windows.Media.Ocr (the user's profile languages) on the
// core's cropped and masked BGRA image. Lines top to bottom. The image is dropped right after.

use context_capsule_core::Image;

/// Lines top to bottom (stable: lines at the same height keep Windows' order), joined by "\n".
pub fn join_lines(mut lines: Vec<(f32, String)>) -> String {
    lines.retain(|(_, t)| !t.trim().is_empty());
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    lines.into_iter().map(|(_, t)| t).collect::<Vec<_>>().join("\n")
}

/// Shrink (never grow) so both sides fit in `max`, keeping the aspect ratio.
pub fn fit(width: usize, height: usize, max: usize) -> (usize, usize) {
    if width <= max && height <= max || max == 0 {
        return (width, height);
    }
    let scale = max as f64 / width.max(height) as f64;
    (((width as f64 * scale).floor() as usize).max(1), ((height as f64 * scale).floor() as usize).max(1))
}

/// Nearest-neighbour resize, for images larger than the OCR engine accepts.
pub fn downscale(image: &Image, max: usize) -> Image {
    let (w, h) = fit(image.width, image.height, max);
    if (w, h) == (image.width, image.height) {
        return image.clone();
    }
    let mut bgra = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let sy = y * image.height / h;
        for x in 0..w {
            let sx = x * image.width / w;
            let i = sy * image.stride + sx * 4;
            bgra.extend_from_slice(&image.bgra[i..i + 4]);
        }
    }
    Image { width: w, height: h, stride: w * 4, bgra }
}

#[cfg(windows)]
pub use win::Ocr;

#[cfg(windows)]
mod win {
    use super::*;
    use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Security::Cryptography::CryptographicBuffer;

    /// Made lazily on the engine thread; None when no OCR language is installed.
    #[derive(Default)]
    pub struct Ocr {
        engine: Option<OcrEngine>,
        tried: bool,
    }

    impl Ocr {
        fn engine(&mut self) -> Option<&OcrEngine> {
            if !self.tried {
                self.tried = true;
                self.engine = OcrEngine::TryCreateFromUserProfileLanguages().ok();
                if self.engine.is_none() {
                    eprintln!("Context Capsule: no Windows OCR language installed (Settings → Time & language → Language)");
                }
            }
            self.engine.as_ref()
        }

        pub fn read(&mut self, image: &Image) -> String {
            match self.recognize(image) {
                Ok(text) => text,
                Err(e) => {
                    eprintln!("Context Capsule: OCR failed: {}", e.message());
                    String::new()
                }
            }
        }

        fn recognize(&mut self, image: &Image) -> windows::core::Result<String> {
            let max = OcrEngine::MaxImageDimension().unwrap_or(2600) as usize;
            let Some(engine) = self.engine() else { return Ok(String::new()) };
            let image = downscale(image, max);
            if image.width == 0 || image.height == 0 {
                return Ok(String::new());
            }
            let buffer = CryptographicBuffer::CreateFromByteArray(&image.bgra)?;
            let bitmap = SoftwareBitmap::CreateWithAlpha(
                BitmapPixelFormat::Bgra8,
                image.width as i32,
                image.height as i32,
                BitmapAlphaMode::Premultiplied,
            )?;
            bitmap.CopyFromBuffer(&buffer)?;
            let result = engine.RecognizeAsync(&bitmap)?.join()?;
            let mut lines = vec![];
            for line in result.Lines()? {
                let top = line
                    .Words()?
                    .into_iter()
                    .filter_map(|w| w.BoundingRect().ok())
                    .map(|r| r.Y)
                    .fold(f32::INFINITY, f32::min);
                lines.push((if top.is_finite() { top } else { 0.0 }, line.Text()?.to_string_lossy()));
            }
            let _ = bitmap.Close();
            Ok(join_lines(lines))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_sorted_top_to_bottom() {
        let lines = vec![(40.0, "third".into()), (10.0, "first".into()), (25.0, "second".into()), (25.0, "second b".into())];
        assert_eq!(join_lines(lines), "first\nsecond\nsecond b\nthird");
        assert_eq!(join_lines(vec![(1.0, " ".into())]), "");
    }

    #[test]
    fn fit_only_shrinks() {
        assert_eq!(fit(800, 600, 2600), (800, 600));
        assert_eq!(fit(5200, 1300, 2600), (2600, 650));
        assert_eq!(fit(1000, 4000, 2000), (500, 2000));
        assert_eq!(fit(10_000, 1, 100), (100, 1), "never zero");
    }

    #[test]
    fn downscale_samples_pixels() {
        // 4×2, pixel (x, y) = [x, y, 0, 255]
        let mut bgra = vec![];
        for y in 0..2u8 {
            for x in 0..4u8 {
                bgra.extend_from_slice(&[x, y, 0, 255]);
            }
        }
        let img = Image { width: 4, height: 2, stride: 16, bgra };
        let small = downscale(&img, 2);
        assert_eq!((small.width, small.height, small.stride), (2, 1, 8));
        assert_eq!(small.bgra, vec![0, 0, 0, 255, 2, 0, 0, 255]);
        assert_eq!(downscale(&img, 10), img, "small enough already");
    }
}
