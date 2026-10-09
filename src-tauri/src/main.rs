#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> tauri::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let hyprland = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
            || std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktop| {
                desktop
                    .split(':')
                    .next()
                    .is_some_and(|name| name.trim().eq_ignore_ascii_case("hyprland"))
            });
        if hyprland && std::env::var_os("WEBKIT_USE_SKIA_FOR_COMPOSITION").is_none() {
            // WebKitGTK 2.54's Skia compositor flashes stale animation frames on Hyprland.
            // TextureMapper keeps accelerated compositing, WebGL and transparency available.
            // SAFETY: main is still single-threaded; Tauri and GTK have not been initialized.
            unsafe { std::env::set_var("WEBKIT_USE_SKIA_FOR_COMPOSITION", "0") };
        }
    }
    legio_lib::run()
}
