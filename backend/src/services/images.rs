//! Conversion of uploaded images to WebP (spec §13, ADR-0013).
//!
//! Untrusted bytes are parsed only by the pure-Rust decoders of the `image`
//! crate, under explicit limits. libwebp (C) only encodes pixel buffers
//! produced here, so nothing of the upload survives except its pixels and
//! frame timing: metadata, extension blocks and trailing bytes are dropped.

use std::io::Cursor;

use image::{
    AnimationDecoder, DynamicImage, ImageDecoder, Limits, RgbaImage,
    codecs::{gif::GifDecoder, jpeg::JpegDecoder, png::PngDecoder, webp::WebPDecoder},
    metadata::LoopCount,
};
use webp_animation::{Encoder, EncoderOptions, EncodingConfig, EncodingType};

use crate::domain::pictures::sniff_image_type;

/// Largest accepted upload, in bytes.
pub const MAX_UPLOAD_BYTES: usize = 5 * 1024 * 1024;
/// Largest stored WebP, in bytes.
pub const MAX_OUTPUT_BYTES: usize = 10 * 1024 * 1024;
/// Longest side of an image or animation canvas, in pixels.
pub const MAX_SIDE: u32 = 4096;
/// Most pixels in one frame (4096 × 4096).
pub const MAX_FRAME_PIXELS: u64 = 16_777_216;
/// Most frames in an animation.
pub const MAX_FRAMES: usize = 500;
/// Most pixels over all frames of an animation: bounds the encoding time
/// (about 7 s of one core at the limit).
pub const MAX_TOTAL_PIXELS: u64 = 100_000_000;
/// Largest allocation a decoder may make, in bytes.
const MAX_ALLOC: u64 = 256 * 1024 * 1024;
/// Browsers play GIF delays under 20 ms at 100 ms; keep the same speed.
const MIN_DELAY_MS: u32 = 20;
const DEFAULT_DELAY_MS: u32 = 100;
/// Longest frame delay a GIF can express (65 535 centiseconds).
const MAX_DELAY_MS: u32 = 655_350;
/// Quality of lossy stills (photos) and of lossy animations.
const STILL_QUALITY: f32 = 85.0;
const ANIMATION_QUALITY: f32 = 80.0;

/// An image re-encoded as WebP.
#[derive(Debug)]
pub struct Webp {
    pub width: u32,
    pub height: u32,
    pub animated: bool,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ImageError {
    /// Not a PNG, JPEG, GIF or WebP image.
    #[error("unsupported image type")]
    Unsupported,
    /// Claims a supported type but does not decode.
    #[error("invalid image")]
    Invalid,
    /// Beyond the dimension, frame or pixel budgets.
    #[error("image dimensions too large")]
    DimensionsTooLarge,
    /// The upload, or the WebP made from it, is too many bytes.
    #[error("image too large")]
    TooLarge,
}

/// How a still is encoded.
#[derive(Clone, Copy)]
enum Still {
    /// Photos (JPEG, lossy WebP).
    Lossy,
    /// Screenshots, drawings and GIFs (PNG, GIF, lossless WebP).
    Lossless,
}

/// Decodes `bytes` (PNG, JPEG, GIF or WebP) and re-encodes it as WebP.
/// Animated GIF and WebP stay animated; a single-frame animation is a still.
pub fn to_webp(bytes: &[u8]) -> Result<Webp, ImageError> {
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(ImageError::TooLarge);
    }
    let webp = match sniff_image_type(bytes).ok_or(ImageError::Unsupported)? {
        "image/png" => {
            let decoder = limited(PngDecoder::new(Cursor::new(bytes)).map_err(invalid)?)?;
            still(
                DynamicImage::from_decoder(decoder).map_err(invalid)?,
                Still::Lossless,
            )
        }
        "image/jpeg" => {
            let mut decoder = limited(JpegDecoder::new(Cursor::new(bytes)).map_err(invalid)?)?;
            // EXIF is dropped, so its rotation must be applied to the pixels.
            let orientation = decoder.orientation().map_err(invalid)?;
            let mut image = DynamicImage::from_decoder(decoder).map_err(invalid)?;
            image.apply_orientation(orientation);
            still(image, Still::Lossy)
        }
        "image/gif" => {
            let decoder = limited(GifDecoder::new(Cursor::new(bytes)).map_err(invalid)?)?;
            animation(decoder, Still::Lossless)
        }
        "image/webp" => {
            let decoder = limited(WebPDecoder::new(Cursor::new(bytes)).map_err(invalid)?)?;
            let kind = if is_lossless_webp(bytes) {
                Still::Lossless
            } else {
                Still::Lossy
            };
            if decoder.has_animation() {
                animation(decoder, kind)
            } else {
                still(DynamicImage::from_decoder(decoder).map_err(invalid)?, kind)
            }
        }
        _ => Err(ImageError::Unsupported),
    }?;
    if webp.data.len() > MAX_OUTPUT_BYTES {
        return Err(ImageError::TooLarge);
    }
    Ok(webp)
}

