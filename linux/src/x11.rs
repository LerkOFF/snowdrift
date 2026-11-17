//! The X11 side: a see-through click-through window over each display, the list of windows with their frames, and the
//! pointer. Wayland sessions get the same through XWayland, which shows X windows only.

use std::error::Error;

use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    self, AtomEnum, ColormapAlloc, ConnectionExt as _, CreateGCAux, CreateWindowAux, EventMask, ImageFormat, MapState, PropMode, StackMode,
    VisualClass, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::canvas::Canvas;

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Clone, Debug, PartialEq)]
pub struct Monitor {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// A window as the window manager shows it: its outer frame in root pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Client {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
    pub fullscreen: bool,
    /// Another workspace than the one showing.
    pub elsewhere: bool,
    pub radius: f32,
}

struct Atoms {
    client_list_stacking: u32,
    client_list: u32,
    frame_extents: u32,
    gtk_frame_extents: u32,
    wm_state: u32,
    wm_desktop: u32,
    current_desktop: u32,
    window_type: u32,
    state_hidden: u32,
    state_fullscreen: u32,
    state_max_h: u32,
    state_max_v: u32,
    type_normal: u32,
    type_dialog: u32,
    type_utility: u32,
    type_toolbar: u32,
    type_splash: u32,
    resource_manager: u32,
}

pub struct Display {
    pub conn: RustConnection,
    root: Window,
    screen_num: usize,
    atoms: Atoms,
    depth: u8,
    visual: u32,
    colormap: u32,
    gc: u32,
    own: Vec<u32>,
}

pub struct Overlay {
    pub window: u32,
    pub shown: bool,
}

fn atom(conn: &RustConnection, name: &str) -> Result<u32> {
    Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
}

