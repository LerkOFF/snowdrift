//! Panels inside a window, which the window list knows nothing of: a dialog or alert drawn inside the window, a picture
//! opened over the page. Only with `panels on`: they are read from the accessibility bus (AT-SPI) for the window in
//! front, on a thread of their own, and each catches snow on its top edge like a window.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use zbus::blocking::{Connection, connection};
use zbus::zvariant::OwnedObjectPath;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Dialog,
    Picture,
}

/// A panel, in screen pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Panel {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub kind: Kind,
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static CHANGED: AtomicBool = AtomicBool::new(false);
/// The process of the window in front, which the program tells the thread.
static ACTIVE_PID: AtomicU32 = AtomicU32::new(0);
static FOUND: OnceLock<Mutex<Vec<Panel>>> = OnceLock::new();

fn found() -> &'static Mutex<Vec<Panel>> {
    FOUND.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn set_enabled(on: bool) {
    if on && !ENABLED.swap(true, Ordering::Relaxed) {
        start();
    } else if !on {
        ENABLED.store(false, Ordering::Relaxed);
        if let Ok(mut panels) = found().lock() {
            panels.clear();
        }
        CHANGED.store(true, Ordering::Relaxed);
    }
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_active_pid(pid: u32) {
    ACTIVE_PID.store(pid, Ordering::Relaxed);
}

/// Whether the panels changed since the last call.
pub fn take_changed() -> bool {
    CHANGED.swap(false, Ordering::Relaxed)
}

pub fn all() -> Vec<Panel> {
    found().lock().map(|p| p.clone()).unwrap_or_default()
}

// AT-SPI roles that count, and those never worth looking inside.
const ROLE_ALERT: u32 = 2;
const ROLE_COLOR_CHOOSER: u32 = 9;
const ROLE_DIALOG: u32 = 16;
const ROLE_FILE_CHOOSER: u32 = 19;
const ROLE_FONT_CHOOSER: u32 = 22;
const ROLE_ICON: u32 = 26;
const ROLE_IMAGE: u32 = 27;
const ROLE_OPTION_PANE: u32 = 36;
const LEAVES: [u32; 14] = [29, 61, 43, 35, 32, 56, 79, 73, 88, 83, 7, 44, 62, 40];

const REGISTRY: &str = "org.a11y.atspi.Registry";
const ROOT: &str = "/org/a11y/atspi/accessible/root";
const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const COMPONENT: &str = "org.a11y.atspi.Component";

fn debug() -> bool {
    std::env::var_os("SNOWDRIFT_DEBUG").is_some()
}

type Child = (String, OwnedObjectPath);

fn connect() -> zbus::Result<Connection> {
    let session = Connection::session()?;
    // Toolkits only publish their tree once something says assistive technology is in use.
    let _ = session.call_method(
        Some("org.a11y.Bus"),
        "/org/a11y/bus",
        Some("org.freedesktop.DBus.Properties"),
        "Set",
        &("org.a11y.Status", "IsEnabled", zbus::zvariant::Value::from(true)),
    );
    let address: String = session.call_method(Some("org.a11y.Bus"), "/org/a11y/bus", Some("org.a11y.Bus"), "GetAddress", &())?.body().deserialize()?;
    connection::Builder::address(address.as_str())?.build()
}

fn children(bus: &Connection, node: &Child) -> Vec<Child> {
    bus.call_method(Some(node.0.as_str()), node.1.as_str(), Some(ACCESSIBLE), "GetChildren", &())
        .ok()
        .and_then(|m| m.body().deserialize::<Vec<Child>>().ok())
        .unwrap_or_default()
}

fn role(bus: &Connection, node: &Child) -> Option<u32> {
    bus.call_method(Some(node.0.as_str()), node.1.as_str(), Some(ACCESSIBLE), "GetRole", &()).ok()?.body().deserialize::<u32>().ok()
}

fn extents(bus: &Connection, node: &Child) -> Option<(i32, i32, i32, i32)> {
    // 0 asks for the screen's coordinates.
    bus.call_method(Some(node.0.as_str()), node.1.as_str(), Some(COMPONENT), "GetExtents", &(0u32,)).ok()?.body().deserialize().ok()
}

fn showing(bus: &Connection, node: &Child) -> bool {
    // The state set is two words of bits; "showing" is the twenty-fifth.
    bus.call_method(Some(node.0.as_str()), node.1.as_str(), Some(ACCESSIBLE), "GetState", &())
        .ok()
        .and_then(|m| m.body().deserialize::<Vec<u32>>().ok())
        .is_some_and(|words| words.first().is_some_and(|w| w & (1 << 25) != 0))
}

fn pid_of(bus: &Connection, name: &str) -> Option<u32> {
    bus.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "GetConnectionUnixProcessID", &(name,))
        .ok()?
        .body()
        .deserialize()
        .ok()
}