fn invalid(_: image::ImageError) -> ImageError {
    ImageError::Invalid
}

/// Checks the header's dimensions, then caps the decoder's allocations. Only
/// the header has been read so far: no pixel buffer exists yet.
fn limited<D: ImageDecoder>(mut decoder: D) -> Result<D, ImageError> {
    let (width, height) = decoder.dimensions();
    check_frame(width, height)?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    decoder
        .set_limits(limits)
        .map_err(|_| ImageError::DimensionsTooLarge)?;
    Ok(decoder)
}

fn check_frame(width: u32, height: u32) -> Result<(), ImageError> {
    if width == 0 || height == 0 {
        return Err(ImageError::Invalid);
    }
    if width > MAX_SIDE
        || height > MAX_SIDE
        || u64::from(width) * u64::from(height) > MAX_FRAME_PIXELS
    {
        return Err(ImageError::DimensionsTooLarge);
    }
    Ok(())
}

/// True for a simple-format lossless WebP (`VP8L` as its only chunk).
fn is_lossless_webp(bytes: &[u8]) -> bool {
    bytes.get(12..16) == Some(b"VP8L")
}

fn config(kind: Still) -> EncodingConfig {
    match kind {
        Still::Lossy => EncodingConfig::new_lossy(STILL_QUALITY),
        Still::Lossless => EncodingConfig {
            encoding_type: EncodingType::Lossless,
            ..EncodingConfig::default()
        },
    }
}

fn encoder(
    width: u32,
    height: u32,
    loop_count: i32,
    config: EncodingConfig,
) -> Result<Encoder, ImageError> {
    let mut options = EncoderOptions {
        encoding_config: Some(config),
        ..EncoderOptions::default()
    };
    options.anim_params.loop_count = loop_count;
    Encoder::new_with_options((width, height), options).map_err(|_| ImageError::Invalid)
}

/// Encodes one RGBA frame as a still WebP (libwebp writes a single-frame
/// animation as a plain image).
fn still(image: DynamicImage, kind: Still) -> Result<Webp, ImageError> {
    let rgba = image.into_rgba8();
    let (width, height) = rgba.dimensions();
    check_frame(width, height)?;
    let mut encoder = encoder(width, height, 0, config(kind))?;
    encoder
        .add_frame(&rgba, 0)
        .map_err(|_| ImageError::Invalid)?;
    let data = encoder
        .finalize(DEFAULT_DELAY_MS as i32)
        .map_err(|_| ImageError::Invalid)?;
    Ok(Webp {
        width,
        height,
        animated: false,
        data: data.to_vec(),
    })
}

