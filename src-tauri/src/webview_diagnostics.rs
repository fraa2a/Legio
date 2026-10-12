//! Observe WebView2 subprocess failures while the host is alive.
//! Native host termination cannot deliver these WebView2 callbacks.

use crate::application_log::ApplicationLog;

fn record_process_failure(
    log: &ApplicationLog,
    kind: Option<i32>,
    reason: Option<i32>,
    exit: Option<i32>,
) {
    let kind = kind.map_or_else(|| "unavailable".to_owned(), |value| value.to_string());
    let reason = reason.map_or_else(|| "unavailable".to_owned(), |value| value.to_string());
    let exit = exit.map_or_else(
        || "unavailable".to_owned(),
        |value| format!("0x{:08X}", value as u32),
    );
    log.record(
        "error",
        "webview_process_failed",
        Some(&format!(
            "kind: {kind}; reason: {reason}; exit_code: {exit}"
        )),
        None,
        true,
    );
}

fn record_browser_exit(log: &ApplicationLog, kind: Option<i32>) {
    let level = if kind == Some(0) { "info" } else { "error" };
    let kind = kind.map_or_else(|| "unavailable".to_owned(), |value| value.to_string());
    log.record(
        level,
        "webview_browser_exited",
        Some(&format!("kind: {kind}")),
        None,
        true,
    );
}

fn record_runtime(log: &ApplicationLog, version: &str) {
    let version = version
        .split_whitespace()
        .next()
        .filter(|version| {
            !version.is_empty()
                && version.len() <= 64
                && version
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b'.')
        })
        .unwrap_or("unavailable");
    log.record(
        "info",
        "webview_runtime",
        Some(&format!("WebView2 version: {version}")),
        None,
        true,
    );
}

#[cfg(windows)]
pub(crate) fn install(window: &tauri::WebviewWindow, log: ApplicationLog) {
    let callback_log = log.clone();
    if let Err(error) = window.with_webview(move |platform| {
        windows_observers::install(platform.controller(), platform.environment(), &callback_log);
    }) {
        log.record(
            "error",
            "webview_diagnostics_error",
            Some(&format!("schedule_observers: {error}")),
            None,
            true,
        );
    }
}

