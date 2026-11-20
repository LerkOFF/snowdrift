//! Snowdrift for Linux: snowfall over every window, on X11 (and Wayland sessions, through XWayland).

mod canvas;
mod config;
mod control;
mod install;
mod panels;
mod render;
mod update;
mod x11;

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use snowdrift_core::geometry::{Rect, V2};
use snowdrift_core::simulation::{Pace, SnowSimulation, WindowSurface};

use canvas::Canvas;
use config::Settings;
use x11::{Client, Display, Monitor, Overlay};

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// A cursor far off every display.
const AWAY: V2 = V2 { x: -10_000.0, y: -10_000.0 };

const HELP: &str = "Snowdrift: snowfall over all your windows.

Usage: snowdrift [command]

  run                    start the snowfall in this terminal (what the launcher entry does)
  start                  start it in the background
  stop                   quit the running one
  plow                   send the snowplow along the bottom of the screen
  clear                  blow the snow away
  set <name> <value>     change a setting: intensity 0-100, opacity 10-100, ground on|off, melt on|off,
                         windows on|off, fullscreen on|off, panels on|off, garland on|off, snowmen on|off,
                         frost on|off, newyear on|off (all three at once), mode snow|autumn|rain|blossom, mixed on|off
  snowmen clear          burst every snowman and ball
  status                 what is running and how it is set
  autostart on|off       start at login
  update                 check for a newer version, download it, replace this one and restart
  install                put the program, launcher entry and icon in your home folder
  uninstall              take them out again
  --version              the version";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let code = match args[..] {
        [] | ["run"] => run(),
        ["start"] => start(),
        ["stop"] => ask("stop"),
        ["plow"] => ask("plow"),
        ["clear"] => ask("clear"),
        ["snowmen", "clear"] => ask("snowmen clear"),
        ["status"] => status(),
        ["set", name, value] => ask(&format!("set {name} {value}")),
        ["autostart", on @ ("on" | "off")] => report(install::autostart(on == "on")),
        ["update"] => match update::update(VERSION) {
            Ok(text) => {
                println!("{text}");
                0
            }
            Err(error) => fail(&error),
        },
        ["install"] => match install::install() {
            Ok(path) => {
                println!("Installed to {}. Find Snowdrift in the application menu, or run `snowdrift start`.", path.display());
                0
            }
            Err(error) => fail(&error),
        },
        ["uninstall"] => {
            let _ = control::send("stop");
            report(install::uninstall())
        }
        ["desktop-entry", exec] => {
            print!("{}", install::desktop_entry(exec));
            0
        }
        ["--version" | "-V" | "version"] => {
            println!("Snowdrift {VERSION}");
            0
        }
        ["--help" | "-h" | "help"] => {
            println!("{HELP}");
            0
        }
        _ => {
            eprintln!("{HELP}");
            2
        }
    };
    std::process::exit(code);
}

fn fail(text: &str) -> i32 {
    eprintln!("snowdrift: {text}");
    1
}

fn report(result: Result<(), String>) -> i32 {
    result.map_or_else(|e| fail(&e), |()| 0)
}

fn ask(command: &str) -> i32 {
    match control::send(command) {
        Some(answer) if answer.starts_with("error") => fail(answer.trim_start_matches("error: ")),
        Some(answer) => {
            if !answer.is_empty() && answer != "ok" {
                println!("{answer}");
            }
            0
        }
        None => fail("Snowdrift isn't running; start it with `snowdrift start`"),
    }
}

fn status() -> i32 {
    match control::send("status") {
        Some(text) => {
            println!("{text}");
            0
        }
        None => {
            println!("Snowdrift isn't running");
            1
        }
    }
}