/// Re-encodes an animation frame by frame, so only a few frames are in
/// memory at once. Frames arrive composited onto the full canvas.
fn animation<'a, D: AnimationDecoder<'a>>(decoder: D, kind: Still) -> Result<Webp, ImageError> {
    let loop_count = match decoder.loop_count() {
        LoopCount::Infinite => 0,
        LoopCount::Finite(n) => i32::try_from(n.get()).unwrap_or(0),
    };
    let mut frames = decoder.into_frames();
    let next = |frames: &mut image::Frames<'a>| -> Result<Option<(RgbaImage, u32)>, ImageError> {
        match frames.next() {
            None => Ok(None),
            Some(Err(image::ImageError::Limits(_))) => Err(ImageError::DimensionsTooLarge),
            Some(Err(_)) => Err(ImageError::Invalid),
            Some(Ok(frame)) => {
                let (num, den) = frame.delay().numer_denom_ms();
                let ms = num.checked_div(den).unwrap_or(0);
                let delay = if ms < MIN_DELAY_MS {
                    DEFAULT_DELAY_MS
                } else {
                    ms.min(MAX_DELAY_MS)
                };
                Ok(Some((frame.into_buffer(), delay)))
            }
        }
    };

    let Some((first, first_delay)) = next(&mut frames)? else {
        return Err(ImageError::Invalid);
    };
    let Some(second) = next(&mut frames)? else {
        return still(DynamicImage::ImageRgba8(first), kind);
    };

    let (width, height) = first.dimensions();
    check_frame(width, height)?;
    let frame_pixels = u64::from(width) * u64::from(height);
    // GIF frames (at most 256 colours each) stay lossless: smaller than the
    // GIF and as sharp. Lossy WebP animations stay lossy.
    let config = match kind {
        Still::Lossy => EncodingConfig::new_lossy(ANIMATION_QUALITY),
        Still::Lossless => config(Still::Lossless),
    };
    let mut encoder = encoder(width, height, loop_count, config)?;
    let (mut count, mut pixels, mut timestamp) = (0usize, 0u64, 0i32);
    let mut add = |frame: RgbaImage, delay: u32| -> Result<(), ImageError> {
        count += 1;
        pixels += frame_pixels;
        if count > MAX_FRAMES || pixels > MAX_TOTAL_PIXELS {
            return Err(ImageError::DimensionsTooLarge);
        }
        if frame.dimensions() != (width, height) {
            return Err(ImageError::Invalid);
        }
        encoder
            .add_frame(&frame, timestamp)
            .map_err(|_| ImageError::Invalid)?;
        // At most 500 frames of at most 655 350 ms: fits an i32.
        timestamp += delay as i32;
        Ok(())
    };
    add(first, first_delay)?;
    add(second.0, second.1)?;
    while let Some((frame, delay)) = next(&mut frames)? {
        add(frame, delay)?;
    }
    let data = encoder
        .finalize(timestamp)
        .map_err(|_| ImageError::Invalid)?;
    Ok(Webp {
        width,
        height,
        animated: true,
        data: data.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use image::{
        Delay, Frame, ImageEncoder, Rgba,
        codecs::{
            gif::{GifEncoder, Repeat},
            jpeg::JpegEncoder,
            png::PngEncoder,
            webp::WebPEncoder,
        },
    };

    use super::*;

    fn pattern(width: u32, height: u32, seed: u8) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            Rgba([
                (x as u8).wrapping_mul(16).wrapping_add(seed),
                (y as u8).wrapping_mul(32),
                seed,
                255,
            ])
        })
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        PngEncoder::new(&mut out)
            .write_image(
                &pattern(width, height, 0),
                width,
                height,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        out
    }

    fn jpeg(width: u32, height: u32) -> Vec<u8> {
        let rgb = DynamicImage::ImageRgba8(pattern(width, height, 0)).into_rgb8();
        let mut out = Vec::new();
        JpegEncoder::new_with_quality(&mut out, 90)
            .write_image(&rgb, width, height, image::ExtendedColorType::Rgb8)
            .unwrap();
        out
    }

    fn gif(frames: usize, width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut out);
            encoder.set_repeat(Repeat::Infinite).unwrap();
            for i in 0..frames {
                encoder
                    .encode_frame(Frame::from_parts(
                        pattern(width, height, (i * 40) as u8),
                        0,
                        0,
                        Delay::from_numer_denom_ms(100, 1),
                    ))
                    .unwrap();
            }
        }
        out
    }

    fn decode(webp: &Webp) -> WebPDecoder<Cursor<&[u8]>> {
        assert_eq!(&webp.data[..4], b"RIFF");
        assert_eq!(&webp.data[8..12], b"WEBP");
        WebPDecoder::new(Cursor::new(&webp.data[..])).unwrap()
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    #[test]
    fn png_becomes_a_lossless_still() {
        let out = to_webp(&png(8, 4)).unwrap();
        assert_eq!((out.width, out.height, out.animated), (8, 4, false));
        let decoder = decode(&out);
        assert!(!decoder.has_animation());
        let pixels = DynamicImage::from_decoder(decoder).unwrap().into_rgba8();
        assert_eq!(pixels, pattern(8, 4, 0));
    }

    #[test]
    fn jpeg_becomes_a_still_without_metadata_and_keeps_its_orientation() {
        let plain = jpeg(8, 4);
        // APP1 Exif segment: orientation 6 (rotate 90° clockwise), and a
        // comment segment standing in for GPS data or a payload.
        let tiff: &[u8] = &[
            b'M', b'M', 0, 42, 0, 0, 0, 8, // header
            0, 1, // one entry
            0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0, // orientation = 6
            0, 0, 0, 0, // no next IFD
        ];
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend_from_slice(tiff);
        let secret = b"SECRET-GPS-48.8584N";
        let mut bytes = vec![0xFF, 0xD8];
        bytes.extend_from_slice(&[0xFF, 0xE1]);
        bytes.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
        bytes.extend_from_slice(&app1);
        bytes.extend_from_slice(&[0xFF, 0xFE]);
        bytes.extend_from_slice(&((secret.len() + 2) as u16).to_be_bytes());
        bytes.extend_from_slice(secret);
        bytes.extend_from_slice(&plain[2..]);

        let out = to_webp(&bytes).unwrap();
        assert_eq!((out.width, out.height, out.animated), (4, 8, false));
        assert!(!contains(&out.data, b"Exif"));
        assert!(!contains(&out.data, secret));
        assert!(!decode(&out).has_animation());
    }

    #[test]
    fn animated_gif_becomes_an_animated_webp_without_its_payload() {
        let mut bytes = gif(3, 6, 6);
        // A comment extension before the trailer, and HTML after it.
        let trailer = bytes.pop();
        assert_eq!(trailer, Some(0x3B));
        bytes.extend_from_slice(&[0x21, 0xFE, 11]);
        bytes.extend_from_slice(b"hidden-note");
        bytes.extend_from_slice(&[0x00, 0x3B]);
        bytes.extend_from_slice(b"<script>alert(1)</script>");

        let out = to_webp(&bytes).unwrap();
        assert_eq!((out.width, out.height, out.animated), (6, 6, true));
        assert!(!contains(&out.data, b"<script>"));
        assert!(!contains(&out.data, b"hidden-note"));
        let decoder = decode(&out);
        assert!(decoder.has_animation());
        let frames = decoder.into_frames().collect_frames().unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].delay().numer_denom_ms(), (100, 1));
    }

    #[test]
    fn single_frame_gif_is_a_still() {
        let out = to_webp(&gif(1, 5, 5)).unwrap();
        assert!(!out.animated);
        assert!(!decode(&out).has_animation());
    }

    #[test]
    fn webp_stills_and_animations_are_re_encoded() {
        let mut still = Vec::new();
        WebPEncoder::new_lossless(&mut still)
            .write_image(&pattern(4, 4, 7), 4, 4, image::ExtendedColorType::Rgba8)
            .unwrap();
        still.extend_from_slice(b"<html>");
        let out = to_webp(&still).unwrap();
        assert!(!out.animated);
        assert!(!contains(&out.data, b"<html>"));

        let animated = to_webp(&gif(2, 4, 4)).unwrap();
        let again = to_webp(&animated.data).unwrap();
        assert!(again.animated);
        assert_eq!(
            decode(&again).into_frames().collect_frames().unwrap().len(),
            2
        );
    }

    #[test]
    fn short_delays_play_at_browser_speed() {
        let mut out = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut out);
            for i in 0..2 {
                encoder
                    .encode_frame(Frame::from_parts(
                        pattern(2, 2, i * 99),
                        0,
                        0,
                        Delay::from_numer_denom_ms(0, 1),
                    ))
                    .unwrap();
            }
        }
        let webp = to_webp(&out).unwrap();
        let frames = decode(&webp).into_frames().collect_frames().unwrap();
        assert_eq!(frames[0].delay().numer_denom_ms(), (DEFAULT_DELAY_MS, 1));
    }

    #[test]
    fn rejects_unsupported_and_broken_input() {
        assert_eq!(to_webp(b"").unwrap_err(), ImageError::Unsupported);
        assert_eq!(
            to_webp(br#"<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"/>"#).unwrap_err(),
            ImageError::Unsupported
        );
        assert_eq!(
            to_webp(b"<html><script>alert(1)</script>").unwrap_err(),
            ImageError::Unsupported
        );
        // Right magic bytes, nothing behind them.
        assert_eq!(to_webp(b"GIF89a").unwrap_err(), ImageError::Invalid);
        assert_eq!(
            to_webp(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0]).unwrap_err(),
            ImageError::Invalid
        );
        let truncated = png(8, 8);
        assert!(to_webp(&truncated[..truncated.len() / 2]).is_err());
    }

    #[test]
    fn rejects_oversize_bodies() {
        let mut bytes = png(1, 1);
        bytes.resize(MAX_UPLOAD_BYTES + 1, 0);
        assert_eq!(to_webp(&bytes).unwrap_err(), ImageError::TooLarge);
    }

    #[test]
    fn rejects_huge_dimensions_before_decoding() {
        // A 1 × 1 GIF whose header announces a 65 535 × 65 535 canvas: a
        // decompression bomb must fail on the header alone.
        let mut bytes = gif(1, 1, 1);
        bytes[6..10].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(to_webp(&bytes).unwrap_err(), ImageError::DimensionsTooLarge);

        // A PNG header claiming 5000 × 10 px.
        let mut png_bytes = png(1, 1);
        png_bytes[16..20].copy_from_slice(&5000u32.to_be_bytes());
        assert!(matches!(
            to_webp(&png_bytes).unwrap_err(),
            ImageError::DimensionsTooLarge | ImageError::Invalid
        ));
    }

    #[test]
    fn rejects_too_many_frames() {
        let bytes = gif(MAX_FRAMES + 1, 1, 1);
        assert_eq!(to_webp(&bytes).unwrap_err(), ImageError::DimensionsTooLarge);
    }
}
