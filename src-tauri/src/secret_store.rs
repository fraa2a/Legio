//! One secret in the operating system keychain (Secret Service on Linux, Credential
//! Manager on Windows). Nothing is ever stored in the database or in a plain file.

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SecretError {
    /// No keychain service can be reached, for example headless Linux without a Secret Service.
    Unavailable,
    Failed(String),
}

impl fmt::Display for SecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => f.write_str(
                "No system keychain is available. Install and unlock a Secret Service provider \
                 such as GNOME Keyring or KWallet, then try again.",
            ),
            Self::Failed(message) => write!(f, "The system keychain failed: {message}"),
        }
    }
}

pub(crate) trait SecretStore: Send + Sync {
    fn get(&self) -> Result<Option<String>, SecretError>;
    fn set(&self, value: &str) -> Result<(), SecretError>;
    fn clear(&self) -> Result<(), SecretError>;
}

pub(crate) struct SystemKeychain {
    account: &'static str,
}

impl SystemKeychain {
    pub(crate) const fn new(account: &'static str) -> Self {
        Self { account }
    }
}

#[cfg(any(target_os = "linux", windows))]
impl SystemKeychain {
    fn entry(&self) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new("Legio", self.account).map_err(map_error)
    }
}

#[cfg(any(target_os = "linux", windows))]
fn map_error(error: keyring::Error) -> SecretError {
    match error {
        keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_) => {
            SecretError::Unavailable
        }
        other => SecretError::Failed(other.to_string()),
    }
}

#[cfg(any(target_os = "linux", windows))]
impl SecretStore for SystemKeychain {
    fn get(&self) -> Result<Option<String>, SecretError> {
        match self.entry()?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(map_error(error)),
        }
    }

    fn set(&self, value: &str) -> Result<(), SecretError> {
        self.entry()?.set_password(value).map_err(map_error)
    }

    fn clear(&self) -> Result<(), SecretError> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(map_error(error)),
        }
    }
}

// Only Linux and Windows are supported targets; elsewhere no keychain is wired up.
#[cfg(not(any(target_os = "linux", windows)))]
impl SecretStore for SystemKeychain {
    fn get(&self) -> Result<Option<String>, SecretError> {
        Err(SecretError::Unavailable)
    }

    fn set(&self, _value: &str) -> Result<(), SecretError> {
        Err(SecretError::Unavailable)
    }

    fn clear(&self) -> Result<(), SecretError> {
        let _ = self.account;
        Err(SecretError::Unavailable)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    /// In-memory keychain; `unavailable` simulates a machine without a Secret Service.
    #[derive(Default)]
    pub(crate) struct FakeKeychain {
        value: Mutex<Option<String>>,
        unavailable: bool,
    }

    impl FakeKeychain {
        pub(crate) fn unavailable() -> Self {
            Self {
                unavailable: true,
                ..Self::default()
            }
        }
    }

    impl SecretStore for FakeKeychain {
        fn get(&self) -> Result<Option<String>, SecretError> {
            if self.unavailable {
                return Err(SecretError::Unavailable);
            }
            Ok(self.value.lock().unwrap().clone())
        }

        fn set(&self, value: &str) -> Result<(), SecretError> {
            if self.unavailable {
                return Err(SecretError::Unavailable);
            }
            *self.value.lock().unwrap() = Some(value.to_owned());
            Ok(())
        }

        fn clear(&self) -> Result<(), SecretError> {
            if self.unavailable {
                return Err(SecretError::Unavailable);
            }
            *self.value.lock().unwrap() = None;
            Ok(())
        }
    }

    #[test]
    fn unavailable_keychain_explains_the_problem() {
        assert!(
            SecretError::Unavailable
                .to_string()
                .contains("No system keychain")
        );
    }
}
