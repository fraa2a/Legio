# Settings review screenshots

These screenshots render the actual SettingsDialog and shared controls in Chromium at 1440 x 1000. Native IPC is stubbed for the component preview; this is not a full desktop integration test.

- settings-downloads-dark.png: download settings and shared category navigation.
- settings-downloads-light-error.png: light palette and a persistent operation error.

The frontend regression suite separately checks that retry actions remain accessible and that errors do not disappear with toast expiry.
