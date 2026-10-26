//! Round trips through real encoders: every PNG color type and depth, and
//! baseline JPEG, decode to the pixels that went in.

use freeqrent::{Palette, PaletteOpts, Token};
use freeqrent_image::{DecodeError, Limits, RasterFormat, decode, decode_with};

fn encode_png(w: u32, h: u32, color: png::ColorType, depth: png::BitDepth, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(color);
        enc.set_depth(depth);
        if color == png::ColorType::Indexed {
            enc.set_palette(vec![255, 255, 255, 200, 30, 30]);
            enc.set_trns(vec![255, 128]);
        }
        let mut writer = enc.write_header().unwrap();
        writer.write_image_data(data).unwrap();
    }
    out
}

/// A color type and depth, the samples of two pixels, and the RGBA they
/// should decode to.
type Case = (png::ColorType, png::BitDepth, Vec<u8>, [[u8; 4]; 2]);

#[test]
fn png_color_types() {
    use png::{BitDepth::*, ColorType::*};
    let cases: Vec<Case> = vec![
        (
            Rgba,
            Eight,
            vec![10, 20, 30, 40, 50, 60, 70, 80],
            [[10, 20, 30, 40], [50, 60, 70, 80]],
        ),
        (
            Rgb,
            Eight,
            vec![1, 2, 3, 250, 251, 252],
            [[1, 2, 3, 255], [250, 251, 252, 255]],
        ),
        (
            Grayscale,
            Eight,
            vec![0, 200],
            [[0, 0, 0, 255], [200, 200, 200, 255]],
        ),
        (
            GrayscaleAlpha,
            Eight,
            vec![90, 10, 180, 255],
            [[90, 90, 90, 10], [180, 180, 180, 255]],
        ),
        (
            Indexed,
            Eight,
            vec![0, 1],
            [[255, 255, 255, 255], [200, 30, 30, 128]],
        ),
        (
            Rgb,
            Sixteen,
            vec![0xff, 0x00, 0x80, 0x00, 0x00, 0xff, 0, 0, 0, 0, 0, 0],
            [[255, 128, 0, 255], [0, 0, 0, 255]],
        ),
    ];
    for (color, depth, data, want) in cases {
        let img = decode(&encode_png(2, 1, color, depth, &data)).unwrap();
        assert_eq!((img.width(), img.height()), (2, 1));
        for (x, px) in want.iter().enumerate() {
            let got = img.pixel(x as u32, 0).unwrap();
            // 16-bit samples may round either way when stripped.
            let close = got.iter().zip(px).all(|(a, b)| a.abs_diff(*b) <= 1);
            assert!(close, "{color:?} {depth:?} pixel {x}: {got:?} vs {px:?}");
        }
    }
}

#[test]
fn jpeg_round_trip() {
    let (w, h) = (48u16, 32u16);
    let mut rgb = Vec::new();
    for _ in 0..h {
        for x in 0..w {
            rgb.extend_from_slice(if x < w / 2 {
                &[20, 40, 110]
            } else {
                &[250, 248, 240]
            });
        }
    }
    let mut out = Vec::new();
    let enc = jpeg_encoder::Encoder::new(&mut out, 95);
    enc.encode(&rgb, w, h, jpeg_encoder::ColorType::Rgb)
        .unwrap();
    let img = decode(&out).unwrap();
    assert_eq!((img.width(), img.height()), (48, 32));
    let near =
        |p: [u8; 4], q: [u8; 3]| p[3] == 255 && p.iter().zip(q).all(|(a, b)| a.abs_diff(b) <= 8);
    assert!(near(img.pixel(4, 16).unwrap(), [20, 40, 110]));
    assert!(near(img.pixel(44, 16).unwrap(), [250, 248, 240]));
    // And it feeds extraction.
    let (palette, _) = Palette::from_image(&img, PaletteOpts::new()).unwrap();
    assert!(palette.contrast(Token::Fg).delta_l >= freeqrent::EXTRACT_CONTRAST);
}

#[test]
fn rejects_what_it_should() {
    assert_eq!(decode(b"hello"), Err(DecodeError::UnknownFormat));
    assert_eq!(
        decode(b"GIF89a...."),
        Err(DecodeError::Unsupported(RasterFormat::Gif))
    );
    assert!(matches!(
        decode(b"\x89PNG\r\n\x1a\n garbage"),
        Err(DecodeError::Malformed(_))
    ));
    assert!(matches!(
        decode(&[0xff, 0xd8, 0xff, 0, 1, 2]),
        Err(DecodeError::Malformed(_))
    ));
    let big = encode_png(
        300,
        2,
        png::ColorType::Grayscale,
        png::BitDepth::Eight,
        &[0; 600],
    );
    let tight = Limits {
        max_width: 256,
        ..Limits::default()
    };
    assert_eq!(
        decode_with(&big, tight),
        Err(DecodeError::TooLarge {
            width: 300,
            height: 2
        })
    );
    let few = Limits {
        max_pixels: 500,
        ..Limits::default()
    };
    assert_eq!(
        decode_with(&big, few),
        Err(DecodeError::TooLarge {
            width: 300,
            height: 2
        })
    );
    assert!(decode(&big).is_ok());
}
