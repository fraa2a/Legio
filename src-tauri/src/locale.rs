use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LanguagePreference {
    #[default]
    System,
    #[serde(rename = "it")]
    Italian,
    #[serde(rename = "en")]
    English,
}

impl LanguagePreference {
    pub(crate) fn resolve(self) -> Self {
        if self != Self::System {
            return self;
        }
        #[cfg(windows)]
        {
            // SAFETY: this query takes no pointers and has no caller-side preconditions.
            let language = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
            if language & 0x03ff == 0x10 {
                return Self::Italian;
            }
        }
        #[cfg(target_os = "linux")]
        {
            let locales = std::env::var("LANGUAGE")
                .ok()
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    std::env::var("LC_ALL")
                        .ok()
                        .filter(|value| !value.is_empty())
                })
                .or_else(|| {
                    std::env::var("LC_MESSAGES")
                        .ok()
                        .filter(|value| !value.is_empty())
                })
                .or_else(|| std::env::var("LANG").ok())
                .unwrap_or_default();
            for locale in locales.split(':') {
                let primary = locale.split(['-', '_', '.']).next().unwrap_or_default();
                if primary.eq_ignore_ascii_case("it") {
                    return Self::Italian;
                }
                if primary.eq_ignore_ascii_case("en") {
                    return Self::English;
                }
            }
        }
        Self::English
    }

    pub(crate) fn code(self) -> &'static str {
        match self.resolve() {
            Self::Italian => "it",
            _ => "en",
        }
    }

    pub(crate) fn steam(self) -> &'static str {
        match self.resolve() {
            Self::Italian => "italian",
            _ => "english",
        }
    }

    pub(crate) fn text(self, italian: &'static str, english: &'static str) -> &'static str {
        match self.resolve() {
            Self::Italian => italian,
            _ => english,
        }
    }
}
