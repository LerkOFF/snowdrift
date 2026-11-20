//! Settings, kept as `key=value` lines in `~/.config/snowdrift/config`.

use std::path::PathBuf;

use snowdrift_core::modes::Picked;
use snowdrift_core::simulation::Mode;

#[derive(Clone)]
pub struct Settings {
    /// How much falls, in percent: 0 to 100.
    pub intensity: u32,
    /// How opaque what falls is drawn, in percent: 10 to 100.
    pub opacity: u32,
    pub ground: bool,
    pub melt: bool,
    pub snow_on_windows: bool,
    /// The snow stays over full-screen apps instead of stepping out of their way.
    pub over_full_screen: bool,
    /// Snow also settles on dialogs and big pictures inside windows, read through the accessibility bus.
    pub panels: bool,
    /// A string of lights along the top of every window.
    pub garland: bool,
    /// Balls of snow to roll up and stack into snowmen.
    pub snowmen: bool,
    /// Frost creeping in from the edges of the screen, wiped by the pointer.
    pub frost: bool,
    pub picked: Picked,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            intensity: 50,
            opacity: 100,
            ground: true,
            melt: true,
            snow_on_windows: true,
            over_full_screen: false,
            panels: false,
            garland: false,
            snowmen: false,
            frost: false,
            picked: Picked::load(None, None, None, [None, None]),
        }
    }
}

pub const KEYS: [&str; 13] = ["intensity", "opacity", "ground", "melt", "windows", "fullscreen", "panels", "garland", "snowmen", "frost", "newyear", "mode", "mixed"];

pub fn dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("snowdrift")
}

fn path() -> PathBuf {
    dir().join("config")
}

pub fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Snow => "snow",
        Mode::Autumn => "autumn",
        Mode::Rain => "rain",
        Mode::Blossom => "blossom",
    }
}

fn mode_from(name: &str) -> Option<Mode> {
    Mode::ALL.into_iter().find(|&m| mode_name(m) == name)
}

fn flag(value: &str) -> Option<bool> {
    match value.trim().to_lowercase().as_str() {
        "1" | "on" | "yes" | "true" => Some(true),
        "0" | "off" | "no" | "false" => Some(false),
        _ => None,
    }
}

impl Settings {
    pub fn load() -> Settings {
        let mut settings = Settings::default();
        let Ok(text) = std::fs::read_to_string(path()) else { return settings };
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('=') {
                let _ = settings.set(key.trim(), value.trim());
            }
        }
        settings
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(dir());
        let _ = std::fs::write(path(), self.to_text());
    }

    pub fn accumulates(&self) -> bool {
        self.ground || self.snow_on_windows
    }

    pub fn to_text(&self) -> String {
        let on = |v: bool| if v { "on" } else { "off" };
        format!(
            "intensity={}\nopacity={}\nground={}\nmelt={}\nwindows={}\nfullscreen={}\npanels={}\ngarland={}\nsnowmen={}\nfrost={}\nmode={}\nmixed={}\nmix={},{}\n",
            self.intensity,
            self.opacity,
            on(self.ground),
            on(self.melt),
            on(self.snow_on_windows),
            on(self.over_full_screen),
            on(self.panels),
            on(self.garland),
            on(self.snowmen),
            on(self.frost),
            mode_name(self.picked.mode),
            on(self.picked.mixed),
            mode_name(self.picked.mix[0]),
            mode_name(self.picked.mix[1]),
        )
    }

    /// Changes one setting by name; says what was wrong with the value if it can't.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let number = |min: u32, max: u32| value.parse::<u32>().ok().filter(|v| (min..=max).contains(v)).ok_or_else(|| format!("{key} is a number from {min} to {max}"));
        let switch = || flag(value).ok_or_else(|| format!("{key} is on or off"));
        match key {
            "intensity" => self.intensity = number(0, 100)?,
            "opacity" => self.opacity = number(10, 100)?,
            "ground" => self.ground = switch()?,
            "melt" => self.melt = switch()?,
            "windows" => self.snow_on_windows = switch()?,
            "fullscreen" => self.over_full_screen = switch()?,
            "panels" => self.panels = switch()?,
            "garland" => self.garland = switch()?,
            "snowmen" => self.snowmen = switch()?,
            "frost" => self.frost = switch()?,
            "newyear" => {
                let on = switch()?;
                self.garland = on;
                self.snowmen = on;
                self.frost = on;
            }
            "mode" => {
                let mode = mode_from(value).ok_or("mode is snow, autumn, rain or blossom")?;
                self.picked.pick(mode);
            }
            "mixed" => {
                let want = switch()?;
                if want != self.picked.mixed {
                    self.picked.toggle_mixed();
                }
            }
            "mix" => {
                let names: Vec<&str> = value.split(',').collect();
                if let [a, b] = names[..]
                    && let (Some(a), Some(b)) = (mode_from(a.trim()), mode_from(b.trim()))
                    && a != b
                {
                    self.picked.mix = [a, b];
                }
            }
            _ => return Err(format!("unknown setting {key}; known: {}", KEYS.join(", "))),
        }
        Ok(())
    }
}
