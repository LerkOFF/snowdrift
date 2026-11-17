//! Panels inside a window, which the window list knows nothing of: a dialog, a picture opened over the page. Only with
//! Snow on Panels on: they are read from the foreground window's UI Automation tree on a thread of their own, and each
//! catches snow on its top edge like a window.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, TreeScope_Descendants, UIA_ControlTypePropertyId, UIA_ImageControlTypeId, UIA_IsDialogPropertyId,
    UIA_WindowControlTypeId,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId};
use windows::Win32::System::Variant::{VARIANT, VT_BOOL, VT_I4};

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Dialog,
    Picture,
}

/// A panel, in physical screen pixels.
#[derive(Clone, Copy, PartialEq)]
pub struct Panel {
    pub rect: RECT,
    pub kind: Kind,
}

static ENABLED: AtomicBool = AtomicBool::new(false);
/// Set whenever a look found something different; the tracker clears it as it lists the windows again.
static CHANGED: AtomicBool = AtomicBool::new(false);
static FOUND: OnceLock<Mutex<HashMap<isize, (RECT, Vec<Panel>)>>> = OnceLock::new();

fn found() -> &'static Mutex<HashMap<isize, (RECT, Vec<Panel>)>> {
    FOUND.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn set_enabled(on: bool) {
    if on && !ENABLED.swap(true, Ordering::Relaxed) {
        start();
    } else if !on {
        ENABLED.store(false, Ordering::Relaxed);
        found().lock().map(|mut map| map.clear()).ok();
        CHANGED.store(true, Ordering::Relaxed);
    }
}

/// Whether the panels changed since the last call.
pub fn take_changed() -> bool {
    CHANGED.swap(false, Ordering::Relaxed)
}

/// The panels last found in the window `id` whose frame is `rect`.
pub fn of(id: isize, rect: &RECT) -> Vec<Panel> {
    let Ok(map) = found().lock() else { return Vec::new() };
    match map.get(&id) {
        Some((window, panels)) if near(window, rect) => panels.clone(),
        _ => Vec::new(),
    }
}

fn near(a: &RECT, b: &RECT) -> bool {
    (a.left - b.left).abs() < 16 && (a.top - b.top).abs() < 16 && (a.right - b.right).abs() < 16 && (a.bottom - b.bottom).abs() < 16
}

fn start() {
    let _ = std::thread::Builder::new().name("panels".into()).spawn(|| {
        unsafe {
            if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
                return;
            }
        }
        let Ok(automation) = (unsafe { CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }) else { return };
        while ENABLED.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(400));
            let window = unsafe { GetForegroundWindow() };
            if window.is_invalid() || own(window) {
                continue;
            }
            let panels = scan(&automation, window).unwrap_or_default();
            let mut rect = RECT::default();
            if unsafe { GetWindowRect(window, &mut rect) }.is_err() {
                continue;
            }
            if let Ok(mut map) = found().lock() {
                let id = window.0 as isize;
                if map.get(&id).map(|(_, before)| before) != Some(&panels) {
                    CHANGED.store(true, Ordering::Relaxed);
                }
                map.insert(id, (rect, panels));
            }
        }
    });
}

fn own(window: HWND) -> bool {
    let mut process = 0u32;
    unsafe { GetWindowThreadProcessId(window, Some(&mut process)) };
    process == unsafe { GetCurrentProcessId() }
}

/// Every dialog and big picture in a window, nothing if it has none or can't be read.
fn scan(automation: &IUIAutomation, window: HWND) -> windows::core::Result<Vec<Panel>> {
    unsafe {
        let mut bounds = RECT::default();
        GetWindowRect(window, &mut bounds)?;
        let root = automation.ElementFromHandle(window)?;
        let dialog = automation.CreatePropertyCondition(UIA_IsDialogPropertyId, &variant_bool(true))?;
        let panes = automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &variant_int(UIA_WindowControlTypeId.0))?;
        let pictures = automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &variant_int(UIA_ImageControlTypeId.0))?;
        let condition = automation.CreateOrCondition(&automation.CreateOrCondition(&dialog, &panes)?, &pictures)?;
        let all = root.FindAll(TreeScope_Descendants, &condition)?;
        let (width, height) = ((bounds.right - bounds.left).max(1) as f32, (bounds.bottom - bounds.top).max(1) as f32);
        let mut panels = Vec::new();
        for i in 0..all.Length()?.min(80) {
            let element = all.GetElement(i)?;
            if element.CurrentIsOffscreen().map(|b| b.as_bool()).unwrap_or(true) {
                continue;
            }
            let rect = element.CurrentBoundingRectangle()?;
            let (w, h) = ((rect.right - rect.left) as f32, (rect.bottom - rect.top) as f32);
            let inside = rect.left >= bounds.left - 2 && rect.top >= bounds.top - 2 && rect.right <= bounds.right + 2 && rect.bottom <= bounds.bottom + 2;
            let share = w * h / (width * height);
            let kind = if element.CurrentControlType()? == UIA_ImageControlTypeId { Kind::Picture } else { Kind::Dialog };
            let big = match kind {
                Kind::Dialog => w >= 120.0 && h >= 60.0 && share < 0.95,
                // A picture opened over the page is big; the thumbnails and icons in a page are not.
                Kind::Picture => w >= 360.0 && h >= 220.0 && share < 0.9,
            };
            if inside && big {
                panels.push(Panel { rect, kind });
            }
        }
        panels.sort_by_key(|p| -((p.rect.right - p.rect.left) as i64 * (p.rect.bottom - p.rect.top) as i64));
        panels.truncate(6);
        Ok(panels)
    }
}

fn variant_bool(value: bool) -> VARIANT {
    let mut variant = VARIANT::default();
    unsafe {
        let inner = &mut variant.Anonymous.Anonymous;
        inner.vt = VT_BOOL;
        inner.Anonymous.boolVal = windows::Win32::Foundation::VARIANT_BOOL(if value { -1 } else { 0 });
    }
    variant
}

fn variant_int(value: i32) -> VARIANT {
    let mut variant = VARIANT::default();
    unsafe {
        let inner = &mut variant.Anonymous.Anonymous;
        inner.vt = VT_I4;
        inner.Anonymous.lVal = value;
    }
    variant
}
