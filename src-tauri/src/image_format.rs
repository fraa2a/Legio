#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImageFormat {
    Jpeg,
    Png,
    Webp,
    Ico,
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

    pub(crate) fn from_steam_bytes(bytes: &[u8]) -> Option<Self> {
        Self::from_bytes(bytes).or_else(|| {
            (bytes.len() >= 6
                && bytes[..4] == [0, 0, 1, 0]
                && u16::from_le_bytes([bytes[4], bytes[5]]) > 0)
                .then_some(Self::Ico)
        })
    }

    pub(crate) fn content_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
            Self::Ico => "image/x-icon",
        }
    }

    pub(crate) fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
            Self::Ico => "ico",
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
        assert_eq!(ImageFormat::from_bytes(&[0, 0, 1, 0, 1, 0]), None);
        assert_eq!(
            ImageFormat::from_steam_bytes(&[0, 0, 1, 0, 1, 0]),
            Some(ImageFormat::Ico)
        );
    }
}
