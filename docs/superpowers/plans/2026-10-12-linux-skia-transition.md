# Linux Skia transition

Approved implementation scope: adopt WebKitGTK's current compositor without rewriting the UI. Windows continues to use WebView2.

1. Remove the Hyprland startup override that selects TextureMapper. Keep explicit user environment overrides available for diagnosis.
2. Use the loaded WebKit version and per-view preferences to configure and report the Linux renderer. Retain GTK full-window invalidation only for legacy composition on transparent windows. Use a separate native probe for damage-limited composition with settings applied before page creation, leaving the application's engine default unchanged.
3. Read feature APIs dynamically because Tauri's current bindings predate them and older distribution runtimes must remain loadable. Check all symbols and feature availability, report unavailable APIs and features. The native probe validates requests and applies settings before constructing the WebView.
4. Add opt-in native diagnostics for backend, loaded engine, composition settings, transport overrides and WebProcess termination. Record no user paths or display names.
5. Verify policy boundaries and real feature read/write on the installed engine, compile, lint and run the Rust suite. Document affected-machine acceptance and controlled-runtime requirements. Build checks alone do not establish a visual fix.

Runtime packaging is a separate deliverable: the existing AppImage/deb/rpm pipeline does not by itself pin the host WebKit engine. Do not describe an ordinary package as an isolated runtime.

## Final review correction

- Important: live feature updates cannot establish a changed compositor because WebKit 2.54 latches those values at renderer construction. Removed all live writes from Legio. The separate native probe applies settings before WebView creation. `unsupported_live_damage_override_is_reported_instead_of_claimed_applied` failed before implementation and now covers rejection of an unsupported live switch. Native probe validation and final suite results are recorded in the verification document.
- Minor: requested-feature errors were ambiguous. Removing the ineffective production write paths also removes those errors; the standalone probe reports its exact missing feature. No separate polish change was made.