impl Display {
    pub fn open() -> Result<Display> {
        let (conn, screen_num) = RustConnection::connect(None)?;
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        let (mut depth, mut visual) = (0u8, 0u32);
        for d in &screen.allowed_depths {
            if d.depth == 32 {
                if let Some(v) = d.visuals.iter().find(|v| v.class == VisualClass::TRUE_COLOR && v.bits_per_rgb_value == 8) {
                    depth = 32;
                    visual = v.visual_id;
                    break;
                }
            }
        }
        if depth != 32 {
            return Err("the X server has no 32-bit visual with alpha, so snow can't be drawn over windows".into());
        }
        let colormap = conn.generate_id()?;
        conn.create_colormap(ColormapAlloc::NONE, colormap, root, visual)?;
        let atoms = Atoms {
            client_list_stacking: atom(&conn, "_NET_CLIENT_LIST_STACKING")?,
            client_list: atom(&conn, "_NET_CLIENT_LIST")?,
            frame_extents: atom(&conn, "_NET_FRAME_EXTENTS")?,
            gtk_frame_extents: atom(&conn, "_GTK_FRAME_EXTENTS")?,
            wm_state: atom(&conn, "_NET_WM_STATE")?,
            wm_desktop: atom(&conn, "_NET_WM_DESKTOP")?,
            current_desktop: atom(&conn, "_NET_CURRENT_DESKTOP")?,
            window_type: atom(&conn, "_NET_WM_WINDOW_TYPE")?,
            state_hidden: atom(&conn, "_NET_WM_STATE_HIDDEN")?,
            state_fullscreen: atom(&conn, "_NET_WM_STATE_FULLSCREEN")?,
            state_max_h: atom(&conn, "_NET_WM_STATE_MAXIMIZED_HORZ")?,
            state_max_v: atom(&conn, "_NET_WM_STATE_MAXIMIZED_VERT")?,
            type_normal: atom(&conn, "_NET_WM_WINDOW_TYPE_NORMAL")?,
            type_dialog: atom(&conn, "_NET_WM_WINDOW_TYPE_DIALOG")?,
            type_utility: atom(&conn, "_NET_WM_WINDOW_TYPE_UTILITY")?,
            type_toolbar: atom(&conn, "_NET_WM_WINDOW_TYPE_TOOLBAR")?,
            type_splash: atom(&conn, "_NET_WM_WINDOW_TYPE_SPLASH")?,
            resource_manager: AtomEnum::RESOURCE_MANAGER.into(),
        };
        let gc = conn.generate_id()?;
        let probe = conn.generate_id()?;
        conn.create_window(depth, probe, root, 0, 0, 1, 1, 0, WindowClass::INPUT_OUTPUT, visual, &CreateWindowAux::new().border_pixel(0).colormap(colormap))?;
        conn.create_gc(gc, probe, &CreateGCAux::new())?;
        conn.destroy_window(probe)?;
        conn.change_window_attributes(root, &xproto::ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE | EventMask::SUBSTRUCTURE_NOTIFY))?;
        conn.flush()?;
        Ok(Display { conn, root, screen_num, atoms, depth, visual, colormap, gc, own: Vec::new() })
    }

    /// Whether a compositing manager runs; without one an ARGB window draws black instead of see-through.
    pub fn has_compositor(&self) -> bool {
        let name = format!("_NET_WM_CM_S{}", self.screen_num);
        let Ok(atom) = atom(&self.conn, &name) else { return false };
        self.conn.get_selection_owner(atom).ok().and_then(|c| c.reply().ok()).is_some_and(|r| r.owner != 0)
    }

    pub fn monitors(&self) -> Vec<Monitor> {
        let mut found = Vec::new();
        if let Ok(reply) = self.conn.randr_get_monitors(self.root, true).map_err(|e| e.to_string()).and_then(|c| c.reply().map_err(|e| e.to_string())) {
            for m in reply.monitors {
                if m.width == 0 || m.height == 0 {
                    continue;
                }
                let name = self.conn.get_atom_name(m.name).ok().and_then(|c| c.reply().ok()).map(|r| String::from_utf8_lossy(&r.name).to_string()).unwrap_or_default();
                found.push(Monitor { name, x: m.x as i32, y: m.y as i32, width: m.width as u32, height: m.height as u32 });
            }
        }
        if found.is_empty() {
            let screen = &self.conn.setup().roots[self.screen_num];
            found.push(Monitor { name: "screen".into(), x: 0, y: 0, width: screen.width_in_pixels as u32, height: screen.height_in_pixels as u32 });
        }
        found
    }

    /// Pixels to a point: from the desktop's own scaling setting.
    pub fn scale(&self) -> f32 {
        if let Some(value) = std::env::var("SNOWDRIFT_SCALE").ok().and_then(|v| v.parse::<f32>().ok()) {
            return value.clamp(1.0, 4.0);
        }
        let dpi = self
            .conn
            .get_property(false, self.root, self.atoms.resource_manager, AtomEnum::STRING, 0, 65536)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| {
                String::from_utf8_lossy(&r.value).lines().find_map(|line| line.strip_prefix("Xft.dpi:").and_then(|v| v.trim().parse::<f32>().ok()))
            });
        match dpi {
            Some(dpi) => ((dpi / 96.0) * 4.0).round().max(4.0) / 4.0,
            None => std::env::var("GDK_SCALE").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0).clamp(1.0, 4.0),
        }
    }

    /// The work area of the first workspace: x, y, width, height, less the panels.
    pub fn work_area(&self) -> Option<(i32, i32, u32, u32)> {
        let atom = atom(&self.conn, "_NET_WORKAREA").ok()?;
        let values = self.conn.get_property(false, self.root, atom, AtomEnum::CARDINAL, 0, 4).ok()?.reply().ok()?.value32()?.collect::<Vec<u32>>();
        match values[..] {
            [x, y, w, h] => Some((x as i32, y as i32, w, h)),
            _ => None,
        }
    }

    pub fn create_overlay_at(&mut self, x: i32, y: i32, width: u32, height: u32) -> Result<Overlay> {
        let window = self.conn.generate_id()?;
        let aux = CreateWindowAux::new()
            .override_redirect(1)
            .background_pixel(0)
            .border_pixel(0)
            .colormap(self.colormap)
            .event_mask(EventMask::NO_EVENT);
        self.conn.create_window(
            self.depth,
            window,
            self.root,
            x as i16,
            y as i16,
            width as u16,
            height as u16,
            0,
            WindowClass::INPUT_OUTPUT,
            self.visual,
            &aux,
        )?;
        // Clicks pass through to whatever is underneath: the window has no input region at all.
        self.conn.shape_rectangles(shape::SO::SET, shape::SK::INPUT, xproto::ClipOrdering::UNSORTED, window, 0, 0, &[])?;
        let title = b"Snowdrift";
        self.conn.change_property8(PropMode::REPLACE, window, AtomEnum::WM_NAME, AtomEnum::STRING, title)?;
        self.own.push(window);
        self.conn.flush()?;
        Ok(Overlay { window, shown: false })
    }

    pub fn show(&self, overlay: &mut Overlay) -> Result<()> {
        if !overlay.shown {
            self.conn.map_window(overlay.window)?;
            overlay.shown = true;
        }
        self.conn.configure_window(overlay.window, &xproto::ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
        Ok(())
    }

    pub fn hide(&self, overlay: &mut Overlay) -> Result<()> {
        if overlay.shown {
            self.conn.unmap_window(overlay.window)?;
            overlay.shown = false;
            self.conn.flush()?;
        }
        Ok(())
    }

    pub fn destroy(&self, overlay: &Overlay) {
        let _ = self.conn.destroy_window(overlay.window);
        let _ = self.conn.flush();
    }

    pub fn present(&self, overlay: &Overlay, canvas: &Canvas) -> Result<()> {
        let max = x11rb::connection::RequestConnection::maximum_request_bytes(&self.conn).saturating_sub(64);
        let stride = canvas.width * 4;
        let rows = (max / stride).clamp(1, canvas.height);
        let mut y = 0;
        while y < canvas.height {
            let n = rows.min(canvas.height - y);
            self.conn.put_image(
                ImageFormat::Z_PIXMAP,
                overlay.window,
                self.gc,
                canvas.width as u16,
                n as u16,
                0,
                y as i16,
                0,
                self.depth,
                &canvas.pixels[y * stride..(y + n) * stride],
            )?;
            y += n;
        }
        self.conn.flush()?;
        Ok(())
    }

    pub fn pointer(&self) -> Option<(i32, i32)> {
        let reply = self.conn.query_pointer(self.root).ok()?.reply().ok()?;
        Some((reply.root_x as i32, reply.root_y as i32))
    }

    /// Whether anything the windows depend on changed since the last call, without blocking.
    pub fn drain_events(&self) -> bool {
        let mut changed = false;
        while let Ok(Some(_)) = self.conn.poll_for_event() {
            changed = true;
        }
        changed
    }

    fn cardinals(&self, window: u32, property: u32) -> Vec<u32> {
        self.conn
            .get_property(false, window, property, AtomEnum::CARDINAL, 0, 16)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(|v| v.collect()))
            .unwrap_or_default()
    }

    fn atoms_of(&self, window: u32, property: u32) -> Vec<u32> {
        self.conn
            .get_property(false, window, property, AtomEnum::ATOM, 0, 64)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(|v| v.collect()))
            .unwrap_or_default()
    }

    /// The windows, bottom to top, with their outer frames in root pixels.
    pub fn clients(&self) -> Vec<Client> {
        let list = |property| -> Vec<u32> {
            self.conn
                .get_property(false, self.root, property, AtomEnum::WINDOW, 0, 8192)
                .ok()
                .and_then(|c| c.reply().ok())
                .and_then(|r| r.value32().map(|v| v.collect()))
                .unwrap_or_default()
        };
        let mut ids = list(self.atoms.client_list_stacking);
        if ids.is_empty() {
            ids = list(self.atoms.client_list);
        }
        if ids.is_empty() {
            return self.top_levels();
        }
        let current = self.cardinals(self.root, self.atoms.current_desktop).first().copied().unwrap_or(0);
        let mut found = Vec::new();
        // Everything is asked for at once, then read: one round trip for the lot.
        struct Pending<'a> {
            id: u32,
            geometry: x11rb::cookie::Cookie<'a, RustConnection, xproto::GetGeometryReply>,
            origin: x11rb::cookie::Cookie<'a, RustConnection, xproto::TranslateCoordinatesReply>,
            attributes: x11rb::cookie::Cookie<'a, RustConnection, xproto::GetWindowAttributesReply>,
        }
        let mut pending = Vec::new();
        for &id in &ids {
            if self.own.contains(&id) {
                continue;
            }
            let (Ok(geometry), Ok(origin), Ok(attributes)) =
                (self.conn.get_geometry(id), self.conn.translate_coordinates(id, self.root, 0, 0), self.conn.get_window_attributes(id))
            else {
                continue;
            };
            pending.push(Pending { id, geometry, origin, attributes });
        }
        for p in pending {
            let (Ok(geometry), Ok(origin), Ok(attributes)) = (p.geometry.reply(), p.origin.reply(), p.attributes.reply()) else { continue };
            if attributes.map_state != MapState::VIEWABLE {
                continue;
            }
            let state = self.atoms_of(p.id, self.atoms.wm_state);
            if state.contains(&self.atoms.state_hidden) {
                continue;
            }
            let kinds = self.atoms_of(p.id, self.atoms.window_type);
            let a = &self.atoms;
            let solid = kinds.is_empty()
                || kinds.iter().any(|k| [a.type_normal, a.type_dialog, a.type_utility, a.type_toolbar, a.type_splash].contains(k));
            if !solid {
                continue;
            }
            let desktop = self.cardinals(p.id, self.atoms.wm_desktop).first().copied();
            let elsewhere = desktop.is_some_and(|d| d != 0xFFFF_FFFF && d != current);
            let (mut left, mut right, mut top, mut bottom) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            if let [l, r, t, b] = self.cardinals(p.id, self.atoms.frame_extents)[..] {
                (left, right, top, bottom) = (l as f32, r as f32, t as f32, b as f32);
            }
            let mut radius = 8.0;
            // A client-drawn window reserves an invisible margin for its shadow: the window is only what is inside it.
            let (mut cl, mut cr, mut ct, mut cb) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            if let [l, r, t, b] = self.cardinals(p.id, self.atoms.gtk_frame_extents)[..] {
                (cl, cr, ct, cb) = (l as f32, r as f32, t as f32, b as f32);
                radius = 12.0;
            }
            let x = origin.dst_x as f32 - left + cl;
            let y = origin.dst_y as f32 - top + ct;
            let width = geometry.width as f32 + left + right - cl - cr;
            let height = geometry.height as f32 + top + bottom - ct - cb;
            let maximized = state.contains(&self.atoms.state_max_h) && state.contains(&self.atoms.state_max_v);
            let fullscreen = state.contains(&self.atoms.state_fullscreen);
            found.push(Client {
                id: p.id,
                x,
                y,
                width,
                height,
                maximized,
                fullscreen,
                elsewhere,
                radius: if maximized || fullscreen { 0.0 } else { radius },
            });
        }
        found
    }

    /// Without a window manager that lists its windows: the top-level windows on screen, frames and all.
    fn top_levels(&self) -> Vec<Client> {
        let Ok(tree) = self.conn.query_tree(self.root).map_err(|e| e.to_string()).and_then(|c| c.reply().map_err(|e| e.to_string())) else { return Vec::new() };
        let mut found = Vec::new();
        for id in tree.children {
            if self.own.contains(&id) {
                continue;
            }
            let (Ok(attributes), Ok(geometry)) =
                (self.conn.get_window_attributes(id).map_err(|e| e.to_string()).and_then(|c| c.reply().map_err(|e| e.to_string())), self.conn.get_geometry(id).map_err(|e| e.to_string()).and_then(|c| c.reply().map_err(|e| e.to_string())))
            else {
                continue;
            };
            if attributes.map_state != MapState::VIEWABLE || attributes.class != WindowClass::INPUT_OUTPUT || attributes.override_redirect {
                continue;
            }
            found.push(Client {
                id,
                x: geometry.x as f32,
                y: geometry.y as f32,
                width: (geometry.width + 2 * geometry.border_width) as f32,
                height: (geometry.height + 2 * geometry.border_width) as f32,
                maximized: false,
                fullscreen: false,
                elsewhere: false,
                radius: 6.0,
            });
        }
        found
    }
}