#[cfg(windows)]
mod windows_observers {
    use super::*;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_BROWSER_PROCESS_EXIT_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND,
        COREWEBVIEW2_PROCESS_FAILED_REASON, ICoreWebView2Controller, ICoreWebView2Environment,
        ICoreWebView2Environment5, ICoreWebView2ProcessFailedEventArgs2,
    };
    use webview2_com::{BrowserProcessExitedEventHandler, ProcessFailedEventHandler, take_pwstr};
    use windows::core::{Interface, PWSTR};

    fn record_error(log: &ApplicationLog, stage: &'static str, error: &windows::core::Error) {
        log.record(
            "error",
            "webview_diagnostics_error",
            Some(&format!(
                "{stage}; hresult: 0x{:08X}",
                error.code().0 as u32
            )),
            None,
            true,
        );
    }

    pub(super) fn install(
        controller: ICoreWebView2Controller,
        environment: ICoreWebView2Environment,
        log: &ApplicationLog,
    ) {
        let mut version = PWSTR::null();
        // SAFETY: The live environment writes an allocated string to a valid output pointer.
        match unsafe { environment.BrowserVersionString(&mut version) } {
            Ok(()) => record_runtime(log, &take_pwstr(version)),
            Err(error) => record_error(log, "read_runtime_version", &error),
        }

        // SAFETY: The controller is supplied on its owning UI thread by with_webview.
        let webview = match unsafe { controller.CoreWebView2() } {
            Ok(webview) => webview,
            Err(error) => {
                record_error(log, "get_core_webview", &error);
                return;
            }
        };
        let failure_log = log.clone();
        let failure_handler = ProcessFailedEventHandler::create(Box::new(move |_, args| {
            let mut kind = None;
            let mut reason = None;
            let mut exit = None;
            if let Some(args) = args {
                let mut value = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                // SAFETY: WebView2 supplies live event arguments and this output is initialized.
                match unsafe { args.ProcessFailedKind(&mut value) } {
                    Ok(()) => kind = Some(value.0),
                    Err(error) => record_error(&failure_log, "read_failure_kind", &error),
                }
                if let Ok(extended) = args.cast::<ICoreWebView2ProcessFailedEventArgs2>() {
                    let mut value = COREWEBVIEW2_PROCESS_FAILED_REASON::default();
                    // SAFETY: The queried interface is live and the output pointer is valid.
                    match unsafe { extended.Reason(&mut value) } {
                        Ok(()) => reason = Some(value.0),
                        Err(error) => record_error(&failure_log, "read_failure_reason", &error),
                    }
                    let mut value = 0;
                    // SAFETY: The queried interface is live and the output pointer is valid.
                    match unsafe { extended.ExitCode(&mut value) } {
                        Ok(()) => exit = Some(value),
                        Err(error) => record_error(&failure_log, "read_failure_exit_code", &error),
                    }
                }
            }
            record_process_failure(&failure_log, kind, reason, exit);
            Ok(())
        }));
        // Registrations last for the event source lifetime. Each handler captures only the log,
        // avoiding a reference cycle with the WebView or environment that owns the handler.
        let mut failure_token = 0;
        // SAFETY: The live WebView retains the COM handler until its event registration ends.
        let failure_registered =
            match unsafe { webview.add_ProcessFailed(&failure_handler, &mut failure_token) } {
                Ok(()) => true,
                Err(error) => {
                    record_error(log, "register_process_failed", &error);
                    false
                }
            };

        let browser_registered = match environment.cast::<ICoreWebView2Environment5>() {
            Ok(environment) => {
                let browser_log = log.clone();
                let browser_handler =
                    BrowserProcessExitedEventHandler::create(Box::new(move |_, args| {
                        let kind = if let Some(args) = args {
                            let mut value = COREWEBVIEW2_BROWSER_PROCESS_EXIT_KIND::default();
                            // SAFETY: WebView2 supplies live event arguments and a valid output pointer.
                            match unsafe { args.BrowserProcessExitKind(&mut value) } {
                                Ok(()) => Some(value.0),
                                Err(error) => {
                                    record_error(&browser_log, "read_browser_exit_kind", &error);
                                    None
                                }
                            }
                        } else {
                            None
                        };
                        record_browser_exit(&browser_log, kind);
                        Ok(())
                    }));
                let mut browser_token = 0;
                // SAFETY: The environment retains the COM handler for its own lifetime.
                match unsafe {
                    environment.add_BrowserProcessExited(&browser_handler, &mut browser_token)
                } {
                    Ok(()) => true,
                    Err(error) => {
                        record_error(log, "register_browser_exited", &error);
                        false
                    }
                }
            }
            Err(error) => {
                record_error(log, "get_browser_exit_interface", &error);
                false
            }
        };
        log.record(
            "info",
            "webview_observers_ready",
            Some(&format!(
                "process_failed: {failure_registered}; browser_exited: {browser_registered}"
            )),
            None,
            true,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("legio-webview-log-{}", uuid::Uuid::new_v4()))
    }

    fn records(directory: &std::path::Path) -> String {
        let path = directory.join("app.txt");
        assert!(
            path.is_file(),
            "WebView failures must reach the native application log"
        );
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn process_failure_persists_kind_reason_and_signed_exit_code() {
        let directory = directory();
        let log = ApplicationLog::new(directory.clone(), true);
        record_process_failure(&log, Some(1), Some(0), Some(-1073741819));
        let records = records(&directory);
        assert!(records.contains("ERROR webview_process_failed"));
        let message = records.as_str();
        for field in ["kind: 1", "reason: 0", "exit_code: 0xC0000005"] {
            assert!(message.contains(field), "{message}");
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn unavailable_extended_fields_do_not_discard_a_process_failure() {
        let directory = directory();
        let log = ApplicationLog::new(directory.clone(), true);
        record_process_failure(&log, Some(1), None, None);
        let records = records(&directory);
        let message = records.as_str();
        assert!(message.contains("kind: 1"));
        assert!(message.contains("reason: unavailable"));
        assert!(message.contains("exit_code: unavailable"));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn browser_exits_distinguish_normal_shutdown_from_failure() {
        let directory = directory();
        let log = ApplicationLog::new(directory.clone(), true);
        record_browser_exit(&log, Some(0));
        record_browser_exit(&log, Some(1));
        let records = records(&directory);
        assert!(records.contains("INFO webview_browser_exited"));
        assert!(records.contains("ERROR webview_browser_exited"));
        assert!(records.contains("kind: 1"));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn runtime_logging_keeps_version_and_excludes_unexpected_private_text() {
        let directory = directory();
        let log = ApplicationLog::new(directory.clone(), true);
        record_runtime(&log, "135.0.1.2");
        record_runtime(&log, "https://PRIVATE_USER/private-profile");
        let records = records(&directory);
        assert!(records.contains("135.0.1.2"));
        assert!(!records.contains("PRIVATE_USER"));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn opted_out_webview_events_create_no_files() {
        let directory = directory();
        let log = ApplicationLog::new(directory.clone(), false);
        record_process_failure(&log, Some(1), Some(0), Some(-1073741819));
        record_browser_exit(&log, Some(1));
        record_runtime(&log, "135.0.1.2");
        assert!(!directory.exists());
    }
}
