use std::io::Cursor;

use image::{ImageFormat, ImageReader, Limits, RgbaImage};

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
    transformed_webp(bytes, None)
}

pub(crate) fn hero_webp(bytes: &[u8], max_width: u32, blurred: bool) -> Result<Vec<u8>, String> {
    if max_width == 0 || max_width > 3840 {
        return Err("Invalid artwork display width".to_owned());
    }
    transformed_webp(bytes, Some((max_width, blurred)))
}

fn transformed_webp(bytes: &[u8], hero: Option<(u32, bool)>) -> Result<Vec<u8>, String> {
    let format = format(bytes)?;
    let image = bounded_reader(bytes, format)?
        .decode()
        .map_err(|error| error.to_string())?;
    if format == ImageFormat::WebP && hero.is_none() {
        return bounded_output(bytes.to_vec());
    }
    let rgba = image.into_rgba8();
    let mut rgba = if format == ImageFormat::Png && hero.is_none() {
        trim_rgba(rgba)
    } else {
        rgba
    };
    if let Some((max_width, blurred)) = hero {
        let width = rgba.width().min(max_width);
        let width = if blurred {
            (width * 65 / 100).max(1)
        } else {
            width
        };
        let height =
            (u64::from(rgba.height()) * u64::from(width) / u64::from(rgba.width())).max(1) as u32;
        if width != rgba.width() {
            rgba = image::imageops::resize(
                &rgba,
                width,
                height,
                image::imageops::FilterType::Lanczos3,
            );
        }
        if blurred {
            let sigma = rgba.width() as f32 * (3.0 / 1248.0);
            rgba = image::imageops::blur(&rgba, sigma);
        }
    }
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

    use image::{DynamicImage, GenericImageView};

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
        let mut rgba = RgbaImage::from_pixel(16, 16, image::Rgba([20, 40, 60, 120]));
        rgba.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        rgba.put_pixel(8, 8, image::Rgba([0, 0, 0, 0]));
        let source = DynamicImage::ImageRgba8(rgba.clone());
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
            assert_eq!(result.dimensions(), rgba.dimensions());
            if format != ImageFormat::Jpeg {
                let decoded = result.to_rgba8();
                for (original, decoded) in rgba.pixels().zip(decoded.pixels()) {
                    assert_eq!(decoded[3], original[3], "{format:?} must preserve alpha");
                }
            }
        }
    }

    #[test]
    fn hero_cache_caps_resolution_and_cards_keep_65_percent_without_upscaling() {
        let source = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            3840,
            1240,
            image::Rgba([20, 40, 60, 255]),
        ));
        let mut input = Cursor::new(Vec::new());
        source.write_to(&mut input, ImageFormat::Png).unwrap();
        for (width, sharp_size, card_size) in [
            (1280, (1280, 413), (832, 268)),
            (1920, (1920, 620), (1248, 403)),
            (2560, (2560, 826), (1664, 536)),
            (3840, (3840, 1240), (2496, 806)),
        ] {
            let sharp = hero_webp(input.get_ref(), width, false).unwrap();
            let image = image::load_from_memory(&sharp).unwrap();
            assert_eq!(image.dimensions(), sharp_size);
            let card = hero_webp(&sharp, width, true).unwrap();
            assert_eq!(
                image::load_from_memory(&card).unwrap().dimensions(),
                card_size
            );
        }
        let small = hero_webp(input.get_ref(), 1280, false).unwrap();
        let unchanged = hero_webp(&small, 3840, false).unwrap();
        assert_eq!(
            image::load_from_memory(&unchanged).unwrap().dimensions(),
            (1280, 413)
        );
        assert!(hero_webp(input.get_ref(), 0, false).is_err());
        assert!(hero_webp(input.get_ref(), 5000, false).is_err());
    }

    #[test]
    fn card_processing_blurs_pixels_and_keeps_the_sharp_banner_separate() {
        let source = DynamicImage::ImageRgba8(RgbaImage::from_fn(1920, 620, |x, _| {
            let value = if x < 960 { 0 } else { 255 };
            image::Rgba([value, value, value, 255])
        }));
        let mut input = Cursor::new(Vec::new());
        source.write_to(&mut input, ImageFormat::Png).unwrap();
        let sharp = image::load_from_memory(&hero_webp(input.get_ref(), 1920, false).unwrap())
            .unwrap()
            .to_rgba8();
        let card = image::load_from_memory(&hero_webp(input.get_ref(), 1920, true).unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(sharp.dimensions(), (1920, 620));
        assert_eq!(card.dimensions(), (1248, 403));
        assert!(sharp.get_pixel(959, 310)[0] < 10);
        assert!(card.get_pixel(623, 201)[0] > 10);
        assert_eq!(card.get_pixel(623, 201)[3], 255);
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
