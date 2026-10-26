//! Proof that a freeQRent SVG scans (SPEC §9): render it with resvg,
//! decode it with ZXing (rxing) and rqrr, and compare with the payload. [`verify`] does this
//! at several sizes and with blur and noise, because a code that only
//! decodes from a perfect render isn't good enough.

use std::fmt;

use resvg::{tiny_skia, usvg};

/// A greyscale image, 0 = black, row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gray {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// `width * height` luma values.
    pub pixels: Vec<u8>,
}

/// One way of rendering a code for [`verify`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Render {
    /// Pixels per module.
    pub px_per_module: u32,
    /// A 3×3 box blur, like a slightly soft camera.
    pub blur: bool,
    /// Seeded ±24 luma noise, like sensor noise.
    pub noise: bool,
}

/// The renders [`verify`] tries: three clean sizes and a soft, noisy one.
pub const RENDERS: [Render; 4] = [
    Render {
        px_per_module: 4,
        blur: false,
        noise: false,
    },
    Render {
        px_per_module: 6,
        blur: false,
        noise: false,
    },
    Render {
        px_per_module: 10,
        blur: false,
        noise: false,
    },
    Render {
        px_per_module: 8,
        blur: true,
        noise: true,
    },
];

/// Why verification failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerifyError {
    /// resvg couldn't parse the SVG.
    Parse(String),
    /// The SVG has no size or is too large to render.
    Size,
    /// No code was found in a render.
    NotFound(Render, Decoder),
    /// A code was found but didn't decode.
    Decode(Render, Decoder, String),
    /// A code decoded to something else.
    Mismatch(Render, Decoder, String),
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerifyError::Parse(e) => write!(f, "SVG didn't parse: {e}"),
            VerifyError::Size => f.write_str("SVG has no usable size"),
            VerifyError::NotFound(r, d) => write!(f, "{d:?} found no code ({r:?})"),
            VerifyError::Decode(r, d, e) => write!(f, "{d:?} couldn't decode ({r:?}): {e}"),
            VerifyError::Mismatch(r, d, got) => write!(f, "{d:?} decoded {got:?} ({r:?})"),
        }
    }
}

impl std::error::Error for VerifyError {}

/// Renders `svg` at `px_per_module` user units per pixel over white.
pub fn rasterize(svg: &str, px_per_module: f32) -> Result<Gray, VerifyError> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default())
        .map_err(|e| VerifyError::Parse(e.to_string()))?;
    let size = tree.size();
    let width = (size.width() * px_per_module).ceil() as u32;
    let height = (size.height() * px_per_module).ceil() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or(VerifyError::Size)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(px_per_module, px_per_module),
        &mut pixmap.as_mut(),
    );
    // Premultiplied RGBA over white, then Rec. 601 luma.
    let pixels = pixmap
        .pixels()
        .iter()
        .map(|p| {
            let over = |c: u8| c as u32 + 255 - p.alpha() as u32;
            let (r, g, b) = (over(p.red()), over(p.green()), over(p.blue()));
            ((r * 299 + g * 587 + b * 114 + 500) / 1000) as u8
        })
        .collect();
    Ok(Gray {
        width: width as usize,
        height: height as usize,
        pixels,
    })
}

/// A decoder [`verify`] runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decoder {
    /// rqrr, a port of quirc.
    Rqrr,
    /// rxing, a port of ZXing (the decoder behind most Android scanners).
    Zxing,
}

/// Decodes every code `decoder` finds in `image`.
pub fn decode(image: &Gray, decoder: Decoder) -> Result<Vec<String>, String> {
    match decoder {
        Decoder::Rqrr => {
            let mut prepared =
                rqrr::PreparedImage::prepare_from_greyscale(image.width, image.height, |x, y| {
                    image.pixels[y * image.width + x]
                });
            prepared
                .detect_grids()
                .into_iter()
                .map(|grid| {
                    grid.decode()
                        .map(|(_, text)| text)
                        .map_err(|e| e.to_string())
                })
                .collect()
        }
        Decoder::Zxing => {
            match rxing::helpers::detect_in_luma(
                image.pixels.clone(),
                image.width as u32,
                image.height as u32,
                Some(rxing::BarcodeFormat::QR_CODE),
            ) {
                Ok(result) => Ok(vec![result.getText().to_string()]),
                Err(rxing::Exceptions::NotFoundException(_)) => Ok(Vec::new()),
                Err(e) => Err(e.to_string()),
            }
        }
    }
}

/// Checks that `svg` decodes to `expected` under every one of [`RENDERS`].
pub fn verify(svg: &str, expected: &str) -> Result<(), VerifyError> {
    verify_renders(svg, expected, &RENDERS)
}

/// Checks only the clean renders of [`RENDERS`]: the bar for styles that
/// carry a low-contrast warning.
pub fn verify_clean(svg: &str, expected: &str) -> Result<(), VerifyError> {
    let clean: Vec<Render> = RENDERS
        .into_iter()
        .filter(|r| !r.blur && !r.noise)
        .collect();
    verify_renders(svg, expected, &clean)
}

/// Checks that `svg` decodes to `expected` under each of `renders`.
pub fn verify_renders(svg: &str, expected: &str, renders: &[Render]) -> Result<(), VerifyError> {
    for &render in renders {
        verify_one(svg, expected, render)?;
    }
    Ok(())
}

/// Checks one render. ZXing must decode every render; rqrr only the clean
/// ones, because its binarizer loses large codes after even a 3×3 blur
/// (plain squares included, so that says nothing about a style).
pub fn verify_one(svg: &str, expected: &str, render: Render) -> Result<(), VerifyError> {
    let mut image = rasterize(svg, render.px_per_module as f32)?;
    if render.blur {
        image = box_blur(&image);
    }
    if render.noise {
        add_noise(&mut image, 0x5eed);
    }
    let clean = !render.blur && !render.noise;
    for decoder in [Decoder::Zxing, Decoder::Rqrr] {
        if decoder == Decoder::Rqrr && !clean {
            continue;
        }
        let found = decode(&image, decoder).map_err(|e| VerifyError::Decode(render, decoder, e))?;
        match found.as_slice() {
            [] => return Err(VerifyError::NotFound(render, decoder)),
            [text] if text == expected => {}
            _ => return Err(VerifyError::Mismatch(render, decoder, found.join(" | "))),
        }
    }
    Ok(())
}

fn box_blur(image: &Gray) -> Gray {
    let (w, h) = (image.width as i64, image.height as i64);
    let at =
        |x: i64, y: i64| image.pixels[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize] as u32;
    let mut pixels = Vec::with_capacity(image.pixels.len());
    for y in 0..h {
        for x in 0..w {
            let sum: u32 = (-1..=1)
                .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
                .map(|(dx, dy)| at(x + dx, y + dy))
                .sum();
            pixels.push(((sum + 4) / 9) as u8);
        }
    }
    Gray {
        pixels,
        ..image.clone()
    }
}

fn add_noise(image: &mut Gray, seed: u64) {
    let mut state = seed | 1;
    for p in &mut image.pixels {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let delta = (state % 49) as i32 - 24;
        *p = (*p as i32 + delta).clamp(0, 255) as u8;
    }
}
