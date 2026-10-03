use std::io::Cursor;

use image::{DynamicImage, ImageFormat};

pub(crate) fn trim_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let source = image::load_from_memory_with_format(bytes, ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    trim_dynamic(&source)
}

pub(crate) fn trim_dynamic(source: &DynamicImage) -> Result<Vec<u8>, String> {
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
    Ok(output.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    use image::GenericImageView;

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