fn start() -> i32 {
    if control::send("ping").is_some() {
        println!("Snowdrift is already running");
        return 0;
    }
    let Ok(exe) = std::env::current_exe() else { return fail("can't find my own file") };
    let mut command = if which("setsid") { std::process::Command::new("setsid") } else { std::process::Command::new(&exe) };
    if which("setsid") {
        command.arg(&exe);
    }
    let spawned = command.arg("run").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
    match spawned {
        Ok(_) => {
            for _ in 0..30 {
                std::thread::sleep(Duration::from_millis(100));
                if control::send("ping").is_some() {
                    return 0;
                }
            }
            fail("it didn't start; run `snowdrift run` in a terminal to see why")
        }
        Err(error) => fail(&format!("can't start: {error}")),
    }
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

struct Screen {
    monitor: Monitor,
    /// The part of the monitor the snow covers: all of it, less a panel along the bottom.
    area: (i32, i32, u32, u32),
    overlay: Overlay,
    canvas: Canvas,
    simulation: SnowSimulation,
    frame: render::Frame,
    surfaces: Vec<WindowSurface>,
    last_step: Option<Instant>,
    next_due: Instant,
    last_draw: Instant,
    blank: bool,
    hidden: bool,
    /// The overlay is taking clicks instead of letting them through.
    catching: bool,
}

fn new_simulation(width: f32, height: f32, settings: &Settings) -> SnowSimulation {
    let mut simulation = SnowSimulation::new(width, height, settings.intensity as f32 / 100.0);
    simulation.auto_plow = true;
    simulation.walls = (true, true);
    apply(&mut simulation, settings);
    simulation.set_modes(&settings.picked.modes());
    simulation
}

fn apply(simulation: &mut SnowSimulation, settings: &Settings) {
    simulation.thaws = settings.melt;
    simulation.accumulates = settings.accumulates();
    simulation.ground_settles = settings.ground;
    simulation.falling_opacity = settings.opacity as f32 / 100.0;
    simulation.garland = settings.garland;
    simulation.window_snow = settings.snow_on_windows;
    simulation.snowmen.set_enabled(settings.snowmen);
    simulation.frost.set_enabled(settings.frost);
    simulation.set_intensity(settings.intensity as f32 / 100.0);
}

fn run() -> i32 {
    let server = match control::Server::bind() {
        Ok(server) => server,
        Err(error) => return fail(&error),
    };
    let mut display = match Display::open() {
        Ok(display) => display,
        Err(error) => return fail(&format!("can't open the display: {error}")),
    };
    if !display.has_compositor() {
        return fail(
            "no compositing manager is running, so snow can't be drawn see-through. \
Turn on compositing in your desktop's settings, or start one (for example `picom -b`), then run Snowdrift again.",
        );
    }
    let mut settings = Settings::load();
    let started = Instant::now();
    let scale = display.scale();
    let mut monitors = display.monitors();
    let mut inset = bottom_inset(&display, &monitors);
    let mut screens = match make_screens(&mut display, &monitors, &inset, scale, &settings) {
        Ok(screens) => screens,
        Err(error) => return fail(&format!("can't create the overlay: {error}")),
    };
    eprintln!("Snowdrift {VERSION}: {} display(s), scale {scale}", screens.len());

    let mut clients: Vec<Client> = Vec::new();
    let mut known: HashSet<u32> = HashSet::new();
    let mut windows_at = Instant::now() - Duration::from_secs(1);
    let mut panels_at = Instant::now();
    panels::set_enabled(settings.panels && settings.snow_on_windows);
    let mut changed_at = Instant::now();
    let mut monitors_at = Instant::now();
    let mut quit = false;

    while !quit {
        let now = Instant::now();
        for (command, stream) in server.poll() {
            let mut words = command.splitn(3, ' ');
            let answer = match (words.next(), words.next(), words.next()) {
                (Some("ping"), _, _) => "pong".to_string(),
                (Some("stop"), _, _) => {
                    quit = true;
                    "ok".into()
                }
                (Some("plow"), _, _) => {
                    screens.iter_mut().for_each(|s| s.simulation.start_plow());
                    "ok".into()
                }
                (Some("snowmen"), Some("clear"), _) => {
                    screens.iter_mut().for_each(|s| s.simulation.clear_snowmen());
                    "ok".into()
                }
                (Some("clear"), _, _) => {
                    screens.iter_mut().for_each(|s| s.simulation.blow_away(1.0, false));
                    "ok".into()
                }
                (Some("set"), Some(name), Some(value)) => match settings.set(name, value) {
                    Ok(()) => {
                        settings.save();
                        panels::set_enabled(settings.panels && settings.snow_on_windows);
                        windows_at = now - Duration::from_secs(1);
                        clients.clear();
                        for screen in &mut screens {
                            apply(&mut screen.simulation, &settings);
                            screen.simulation.switch_modes(&settings.picked.modes(), 1.0, false);
                        }
                        "ok".into()
                    }
                    Err(error) => format!("error: {error}"),
                },
                (Some("status"), _, _) => format!(
                    "Snowdrift {VERSION}, {} display(s), scale {scale}, {}",
                    screens.len(),
                    settings.to_text().lines().filter(|l| !l.starts_with("mix=")).collect::<Vec<_>>().join(", ")
                ),
                _ => "error: unknown command".into(),
            };
            control::answer(stream, &answer);
        }
        if display.drain_events() {
            windows_at = now - Duration::from_secs(1);
        }

        // The displays can change under us: a monitor plugged in, a panel moved.
        if now.duration_since(monitors_at) > Duration::from_secs(2) {
            monitors_at = now;
            let fresh = display.monitors();
            let fresh_inset = bottom_inset(&display, &fresh);
            if fresh != monitors || fresh_inset != inset {
                for screen in &screens {
                    display.destroy(&screen.overlay);
                }
                monitors = fresh;
                inset = fresh_inset;
                match make_screens(&mut display, &monitors, &inset, scale, &settings) {
                    Ok(made) => screens = made,
                    Err(error) => return fail(&format!("can't create the overlay: {error}")),
                }
                clients.clear();
                windows_at = now - Duration::from_secs(1);
            }
        }

        // The windows: read about thirty times a second, and handed to the snow when they moved.
        if (settings.snow_on_windows || settings.garland) && now.duration_since(windows_at) >= Duration::from_millis(33) {
            windows_at = now;
            let fresh = display.clients();
            // The process in front tells the panel reader whose tree to look at.
            if panels::enabled() && now.duration_since(panels_at) >= Duration::from_millis(250) {
                panels_at = now;
                panels::set_active_pid(display.active_pid().unwrap_or(0));
            }
            let panels_changed = panels::take_changed();
            if fresh != clients || panels_changed {
                clients = fresh;
                let interval = now.duration_since(changed_at).as_secs_f32().clamp(1.0 / 120.0, 0.25);
                changed_at = now;
                let open: HashSet<u32> = clients.iter().map(|c| c.id).collect();
                let away: HashSet<isize> = clients.iter().filter(|c| c.elsewhere).map(|c| c.id as isize).collect();
                let inside = if panels::enabled() { panels::all() } else { Vec::new() };
                for screen in &mut screens {
                    let (surfaces, covers) = surfaces_for(&clients, &inside, screen, scale);
                    if std::env::var_os("SNOWDRIFT_DEBUG").is_some() {
                        eprintln!("surfaces: {} panels: {} clients: {:?}", surfaces.len(), inside.len(), clients.iter().map(|c| (c.id, c.x, c.y, c.width, c.height)).collect::<Vec<_>>());
                    }
                    screen.simulation.covers = covers;
                    screen.simulation.forget_closed(|id| open.contains(&(id as u32)));
                    if surfaces != screen.surfaces {
                        let gone: HashSet<isize> = away.clone();
                        screen.simulation.update_windows(&surfaces, interval, &HashSet::new(), &gone, false);
                        screen.surfaces = surfaces;
                    }
                }
                known = open;
            }
        } else if !(settings.snow_on_windows || settings.garland) && !known.is_empty() {
            known.clear();
            clients.clear();
            for screen in &mut screens {
                screen.simulation.update_windows(&[], 0.1, &HashSet::new(), &HashSet::new(), false);
                screen.simulation.covers.clear();
                screen.surfaces.clear();
            }
        }

        let (pointer, pressed) = match display.pointer_state() {
            Some((x, y, down)) => (Some((x, y)), down),
            None => (None, false),
        };
        let time = started.elapsed().as_secs_f32();
        let mut soonest = now + Duration::from_millis(250);
        for screen in &mut screens {
            // A full-screen app has the display: the snow steps out of its way.
            let covered = !settings.over_full_screen && covers_display(&clients, screen);
            if covered {
                if !screen.hidden {
                    let _ = display.hide(&mut screen.overlay);
                    screen.hidden = true;
                }
                screen.last_step = None;
                soonest = soonest.min(now + Duration::from_millis(200));
                continue;
            }
            if now < screen.next_due {
                soonest = soonest.min(screen.next_due);
                continue;
            }
            let dt = screen.last_step.map_or(1.0 / 60.0, |t| now.duration_since(t).as_secs_f32().min(0.05));
            screen.last_step = Some(now);
            let cursor = pointer
                .map(|(x, y)| V2::new((x - screen.area.0) as f32 / scale, (screen.area.1 + screen.area.3 as i32 - y) as f32 / scale))
                .unwrap_or(AWAY);
            screen.simulation.pointer_down = pressed;
            let changed = screen.simulation.step(dt, cursor);
            // Over a ball of snow, or holding one, the overlay takes the clicks; elsewhere they go through.
            let wants = screen.simulation.snowmen.wants_pointer;
            if wants != screen.catching {
                screen.catching = wants;
                let _ = display.catch_clicks(&screen.overlay, screen.area.2, screen.area.3, wants);
            }
            if !changed && screen.simulation.is_empty() {
                if !screen.blank {
                    screen.canvas.clear();
                    let _ = display.present(&screen.overlay, &screen.canvas);
                    screen.blank = true;
                }
                screen.last_step = None;
                screen.next_due = now + Duration::from_millis(250);
                soonest = soonest.min(screen.next_due);
                continue;
            }
            let rate = match screen.simulation.pace() {
                Pace::Calm => 30,
                Pace::Lively => 45,
                Pace::Interactive => 60,
            };
            screen.next_due = now + Duration::from_micros(1_000_000 / rate);
            soonest = soonest.min(screen.next_due);
            // A still drift only needs a slow refresh for its glints.
            if changed || screen.blank || now.duration_since(screen.last_draw) > Duration::from_millis(500) {
                screen.frame.draw(&mut screen.canvas, &mut screen.simulation, scale, time);
                if screen.hidden || !screen.overlay.shown {
                    let _ = display.show(&mut screen.overlay);
                    screen.hidden = false;
                } else if changed {
                    // Whatever popped up since stays under the snow.
                    let _ = display.show(&mut screen.overlay);
                }
                if let Err(error) = display.present(&screen.overlay, &screen.canvas) {
                    eprintln!("snowdrift: can't draw: {error}");
                    return 1;
                }
                screen.blank = false;
                screen.last_draw = now;
            }
        }
        let wait = soonest.saturating_duration_since(Instant::now()).min(Duration::from_millis(33));
        std::thread::sleep(wait.max(Duration::from_millis(1)));
    }
    for screen in &screens {
        display.destroy(&screen.overlay);
    }
    0
}

fn make_screens(display: &mut Display, monitors: &[Monitor], inset: &HashMap<usize, u32>, scale: f32, settings: &Settings) -> x11::Result<Vec<Screen>> {
    let mut screens = Vec::new();
    for (index, monitor) in monitors.iter().enumerate() {
        let cut = inset.get(&index).copied().unwrap_or(0);
        let area = (monitor.x, monitor.y, monitor.width, monitor.height.saturating_sub(cut).max(1));
        let overlay = display.create_overlay_at(area.0, area.1, area.2, area.3)?;
        let simulation = new_simulation(area.2 as f32 / scale, area.3 as f32 / scale, settings);
        screens.push(Screen {
            monitor: monitor.clone(),
            area,
            overlay,
            canvas: Canvas::new(area.2 as usize, area.3 as usize),
            simulation,
            frame: render::Frame::new(),
            surfaces: Vec::new(),
            last_step: None,
            next_due: Instant::now(),
            last_draw: Instant::now(),
            blank: false,
            hidden: false,
            catching: false,
        });
    }
    Ok(screens)
}

/// How far a panel along the bottom takes from a display, so the snow lies on top of it. Read from the work area, which
/// the desktop keeps for one workspace as a whole: it can only be told apart per display with a single display.
fn bottom_inset(display: &Display, monitors: &[Monitor]) -> HashMap<usize, u32> {
    let mut inset = HashMap::new();
    if let [monitor] = monitors
        && let Some((_, y, _, height)) = display.work_area()
    {
        let bottom = monitor.y + monitor.height as i32;
        let cut = bottom - (y + height as i32);
        if cut > 0 && (cut as u32) < monitor.height / 3 {
            inset.insert(0, cut as u32);
        }
    }
    inset
}

/// The windows on one display as ledges for the snow, front to back with what hides what, and the frames of all of
/// them for the pumpkin-free "covers" list.
fn surfaces_for(clients: &[Client], inside: &[panels::Panel], screen: &Screen, scale: f32) -> (Vec<WindowSurface>, Vec<[f32; 4]>) {
    let (ax, ay, aw, ah) = screen.area;
    let (width, height) = (aw as f32 / scale, ah as f32 / scale);
    let local = |c: &Client| Rect {
        x: (c.x - ax as f32) / scale,
        y: ((ay + ah as i32) as f32 - (c.y + c.height)) / scale,
        width: c.width / scale,
        height: c.height / scale,
    };
    let overlaps = |r: &Rect| r.max_x() > 0.0 && r.min_x() < width && r.max_y() > 0.0 && r.min_y() < height;
    let touches = |a: &Rect, b: &Rect| a.min_x() < b.max_x() && a.max_x() > b.min_x() && a.min_y() < b.max_y() && a.max_y() > b.min_y();
    // Tiled windows stand flush against each other: one with another right above it has no top edge for snow to lie on.
    let frames: Vec<(u32, Rect)> = clients.iter().filter(|c| !c.elsewhere).map(|c| (c.id, local(c))).collect();
    let boxed_in = |id: u32, frame: &Rect| {
        frames.iter().any(|(other, above)| {
            let shared = (above.max_x().min(frame.max_x()) - above.min_x().max(frame.min_x())).max(0.0);
            *other != id && shared >= frame.width * 0.5 && above.min_y() >= frame.max_y() - 2.0 && above.min_y() <= frame.max_y() + 6.0
        })
    };
    let mut blockers: Vec<Rect> = Vec::new();
    let mut surfaces = Vec::new();
    let mut covers = Vec::new();
    for client in clients.iter().rev().filter(|c| !c.elsewhere) {
        let frame = local(client);
        if !overlaps(&frame) {
            continue;
        }
        covers.push([frame.min_x(), frame.min_y(), frame.max_x(), frame.max_y()]);
        let fills = frame.width * frame.height >= width * height * 0.9;
        // A window lying across displays carries its snow on the one most of it is on.
        let on_this = owner(clients, client, screen) ;
        let room_above = (client.y - ay as f32) / scale >= 6.0 && frame.max_y() < height;
        // Panels inside the window in front that holds them: each catches snow on its top edge. The snow falls through
        // the window's own edge above them, down inside the window, so that edge has a gap over each.
        let mut held: Vec<(panels::Kind, Rect)> = Vec::new();
        if on_this && !fills && !inside.is_empty() && is_front_holder(clients, client, inside) {
            for panel in inside.iter().filter(|p| holds(client, p)).take(6) {
                let outer = Rect {
                    x: (panel.x - ax as f32) / scale,
                    y: ((ay + ah as i32) as f32 - (panel.y + panel.height)) / scale,
                    width: panel.width / scale,
                    height: panel.height / scale,
                };
                if overlaps(&outer) && (panel.y - ay as f32) / scale >= 6.0 && outer.max_y() < height {
                    held.push((panel.kind, outer));
                }
            }
        }
        let shafts: Vec<Rect> = held
            .iter()
            .map(|(_, outer)| Rect { x: outer.x, y: outer.max_y() - 1.0, width: outer.width, height: frame.max_y() - outer.max_y() + 6.0 })
            .collect();
        if on_this && room_above && !boxed_in(client.id, &frame) && !fills && frame.width >= 80.0 && frame.height >= 40.0 {
            let radius = (client.radius / scale).min(frame.width / 2.0).min(frame.height / 2.0);
            let region = Rect { x: frame.x, y: frame.max_y() - radius, width: frame.width, height: 40.0 + radius };
            let mut hiding: Vec<Rect> = blockers.iter().filter(|b| touches(b, &region)).copied().collect();
            hiding.extend(shafts.iter().copied());
            surfaces.push(WindowSurface {
                id: client.id as isize,
                frame,
                blockers: hiding,
                cover: blockers.iter().filter(|b| touches(b, &frame)).copied().collect(),
                corner_radius: radius,
                corner_exponent: 2.0,
            });
        }
        for (index, (kind, outer)) in held.iter().enumerate() {
            let radius = (if *kind == panels::Kind::Dialog { 8.0 } else { 6.0f32 }).min(outer.width / 2.0).min(outer.height / 2.0);
            let region = Rect { x: outer.x, y: outer.max_y() - radius, width: outer.width, height: 40.0 + radius };
            surfaces.push(WindowSurface {
                id: (client.id as isize) << 8 | (index as isize + 1),
                frame: *outer,
                blockers: blockers.iter().filter(|b| touches(b, &region)).copied().collect(),
                cover: blockers.iter().filter(|b| touches(b, outer)).copied().collect(),
                corner_radius: radius,
                corner_exponent: 2.0,
            });
            blockers.push(*outer);
        }
        blockers.push(frame);
    }
    (surfaces, covers)
}

/// Whether the panel lies wholly inside the window's frame.
fn holds(client: &Client, p: &panels::Panel) -> bool {
    p.x >= client.x - 2.0 && p.y >= client.y - 2.0 && p.x + p.width <= client.x + client.width + 2.0 && p.y + p.height <= client.y + client.height + 2.0
}

/// Whether the window is the front one among those holding the panels, which is the one they were read from.
fn is_front_holder(clients: &[Client], client: &Client, inside: &[panels::Panel]) -> bool {
    clients.iter().rev().filter(|c| !c.elsewhere).find(|c| inside.iter().any(|p| holds(c, p))).is_some_and(|c| c.id == client.id)
}

/// Whether this display is the one a window's snow lies on: where most of the window is.
fn owner(_clients: &[Client], client: &Client, screen: &Screen) -> bool {
    let (ax, ay, aw, ah) = screen.area;
    let (m, c) = (&screen.monitor, client);
    let overlap = |x: f32, y: f32, w: f32, h: f32| ((x + w).min(c.x + c.width) - x.max(c.x)).max(0.0) * ((y + h).min(c.y + c.height) - y.max(c.y)).max(0.0);
    let here = overlap(ax as f32, ay as f32, aw as f32, ah as f32);
    let full = overlap(m.x as f32, m.y as f32, m.width as f32, m.height as f32);
    // Without knowing the neighbours, a window belongs here if at least half of what it has on this monitor is on it.
    here * 2.0 >= full || (c.x + c.width / 2.0 >= m.x as f32 && c.x + c.width / 2.0 < (m.x + m.width as i32) as f32)
}

/// A full-screen window is the front one on this display.
fn covers_display(clients: &[Client], screen: &Screen) -> bool {
    let m = &screen.monitor;
    clients
        .iter()
        .rev()
        .filter(|c| !c.elsewhere)
        .find(|c| c.x < (m.x + m.width as i32) as f32 && c.x + c.width > m.x as f32 && c.y < (m.y + m.height as i32) as f32 && c.y + c.height > m.y as f32)
        .is_some_and(|c| c.fullscreen && c.width >= m.width as f32 - 1.0 && c.height >= m.height as f32 - 1.0)
}
