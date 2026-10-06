mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tauri::{
        WebviewUrl,
        webview::{PermissionKind, PermissionResponse, WebviewWindowBuilder},
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_sql::Builder::default().build())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::greet,
            commands::save_recording,
            commands::compress_media,
            commands::transcribe_audio,
            commands::get_entries,
            commands::start_recording,
            commands::stop_recording,
            commands::save_entry,
            commands::get_summary,
            commands::rename_entry,
            commands::generate_title,
        ])
        .setup(|app| {
            let win =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title("your-voice")
                    .inner_size(800.0, 600.0)
                    .on_permission_request(|_, kind| match kind {
                        PermissionKind::Camera | PermissionKind::Microphone => {
                            PermissionResponse::Allow
                        }
                        _ => PermissionResponse::Default,
                    })
                    .build()?;

            // Enable getUserMedia on http://localhost (non-secure origin) via WKPreferences KVC.
            // with_webview runs the closure on the main thread after the WKWebView is created.
            #[cfg(target_os = "macos")]
            let _ = win.with_webview(|webview| {
                use objc2::exception::catch;
                use objc2_foundation::{ns_string, NSNumber, NSObjectNSKeyValueCoding};
                use objc2_web_kit::WKWebView;
                use std::panic::AssertUnwindSafe;
                unsafe {
                    let wkwebview = &*(webview.inner() as *const WKWebView);
                    let prefs = wkwebview.configuration().preferences();
                    let yes = NSNumber::numberWithBool(true);
                    let _ = catch(AssertUnwindSafe(|| {
                        prefs.setValue_forKey(Some(&yes), ns_string!("mediaDevicesEnabled"));
                    }));
                    let _ = catch(AssertUnwindSafe(|| {
                        prefs.setValue_forKey(Some(&yes), ns_string!("getUserMediaEnabled"));
                    }));
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