fn start() {
    let _ = std::thread::Builder::new().name("panels".into()).spawn(|| {
        let mut bus: Option<Connection> = None;
        while ENABLED.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(500));
            let pid = ACTIVE_PID.load(Ordering::Relaxed);
            if pid == 0 || pid == std::process::id() {
                continue;
            }
            if bus.is_none() {
                bus = connect().ok();
                if bus.is_none() {
                    // No accessibility bus: try again in a while, not at once.
                    std::thread::sleep(Duration::from_secs(5));
                    continue;
                }
            }
            let Some(connection) = bus.as_ref() else { continue };
            let panels = scan(connection, pid);
            if let Ok(mut stored) = found().lock() {
                if *stored != panels {
                    *stored = panels;
                    CHANGED.store(true, Ordering::Relaxed);
                }
            }
        }
    });
}

/// The panels in every window of the app with this process, nothing if it isn't on the bus or answers too slowly.
fn scan(bus: &Connection, pid: u32) -> Vec<Panel> {
    let registry: Child = (REGISTRY.to_string(), OwnedObjectPath::try_from(ROOT).unwrap());
    let apps = children(bus, &registry);
    if debug() {
        eprintln!("panels: pid {pid}, {} apps on the bus: {:?}", apps.len(), apps.iter().map(|a| (a.0.clone(), pid_of(bus, &a.0))).collect::<Vec<_>>());
    }
    let Some(app) = apps.into_iter().find(|app| pid_of(bus, &app.0) == Some(pid)) else { return Vec::new() };
    let deadline = Instant::now() + Duration::from_millis(300);
    let mut budget = 400;
    let mut panels = Vec::new();
    // Breadth first, so what is near the top of the tree is seen before the time is up: (node, depth).
    let mut queue: std::collections::VecDeque<(Child, u32)> = children(bus, &app).into_iter().map(|c| (c, 0)).collect();
    while let Some((node, depth)) = queue.pop_front() {
        if budget == 0 || Instant::now() > deadline {
            break;
        }
        budget -= 1;
        let Some(kind_role) = role(bus, &node) else { continue };
        if debug() {
            eprintln!("panels: depth {depth} role {kind_role} extents {:?}", extents(bus, &node));
        }
        let panel_kind = match kind_role {
            ROLE_ALERT | ROLE_COLOR_CHOOSER | ROLE_DIALOG | ROLE_FILE_CHOOSER | ROLE_FONT_CHOOSER | ROLE_OPTION_PANE if depth > 0 => Some(Kind::Dialog),
            ROLE_ICON | ROLE_IMAGE if depth > 0 => Some(Kind::Picture),
            _ => None,
        };
        if let Some(kind) = panel_kind {
            if let Some((x, y, w, h)) = extents(bus, &node) {
                let big = match kind {
                    Kind::Dialog => w >= 120 && h >= 60,
                    // A picture opened over the page is big; the thumbnails and icons in a page are not.
                    Kind::Picture => w >= 360 && h >= 220,
                };
                if debug() {
                    eprintln!("panels: candidate {kind:?} {w}x{h} big {big} showing {}", showing(bus, &node));
                }
                if big && showing(bus, &node) {
                    panels.push(Panel { x: x as f32, y: y as f32, width: w as f32, height: h as f32, kind });
                    continue;
                }
            }
        }
        if depth < 9 && !LEAVES.contains(&kind_role) {
            queue.extend(children(bus, &node).into_iter().map(|c| (c, depth + 1)));
        }
    }
    panels.sort_by(|a, b| (b.width * b.height).total_cmp(&(a.width * a.height)));
    panels.truncate(8);
    panels
}
