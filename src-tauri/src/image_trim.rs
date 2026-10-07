use std::io::Cursor;

use image::{DynamicImage, ImageFormat, ImageReader, Limits, RgbaImage};

const MAX_PIXELS: u64 = 8_000_000;
const MAX_DIMENSION: u32 = 8192;
const MAX_OUTPUT: usize = 2 * 1024 * 1024;

fn bounded_reader(bytes: &[u8], format: ImageFormat) -> Result<ImageReader<Cursor<&[u8]>>, String> {
    let (width, height) = ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|error| error.to_string())?;
    validate_dimensions(width, height)?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_PIXELS * 8);
    reader.limits(limits);
    Ok(reader)
}

pub(crate) fn trim_dynamic(source: &DynamicImage) -> Result<Vec<u8>, String> {
    validate_dimensions(source.width(), source.height())?;
    encode_png(trim_rgba(source.to_rgba8()))
}

fn trim_rgba(mut rgba: RgbaImage) -> RgbaImage {
    let (width, height) = rgba.dimensions();
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for (x, y, pixel) in rgba.enumerate_pixels() {
        if pixel[3] != 0 {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
    }
    if right >= left
        && bottom >= top
        && (left != 0 || top != 0 || right + 1 != width || bottom + 1 != height)
    {
        image::imageops::crop(&mut rgba, left, top, right - left + 1, bottom - top + 1).to_image()
    } else {
        rgba
    }
}

fn encode_png(rgba: RgbaImage) -> Result<Vec<u8>, String> {
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(rgba)
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    bounded_output(output.into_inner())
}

fn format(bytes: &[u8]) -> Result<ImageFormat, String> {
    match crate::image_format::ImageFormat::from_steam_bytes(bytes) {
        Some(crate::image_format::ImageFormat::Jpeg) => Ok(ImageFormat::Jpeg),
        Some(crate::image_format::ImageFormat::Webp) => Ok(ImageFormat::WebP),
        Some(crate::image_format::ImageFormat::Ico) => Ok(ImageFormat::Ico),
        Some(crate::image_format::ImageFormat::Png) => Ok(ImageFormat::Png),
        None => Err("Unsupported artwork image content".to_owned()),
    }
}

pub(crate) fn validate_image(bytes: &[u8]) -> Result<(), String> {
    bounded_reader(bytes, format(bytes)?)?
        .decode()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub(crate) fn webp_asset(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let format = format(bytes)?;
    let image = bounded_reader(bytes, format)?
        .decode()
        .map_err(|error| error.to_string())?;
    if format == ImageFormat::WebP {
        return bounded_output(bytes.to_vec());
    }
    let rgba = image.into_rgba8();
    let rgba = if format == ImageFormat::Png {
        trim_rgba(rgba)
    } else {
        rgba
    };
    let encoder = webp::Encoder::from_rgba(&rgba, rgba.width(), rgba.height());
    let mut config = webp::WebPConfig::new().map_err(|_| "Could not initialize WebP encoder.")?;
    config.quality = 85.0;
    config.method = 4;
    config.alpha_quality = 100;
    let output = encoder
        .encode_advanced(&config)
        .map_err(|error| format!("Could not encode artwork as WebP: {error:?}"))?
        .to_vec();
    bounded_output(output)
}

fn bounded_output(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    if bytes.len() > MAX_OUTPUT {
        return Err("Transformed artwork exceeds the byte limit".to_owned());
    }
    Ok(bytes)
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), String> {
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
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
    fn transparent_images_keep_their_dimensions() {
        let source = DynamicImage::ImageRgba8(image::RgbaImage::new(8, 5));
        let mut input = Cursor::new(Vec::new());
        source.write_to(&mut input, ImageFormat::Png).unwrap();
        let result = image::load_from_memory(&webp_asset(input.get_ref()).unwrap()).unwrap();
        assert_eq!(result.dimensions(), (8, 5));
        assert!(result.to_rgba8().pixels().all(|pixel| pixel[3] == 0));
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
        let trimmed = webp_asset(&bytes).unwrap();
        let decoded = image::load_from_memory(&trimmed).unwrap();
        assert_eq!(decoded.dimensions(), (6, 6));
    }
}
