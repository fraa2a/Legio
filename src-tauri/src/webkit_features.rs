use std::{ffi::CStr, ptr::NonNull};

use gtk::glib::translate::ToGlibPtr;
use webkit2gtk::Settings;

type FeatureList = libc::c_void;
type Feature = libc::c_void;
type SettingsPtr = *mut webkit2gtk::ffi::WebKitSettings;

pub(crate) struct Features {
    list: NonNull<FeatureList>,
    find: unsafe extern "C" fn(*mut FeatureList, *const libc::c_char) -> *mut Feature,
    get: unsafe extern "C" fn(SettingsPtr, *mut Feature) -> libc::c_int,
    unref: unsafe extern "C" fn(*mut FeatureList),
}

impl Features {
    pub(crate) fn load() -> Result<Self, &'static str> {
        // Tauri's bindings predate the feature API; resolve it without raising the runtime minimum.
        macro_rules! symbol {
            ($name:literal, $ty:ty) => {{
                // SAFETY: WebKit is linked for the process lifetime. Each ABI matches its C header.
                let pointer = unsafe { libc::dlsym(libc::RTLD_DEFAULT, $name.as_ptr()) };
                if pointer.is_null() {
                    return Err("Loaded WebKit does not expose the required feature API");
                }
                // SAFETY: dlsym resolved the named function with the explicitly specified C ABI.
                unsafe { std::mem::transmute::<*mut libc::c_void, $ty>(pointer) }
            }};
        }
        let all = symbol!(
            c"webkit_settings_get_all_features",
            unsafe extern "C" fn() -> *mut FeatureList
        );
        let find = symbol!(
            c"webkit_feature_list_find",
            unsafe extern "C" fn(*mut FeatureList, *const libc::c_char) -> *mut Feature
        );
        let get = symbol!(
            c"webkit_settings_get_feature_enabled",
            unsafe extern "C" fn(SettingsPtr, *mut Feature) -> libc::c_int
        );
        let unref = symbol!(
            c"webkit_feature_list_unref",
            unsafe extern "C" fn(*mut FeatureList)
        );
        // SAFETY: the API returns an owned feature list; Drop releases exactly this reference.
        let list = NonNull::new(unsafe { all() }).ok_or("WebKit returned no feature list")?;
        Ok(Self {
            list,
            find,
            get,
            unref,
        })
    }

    pub(crate) fn value(&self, settings: &Settings, name: &CStr) -> Option<bool> {
        // SAFETY: the list and settings are live, and name is a terminated C string.
        let feature = unsafe { (self.find)(self.list.as_ptr(), name.as_ptr()) };
        if feature.is_null() {
            return None;
        }
        // SAFETY: the feature is borrowed from our owned list and settings remains alive.
        Some(unsafe { (self.get)(settings.to_glib_none().0, feature) } != 0)
    }
}

impl Drop for Features {
    fn drop(&mut self) {
        // SAFETY: list is the sole owned reference returned by get_all_features.
        unsafe { (self.unref)(self.list.as_ptr()) };
    }
}

pub(crate) fn version() -> (u32, u32, u32) {
    // SAFETY: these linked, argument-free APIs only return the loaded library version.
    unsafe {
        (
            webkit2gtk::ffi::webkit_get_major_version(),
            webkit2gtk::ffi::webkit_get_minor_version(),
            webkit2gtk::ffi::webkit_get_micro_version(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Requires a display and a loaded WebKitGTK 2.54+ runtime"]
    fn native_feature_snapshot_reports_engine_defaults_and_missing_features() {
        assert!(version() >= (2, 54, 0));
        gtk::init().expect("initialize GTK on the native test thread");
        let settings = Settings::new();
        let features = Features::load().expect("feature API available");
        assert_eq!(
            features.value(&settings, c"UseSkiaForComposition"),
            Some(true)
        );
        assert!(
            features
                .value(&settings, c"UseDamagingInformationForCompositing")
                .is_some()
        );
        assert_eq!(features.value(&settings, c"LegioNonexistentFeature"), None);
        assert_eq!(
            features.value(&settings, c"UseSkiaForComposition"),
            Some(true)
        );
    }
}
