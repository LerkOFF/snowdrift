#![windows_subsystem = "windows"]

mod app;
mod desktop;
mod halloween;
mod light;
mod log;
mod menu;
mod plow;
mod prefs;
mod render;
mod screen;
mod snow;
mod store;
mod tracker;
mod update;
mod welcome;

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::core::w;

fn main() {
    // What "Uninstall" in Settings runs; it removes everything, even with Snowdrift still running.
    if std::env::args().skip(1).any(|argument| argument == "--uninstall") {
        snowdrift::install::uninstall();
        return;
    }
    // What the one-line installer runs: copies this exe into the user's programs folder, registers it and starts it.
    if std::env::args().skip(1).any(|argument| argument == "--install") {
        install_self();
        return;
    }
    unsafe {
        let _instance = CreateMutexW(None, true, w!("Local\\Snowdrift"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    snowdrift::language::choose();
    log::start();
    std::panic::set_hook(Box::new(|info| log!("panic on the {} thread: {info}", std::thread::current().name().unwrap_or("unnamed"))));
    log!("Snowdrift {}", env!("CARGO_PKG_VERSION"));
    if let Err(error) = app::run() {
        log!("failed: {error}");
    }
}

fn install_self() {
    snowdrift::install::close_running();
    let result = std::env::current_exe()
        .map_err(|error| error.to_string())
        .and_then(|exe| std::fs::read(exe).map_err(|error| error.to_string()))
        .and_then(|bytes| snowdrift::install::install(&bytes, env!("CARGO_PKG_VERSION")));
    match result {
        Ok(exe) => snowdrift::install::launch(&exe),
        Err(error) => eprintln!("{error}"),
    }
}
