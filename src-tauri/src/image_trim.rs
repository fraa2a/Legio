use std::io::Cursor;

use image::{DynamicImage, ImageFormat, ImageReader, Limits};

const MAX_PIXELS: u64 = 8_000_000;
const MAX_DIMENSION: u32 = 8192;
const MAX_OUTPUT: usize = 2 * 1024 * 1024;

pub(crate) fn trim_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
    let (width, height) = reader
        .into_dimensions()
        .map_err(|error| error.to_string())?;
    validate_dimensions(width, height)?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_PIXELS * 4);
    reader.limits(limits);
    let source = reader.decode().map_err(|error| error.to_string())?;
    trim_dynamic(&source)
}

pub(crate) fn trim_dynamic(source: &DynamicImage) -> Result<Vec<u8>, String> {
    validate_dimensions(source.width(), source.height())?;
    let rgba = source.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut left = width;
    let mut top = height;
    let mut right = 0;
    let mut bottom = 0;
    for y in 0..height {
        for x in 0..width {
            if rgba.get_pixel(x, y)[3] == 0 {
                continue;
            }
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
    }
    let trimmed = if right >= left && bottom >= top {
        source.crop_imm(left, top, right - left + 1, bottom - top + 1)
    } else {
        source.clone()
    };
    let mut output = Cursor::new(Vec::new());
    trimmed
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    let output = output.into_inner();
    if output.len() > MAX_OUTPUT {
        return Err("Transformed artwork exceeds the byte limit".to_owned());
    }
    Ok(output)
}

pub(crate) fn webp_asset(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let format = crate::image_format::ImageFormat::from_steam_bytes(bytes)
        .ok_or("Steam returned unsupported image content.")?;
    let png;
    let (bytes, format) = if format == crate::image_format::ImageFormat::Png {
        png = trim_png(bytes)?;
        (png.as_slice(), ImageFormat::Png)
    } else {
        (
            bytes,
            match format {
                crate::image_format::ImageFormat::Jpeg => ImageFormat::Jpeg,
                crate::image_format::ImageFormat::Webp => ImageFormat::WebP,
                crate::image_format::ImageFormat::Ico => ImageFormat::Ico,
                crate::image_format::ImageFormat::Png => ImageFormat::Png,
            },
        )
    };
    let reader = ImageReader::with_format(Cursor::new(bytes), format);
    let (width, height) = reader
        .into_dimensions()
        .map_err(|error| error.to_string())?;
    validate_dimensions(width, height)?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_PIXELS * 8);
    reader.limits(limits);
    let image = reader.decode().map_err(|error| error.to_string())?;
    if format == ImageFormat::WebP {
        return Ok(bytes.to_vec());
    }
    let rgba = image.to_rgba8();
    let encoder = webp::Encoder::from_rgba(&rgba, width, height);
    let mut config = webp::WebPConfig::new().map_err(|_| "Could not initialize WebP encoder.")?;
    config.quality = 85.0;
    config.method = 4;
    config.alpha_quality = 100;
    let output = encoder
        .encode_advanced(&config)
        .map_err(|error| format!("Could not encode artwork as WebP: {error:?}"))?
        .to_vec();
    if output.len() > MAX_OUTPUT {
        return Err("Optimized artwork exceeds the byte limit.".to_owned());
    }
    Ok(output)
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), String> {
    if width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err("Artwork exceeds the dimension or pixel limit".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use image::GenericImageView;

    #[test]
    fn rejects_oversized_dimensions_before_allocating_pixels() {
        assert!(validate_dimensions(8193, 1).is_err());
        assert!(validate_dimensions(4000, 4000).is_err());
        assert!(validate_dimensions(1920, 1080).is_ok());
    }

    #[test]
    fn converts_supported_artwork_to_webp_with_alpha() {
        let source = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([20, 40, 60, 120]),
        ));
        for format in [
            ImageFormat::Png,
            ImageFormat::WebP,
            ImageFormat::Ico,
            ImageFormat::Jpeg,
        ] {
            let mut input = Cursor::new(Vec::new());
            let image = if format == ImageFormat::Jpeg {
                DynamicImage::ImageRgb8(source.to_rgb8())
            } else {
                source.clone()
            };
            image.write_to(&mut input, format).unwrap();
            let bytes = webp_asset(input.get_ref()).unwrap();
            assert_eq!(
                crate::image_format::ImageFormat::from_bytes(&bytes),
                Some(crate::image_format::ImageFormat::Webp)
            );
            let result = image::load_from_memory(&bytes).unwrap();
            assert_eq!((result.width(), result.height()), (2, 2));
            if format != ImageFormat::Jpeg {
                assert_eq!(result.to_rgba8().get_pixel(0, 0)[3], 120);
            }
        }
    }

    #[test]
    fn trims_transparent_padding() {
        let mut frame = image::RgbaImage::new(100, 100);
        for pixel in frame.pixels_mut() {
            *pixel = image::Rgba([0, 0, 0, 0]);
        }
        frame.put_pixel(10, 20, image::Rgba([255, 0, 0, 255]));
        frame.put_pixel(15, 25, image::Rgba([0, 255, 0, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(frame)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        let trimmed = trim_png(&bytes).unwrap();
        let decoded = image::load_from_memory(&trimmed).unwrap();
        assert_eq!(decoded.dimensions(), (6, 6));
    }
}
