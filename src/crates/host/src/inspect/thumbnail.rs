//! A copied picture's size, and a small copy of it for the HUD: a full
//! screenshot is megabytes, which every HUD window would otherwise receive
//! with each change to the clipboard window.
use base64::Engine;
use png::{BitDepth, ColorType, Decoder, Encoder, Transformations};
use std::io::Cursor;

/// The small copy fits in this box, in pixels: the window shows it at half,
/// sharp on a Retina screen.
const BOX: (u32, u32) = (360, 240);

/// The picture's width and height, from its PNG header.
pub(crate) fn size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" || &png[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(png[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(png[20..24].try_into().ok()?);
    (width > 0 && height > 0).then_some((width, height))
}

/// A copy of `png` no larger than `BOX`, as a `data:` URL.
pub(crate) fn data_url(png: &[u8]) -> Option<String> {
    data_url_within(png, BOX)
}

/// A copy of `png` no larger than `within`, as a `data:` URL: the picture
/// enlarged in the window asks for a larger one.
pub(crate) fn data_url_within(png: &[u8], within: (u32, u32)) -> Option<String> {
    let (width, height, rgba) = decode(png)?;
    let scale = (within.0 as f64 / width as f64)
        .min(within.1 as f64 / height as f64)
        .min(1.0);
    let small_width = ((width as f64 * scale).round() as u32).max(1);
    let small_height = ((height as f64 * scale).round() as u32).max(1);
    let small = shrink(&rgba, (width, height), (small_width, small_height));
    let mut out = Vec::new();
    {
        let mut encoder = Encoder::new(&mut out, small_width, small_height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&small).ok()?;
    }
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(out)
    ))
}

/// The picture as 8-bit RGBA rows.
fn decode(png: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = Decoder::new(Cursor::new(png));
    decoder.set_transformations(Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    let pixels = &buffer[..info.buffer_size()];
    let rgba = match info.color_type {
        ColorType::Rgba => pixels.to_vec(),
        ColorType::Rgb => pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 255])
            .collect(),
        ColorType::GrayscaleAlpha => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[g, a]| [g, g, g, a])
            .collect(),
        ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        ColorType::Indexed => return None,
    };
    Some((info.width, info.height, rgba))
}

/// Each small pixel is the average of the source pixels it covers.
fn shrink(rgba: &[u8], from: (u32, u32), to: (u32, u32)) -> Vec<u8> {
    let (fw, fh) = (from.0 as usize, from.1 as usize);
    let (tw, th) = (to.0 as usize, to.1 as usize);
    let mut out = Vec::with_capacity(tw * th * 4);
    for y in 0..th {
        let (y0, y1) = (y * fh / th, ((y + 1) * fh / th).max(y * fh / th + 1));
        for x in 0..tw {
            let (x0, x1) = (x * fw / tw, ((x + 1) * fw / tw).max(x * fw / tw + 1));
            let mut sum = [0u64; 4];
            for row in y0..y1.min(fh) {
                for column in x0..x1.min(fw) {
                    let at = (row * fw + column) * 4;
                    for (channel, total) in sum.iter_mut().enumerate() {
                        *total += rgba[at + channel] as u64;
                    }
                }
            }
            let count = ((y1.min(fh) - y0) * (x1.min(fw) - x0)).max(1) as u64;
            out.extend(sum.iter().map(|total| (total / count) as u8));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_of(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = Encoder::new(&mut out, width, height);
            encoder.set_color(ColorType::Rgb);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_image_data(&vec![200; (width * height * 3) as usize])
                .unwrap();
        }
        out
    }

    #[test]
    fn a_screenshot_shrinks_into_the_box() {
        let png = png_of(1440, 900);
        assert_eq!(size(&png), Some((1440, 900)));
        let url = data_url(&png).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(url.trim_start_matches("data:image/png;base64,"))
            .unwrap();
        assert_eq!(size(&bytes), Some((360, 225)));
    }

    #[test]
    fn a_small_picture_keeps_its_size_and_noise_is_refused() {
        let url = data_url(&png_of(40, 20)).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(url.trim_start_matches("data:image/png;base64,"))
            .unwrap();
        assert_eq!(size(&bytes), Some((40, 20)));
        assert_eq!(size(b"not a picture at all, not even close"), None);
        assert_eq!(data_url(b"not a picture"), None);
    }
}
