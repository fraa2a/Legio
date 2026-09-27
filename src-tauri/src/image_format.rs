#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImageFormat {
    Jpeg,
    Png,
    Webp,
}

impl ImageFormat {
    pub(crate) fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            Some(Self::Jpeg)
        } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
            Some(Self::Webp)
        } else {
            None
        }
    }

    pub(crate) fn content_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
        }
    }

    pub(crate) fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
        }
    }

    pub(crate) const ALL: [Self; 3] = [Self::Png, Self::Jpeg, Self::Webp];
}

#[cfg(test)]
mod tests {
    use super::ImageFormat;

    #[test]
    fn identifies_supported_image_formats() {
        assert_eq!(
            ImageFormat::from_bytes(b"\xff\xd8\xff"),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(
            ImageFormat::from_bytes(b"\x89PNG\r\n\x1a\n"),
            Some(ImageFormat::Png)
        );
        assert_eq!(
            ImageFormat::from_bytes(b"RIFF\x00\x00\x00\x00WEBP"),
            Some(ImageFormat::Webp)
        );
        assert_eq!(ImageFormat::from_bytes(b"MZ"), None);
    }
}
