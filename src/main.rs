#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use rand::Rng;
use std::path::PathBuf;
use std::time::{Duration, Instant};

// ─── Colors (matching Python theme) ───────────────────────────────────────────
const BG_DARK: egui::Color32 = egui::Color32::from_rgb(0x01, 0x04, 0x21);
const BG_WIDGET: egui::Color32 = egui::Color32::from_rgb(0x0D, 0x11, 0x2B);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(0xFC, 0x03, 0x5E);
const ACCENT_HOVER: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x33, 0x7E);
const TEXT_WHITE: egui::Color32 = egui::Color32::WHITE;
const TEXT_DIM: egui::Color32 = egui::Color32::from_rgb(0xAA, 0xAA, 0xAA);
const DISABLED_BORDER: egui::Color32 = egui::Color32::from_rgb(0x55, 0x55, 0x55);
const CLEAR_RED: egui::Color32 = egui::Color32::from_rgb(0xAB, 0x00, 0x00);
const BTN_SECONDARY: egui::Color32 = egui::Color32::from_rgb(0x33, 0x33, 0x33);
const RESET_GREEN: egui::Color32 = egui::Color32::from_rgb(0x1B, 0x8A, 0x2A);
const ERR_RED: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x55, 0x55);

// ─── Config persistence ──────────────────────────────────────────────────────
fn config_path() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    exe_dir.join("config.ini")
}

#[derive(Clone)]
struct Config {
    interval: String,
    timer_count: String,
    sound_file: String,
    final_sound_file: String,
    progress_bar_mode: bool,
    geometric_enabled: bool,
    geometric_ratio: String,
    randomize_enabled: bool,
    randomize_gaussian: bool,
    randomize_sigma: String,
}

impl Config {
    fn load() -> Self {
        let path = config_path();
        if path.exists() {
            let mut conf = configparser::ini::Ini::new();
            if conf.load(path.to_string_lossy().as_ref()).is_ok() {
                return Self {
                    interval: conf.get("Settings", "interval").unwrap_or_default(),
                    timer_count: conf.get("Settings", "timer_count").unwrap_or_default(),
                    sound_file: conf.get("Settings", "sound_file").unwrap_or_default(),
                    final_sound_file: conf.get("Settings", "final_sound_file").unwrap_or_default(),
                    progress_bar_mode: conf
                        .get("Settings", "progress_bar_mode")
                        .unwrap_or_default()
                        .eq_ignore_ascii_case("true"),
                    geometric_enabled: conf
                        .get("Settings", "geometric_enabled")
                        .unwrap_or_default()
                        .eq_ignore_ascii_case("true"),
                    geometric_ratio: conf.get("Settings", "geometric_ratio").unwrap_or_default(),
                    randomize_enabled: conf
                        .get("Settings", "randomize_enabled")
                        .unwrap_or_default()
                        .eq_ignore_ascii_case("true"),
                    randomize_gaussian: conf
                        .get("Settings", "randomize_gaussian")
                        .unwrap_or_default()
                        .eq_ignore_ascii_case("true"),
                    randomize_sigma: conf.get("Settings", "randomize_sigma").unwrap_or_default(),
                };
            }
        }
        Self::default()
    }

    fn save(&self) {
        let mut conf = configparser::ini::Ini::new();
        conf.set("Settings", "interval", Some(self.interval.clone()));
        conf.set("Settings", "timer_count", Some(self.timer_count.clone()));
        conf.set("Settings", "sound_file", Some(self.sound_file.clone()));
        conf.set("Settings", "final_sound_file", Some(self.final_sound_file.clone()));
        conf.set(
            "Settings",
            "progress_bar_mode",
            Some(if self.progress_bar_mode { "True" } else { "False" }.to_string()),
        );
        conf.set(
            "Settings",
            "geometric_enabled",
            Some(if self.geometric_enabled { "True" } else { "False" }.to_string()),
        );
        conf.set("Settings", "geometric_ratio", Some(self.geometric_ratio.clone()));
        conf.set(
            "Settings",
            "randomize_enabled",
            Some(if self.randomize_enabled { "True" } else { "False" }.to_string()),
        );
        conf.set(
            "Settings",
            "randomize_gaussian",
            Some(if self.randomize_gaussian { "True" } else { "False" }.to_string()),
        );
        conf.set("Settings", "randomize_sigma", Some(self.randomize_sigma.clone()));
        let _ = conf.write(config_path().to_string_lossy().as_ref());
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            interval: String::new(),
            timer_count: String::new(),
            sound_file: String::new(),
            final_sound_file: String::new(),
            progress_bar_mode: false,
            geometric_enabled: false,
            geometric_ratio: String::new(),
            randomize_enabled: false,
            randomize_gaussian: false,
            randomize_sigma: String::new(),
        }
    }
}

// ─── Audio ───────────────────────────────────────────────────────────────────
struct AudioPlayer {
    _stream: rodio::OutputStream,
    stream_handle: rodio::OutputStreamHandle,
    sink: Option<rodio::Sink>,
    sound_path: Option<PathBuf>,
    preview_start: Option<Instant>,
}

const PREVIEW_TOTAL_MS: u64 = 4000;
const PREVIEW_FADE_MS: u64 = 400;

impl AudioPlayer {
    fn new() -> Option<Self> {
        let (stream, handle) = rodio::OutputStream::try_default().ok()?;
        Some(Self {
            _stream: stream,
            stream_handle: handle,
            sink: None,
            sound_path: None,
            preview_start: None,
        })
    }

    fn set_sound(&mut self, path: PathBuf) {
        self.sound_path = Some(path);
    }

    /// Tear down and recreate the output stream against the current default
    /// output device. rodio binds to the default device once at creation and
    /// does not follow device changes, so when the active device is swapped or
    /// unplugged (Windows falls back to another) the old stream goes silent for
    /// good — a fresh stream is the only recovery. Preserves the selected sound
    /// and drops any in-progress playback. If no default device can be opened,
    /// the existing stream is left in place so a later retry can succeed.
    fn reinit(&mut self) {
        self.stop();
        if let Ok((stream, handle)) = rodio::OutputStream::try_default() {
            self._stream = stream;
            self.stream_handle = handle;
        }
    }

    fn play_once(&mut self) {
        self.stop();
        if let Some(path) = &self.sound_path {
            if let Ok(file) = std::fs::File::open(path) {
                let reader = std::io::BufReader::new(file);
                if let Ok(source) = rodio::Decoder::new(reader) {
                    let sink = rodio::Sink::try_new(&self.stream_handle).ok();
                    if let Some(ref s) = sink {
                        s.append(source);
                        s.play();
                    }
                    self.sink = sink;
                }
            }
        }
    }

    fn preview(&mut self) {
        // No-op if already playing a preview
        if self.preview_start.is_some() && self.is_playing() {
            return;
        }
        self.stop();
        let path = match &self.sound_path {
            Some(p) => p.clone(),
            None => return,
        };
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => return,
        };
        let reader = std::io::BufReader::new(file);
        let source = match rodio::Decoder::new(reader) {
            Ok(s) => s,
            Err(_) => return,
        };
        if let Ok(sink) = rodio::Sink::try_new(&self.stream_handle) {
            sink.set_volume(0.0);
            sink.append(source);
            sink.play();
            self.sink = Some(sink);
            self.preview_start = Some(Instant::now());
        }
    }

    /// Call each frame to drive preview fade-in, fade-out, and stop.
    fn tick_preview(&mut self) {
        if let Some(start) = self.preview_start {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let fade_out_begin = PREVIEW_TOTAL_MS - PREVIEW_FADE_MS;

            if elapsed_ms >= PREVIEW_TOTAL_MS {
                self.stop();
            } else if let Some(ref sink) = self.sink {
                let volume = if elapsed_ms < PREVIEW_FADE_MS {
                    // Fade-in
                    elapsed_ms as f32 / PREVIEW_FADE_MS as f32
                } else if elapsed_ms >= fade_out_begin {
                    // Fade-out
                    let progress =
                        (elapsed_ms - fade_out_begin) as f32 / PREVIEW_FADE_MS as f32;
                    1.0 - progress
                } else {
                    1.0
                };
                sink.set_volume(volume.clamp(0.0, 1.0));
            }
        }
    }

    fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.preview_start = None;
    }

    fn is_playing(&self) -> bool {
        self.sink.as_ref().is_some_and(|s| !s.empty())
    }

    fn play_file(&mut self, path: &std::path::Path) {
        self.stop();
        if let Ok(file) = std::fs::File::open(path) {
            let reader = std::io::BufReader::new(file);
            if let Ok(source) = rodio::Decoder::new(reader) {
                let sink = rodio::Sink::try_new(&self.stream_handle).ok();
                if let Some(ref s) = sink {
                    s.append(source);
                    s.play();
                }
                self.sink = sink;
            }
        }
    }

    fn preview_file(&mut self, path: &std::path::Path) {
        if self.preview_start.is_some() && self.is_playing() {
            return;
        }
        self.stop();
        let file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return,
        };
        let reader = std::io::BufReader::new(file);
        let source = match rodio::Decoder::new(reader) {
            Ok(s) => s,
            Err(_) => return,
        };
        if let Ok(sink) = rodio::Sink::try_new(&self.stream_handle) {
            sink.set_volume(0.0);
            sink.append(source);
            sink.play();
            self.sink = Some(sink);
            self.preview_start = Some(Instant::now());
        }
    }
}

// ─── Timer ───────────────────────────────────────────────────────────────────
struct Timer {
    id: u64,
    total_seconds: f64,
    remaining_seconds: f64,
    /// This timer's own inter-timer duration: how long after the previous
    /// timer fires (or after Start All, for the first) this one fires. Stored
    /// at creation because earlier timers are removed from the list once
    /// silenced, so it can't be re-derived from neighbors later.
    gap_seconds: f64,
    is_active: bool,
    is_alerting: bool,
    marked_for_removal: bool,
}

impl Timer {
    fn new(id: u64, duration_seconds: f64, gap_seconds: f64) -> Self {
        Self {
            id,
            total_seconds: duration_seconds,
            remaining_seconds: duration_seconds,
            gap_seconds,
            is_active: false,
            is_alerting: false,
            marked_for_removal: false,
        }
    }

    /// Advance this timer by dt_seconds * speed. Returns true if alert just started.
    fn tick(&mut self, dt_seconds: f64, speed_multiplier: f64) -> bool {
        if self.is_active {
            if self.remaining_seconds > 0.0 {
                self.remaining_seconds -= dt_seconds * speed_multiplier;
                if self.remaining_seconds < 0.0 {
                    self.remaining_seconds = 0.0;
                }
                if self.remaining_seconds <= 0.0 {
                    self.is_active = false;
                    self.is_alerting = true;
                    return true;
                }
            }
        }
        false
    }

    fn silence(&mut self) {
        self.is_alerting = false;
        self.marked_for_removal = true;
    }

    fn format_time(&self) -> String {
        let secs = self.remaining_seconds.max(0.0) as u64;
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        if h > 0 {
            format!("{h:02}:{m:02}:{s:02}")
        } else {
            format!("{m:02}:{s:02}")
        }
    }

}

// ─── Time parsing ────────────────────────────────────────────────────────────
fn parse_time_str(s: &str) -> f64 {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return 0.0;
    }

    // HH:MM:SS or MM:SS format
    if s.contains(':') {
        let parts: Vec<&str> = s.split(':').collect();
        let mut total = 0.0;
        for (i, part) in parts.iter().rev().enumerate() {
            if let Ok(v) = part.trim().parse::<f64>() {
                match i {
                    0 => total += v,          // seconds
                    1 => total += v * 60.0,   // minutes
                    2 => total += v * 3600.0, // hours
                    _ => {}
                }
            }
        }
        return total;
    }

    // Natural language: "1h 30m 45s" or raw number (=minutes)
    let mut total = 0.0;
    for part in s.split_whitespace() {
        if let Some(num_str) = part.strip_suffix('h') {
            if let Ok(v) = num_str.parse::<f64>() {
                total += v * 3600.0;
            }
        } else if let Some(num_str) = part.strip_suffix('m') {
            if let Ok(v) = num_str.parse::<f64>() {
                total += v * 60.0;
            }
        } else if let Some(num_str) = part.strip_suffix('s') {
            if let Ok(v) = num_str.parse::<f64>() {
                total += v;
            }
        } else if let Ok(v) = part.parse::<f64>() {
            total += v * 60.0; // Raw numbers default to minutes
        }
    }
    total
}

/// Unit the σ field inherits from the Total Duration text: the last h/m/s
/// suffix present wins; a bare number or H:MM:SS falls back to minutes, the
/// parser's own default for raw numbers. Returns (seconds per unit, name).
fn duration_unit(duration_text: &str) -> (f64, &'static str) {
    let s = duration_text.trim().to_lowercase();
    let mut unit = (60.0, "minutes");
    if s.contains(':') {
        return unit;
    }
    for part in s.split_whitespace() {
        if part.ends_with('h') {
            unit = (3600.0, "hours");
        } else if part.ends_with('m') {
            unit = (60.0, "minutes");
        } else if part.ends_with('s') {
            unit = (1.0, "seconds");
        }
    }
    unit
}

/// Restrict the σ buffer to digits and a decimal point — no letters, since the
/// unit is inherited from Total Duration rather than typed.
fn sanitize_sigma(s: &mut String) {
    s.retain(|c| c.is_ascii_digit() || c == '.');
}

/// One standard-normal draw via Box–Muller.
fn sample_standard_normal<R: Rng>(rng: &mut R) -> f64 {
    let u1: f64 = rng.random::<f64>().max(f64::MIN_POSITIVE);
    let u2: f64 = rng.random::<f64>();
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

/// Position curve for the distribution switch: a logistic sigmoid rescaled so
/// it hits exactly 0 at t=0 and 1 at t=1, which makes the knob's velocity
/// profile the sigmoid's bell-shaped first derivative.
fn sigmoid_ease(t: f32) -> f32 {
    // Steepness: at 16 the middle fifth of the animation covers ~2/3 of the
    // travel, so the knob visibly lingers at the ends and snaps through the middle.
    const K: f32 = 16.0;
    let logistic = |x: f32| 1.0 / (1.0 + (-K * (x - 0.5)).exp());
    let lo = logistic(0.0);
    let hi = logistic(1.0);
    ((logistic(t.clamp(0.0, 1.0)) - lo) / (hi - lo)).clamp(0.0, 1.0)
}

const SWITCH_ANIM_SECS: f32 = 0.25;

/// Sliding two-state switch. Left (false) / right (true). The knob glides
/// between the two ends over SWITCH_ANIM_SECS following `sigmoid_ease`.
fn toggle_switch(ui: &mut egui::Ui, on: &mut bool, enabled: bool) -> egui::Response {
    let size = egui::vec2(34.0, 18.0);
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    if !enabled {
        response = response.on_disabled_hover_text("");
    }
    if enabled && response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let linear = ui
        .ctx()
        .animate_bool_with_time(response.id, *on, SWITCH_ANIM_SECS);
    let t = sigmoid_ease(linear);

    if ui.is_rect_visible(rect) {
        let radius = rect.height() / 2.0;
        let (track_stroke, knob_fill) = if !enabled {
            (DISABLED_BORDER, DISABLED_BORDER)
        } else if response.hovered() {
            (ACCENT_HOVER, ACCENT_HOVER)
        } else {
            (ACCENT, ACCENT)
        };
        ui.painter().rect(
            rect,
            radius,
            BG_WIDGET,
            egui::Stroke::new(1.0, track_stroke),
            egui::StrokeKind::Inside,
        );
        let knob_r = radius - 3.0;
        let x_left = rect.left() + radius;
        let x_right = rect.right() - radius;
        let x = egui::lerp(x_left..=x_right, t);
        ui.painter()
            .circle_filled(egui::pos2(x, rect.center().y), knob_r, knob_fill);
    }
    response
}

// ─── Field state tracking ────────────────────────────────────────────────────
#[derive(PartialEq, Clone, Copy)]
enum DisabledField {
    None,
    Interval,
    Count,
    Duration,
    Ratio,
}

/// Format a duration in seconds as a re-parseable time string, preserving
/// sub-second precision (up to centiseconds) when present. Whole seconds render
/// as "H:MM:SS" / "M:SS"; fractional seconds append e.g. "M:SS.dd".
fn format_secs_precise(secs: f64) -> String {
    if secs <= 0.0 {
        return String::new();
    }
    // Round to centiseconds up front so the fractional part never rounds to 1.00.
    let total = (secs * 100.0).round() / 100.0;
    let whole = total.floor() as u64;
    let frac = total - whole as f64;
    let h = whole / 3600;
    let m = (whole % 3600) / 60;
    let s = whole % 60;
    let frac_str = if frac > 1e-9 {
        // "0.dd" → ".dd" with trailing zeros trimmed
        let f = format!("{frac:.2}");
        let trimmed = f.trim_start_matches('0').trim_end_matches('0').trim_end_matches('.');
        trimmed.to_string()
    } else {
        String::new()
    };
    if h > 0 {
        format!("{h}:{m:02}:{s:02}{frac_str}")
    } else {
        format!("{m}:{s:02}{frac_str}")
    }
}

/// Sum of the geometric series 1 + r + r² + … + r^(n-1).
fn geom_sum(r: f64, n: u32) -> f64 {
    if n == 0 {
        0.0
    } else if (r - 1.0).abs() < 1e-12 {
        n as f64
    } else {
        (r.powi(n as i32) - 1.0) / (r - 1.0)
    }
}

// ─── App ─────────────────────────────────────────────────────────────────────
struct CascadingTimersApp {
    // Settings inputs
    input_interval: String,
    input_count: String,
    input_duration: String,
    input_offset: String,
    input_ratio: String,

    // Calculated values for disabled fields
    calculated_interval: f64,
    calculated_count: u32,
    calculated_duration: f64,
    calculated_ratio: f64,

    disabled_field: DisabledField,

    // Geometric series
    geometric_enabled: bool,
    geom_invalid: bool, // a geometric solve had no valid solution

    // Randomized intervals: schedule is drawn from Count + Duration (+ Offset)
    // alone; Interval and Geometric Series are greyed out while this is on.
    randomize_enabled: bool,
    // Distribution switch: false = uniform fire times, true = gaussian gaps
    // around the evenly-spaced mean with user-supplied σ (units follow Total
    // Duration).
    randomize_gaussian: bool,
    input_sigma: String,

    // Speed
    speed_pct: u32, // 50..=200, representing 0.50x to 2.00x

    // Display mode
    progress_bar_mode: bool,

    // Timers
    timers: Vec<Timer>,
    next_timer_id: u64,
    is_paused: bool,

    // Time tracking
    last_tick: Instant,

    // Audio
    audio: Option<AudioPlayer>,
    sound_file_display: String,
    final_sound_file_display: String,
    final_sound_path: Option<PathBuf>,
    current_alert_path: Option<PathBuf>,

    // Repeat alert
    repeat_alert_enabled: bool,
    repeat_alert_count: u32,
    repeat_alert_count_str: String,
    alert_plays_remaining: u32,

    // Config
    config: Config,

    // UI state
    pending_clear: bool,
    needs_rebuild: bool,
    timers_running: bool,
    all_timers_completed: bool,
    completed_timer_durations: Vec<f64>,
}

/// Upper bound on timers per cohort; typed counts clamp to it.
const MAX_COUNT: u32 = 25;

const FONT_BOLD: &str = "app-bold";
const FONT_TIMER: &str = "app-timer";

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Aptos Regular — primary UI font
    let aptos_regular = find_aptos_font("30153066857.ttf")
        .or_else(|| load_system_font("segoeui.ttf"));
    if let Some(data) = aptos_regular {
        fonts
            .font_data
            .insert("aptos-regular".to_string(), egui::FontData::from_owned(data).into());
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, "aptos-regular".to_string());
    }

    // Aptos Bold — for buttons
    let aptos_bold = find_aptos_font("32483553004.ttf")
        .or_else(|| load_system_font("segoeuib.ttf"));
    if let Some(data) = aptos_bold {
        fonts
            .font_data
            .insert("aptos-bold".to_string(), egui::FontData::from_owned(data).into());
        fonts.families.insert(
            egui::FontFamily::Name(FONT_BOLD.into()),
            vec!["aptos-bold".to_string()],
        );
    }

    // Segoe UI — for timer countdown text
    if let Some(data) = load_system_font("segoeui.ttf") {
        fonts
            .font_data
            .insert("segoe-ui".to_string(), egui::FontData::from_owned(data).into());
        fonts.families.insert(
            egui::FontFamily::Name(FONT_TIMER.into()),
            vec!["segoe-ui".to_string()],
        );
    }

    ctx.set_fonts(fonts);
}

fn find_aptos_font(filename: &str) -> Option<Vec<u8>> {
    let home = std::env::var("LOCALAPPDATA").ok()?;
    let path = PathBuf::from(home)
        .join(r"Microsoft\FontCache\4\CloudFonts\Aptos")
        .join(filename);
    std::fs::read(&path).ok()
}

fn load_system_font(filename: &str) -> Option<Vec<u8>> {
    let path = PathBuf::from(r"C:\Windows\Fonts").join(filename);
    std::fs::read(&path).ok()
}

impl CascadingTimersApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_fonts(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let config = Config::load();

        let mut audio = AudioPlayer::new();
        let mut sound_display = "No sound selected".to_string();
        if !config.sound_file.is_empty() {
            let path = PathBuf::from(&config.sound_file);
            if path.exists() {
                sound_display = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Unknown".to_string());
                if let Some(ref mut a) = audio {
                    a.set_sound(path);
                }
            }
        }

        let mut final_sound_display = String::new();
        let mut final_sound_path = None;
        if !config.final_sound_file.is_empty() {
            let path = PathBuf::from(&config.final_sound_file);
            if path.exists() {
                final_sound_display = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Unknown".to_string());
                final_sound_path = Some(path);
            }
        }

        Self {
            input_interval: String::new(),
            input_count: String::new(),
            input_duration: String::new(),
            input_offset: String::new(),
            input_ratio: config.geometric_ratio.clone(),

            calculated_interval: 0.0,
            calculated_count: 0,
            calculated_duration: 0.0,
            calculated_ratio: 0.0,

            disabled_field: DisabledField::None,

            geometric_enabled: config.geometric_enabled && !config.randomize_enabled,
            geom_invalid: false,

            randomize_enabled: config.randomize_enabled,
            randomize_gaussian: config.randomize_gaussian,
            input_sigma: config.randomize_sigma.clone(),

            speed_pct: 100,

            progress_bar_mode: config.progress_bar_mode,

            timers: Vec::new(),
            next_timer_id: 1,
            is_paused: true,

            last_tick: Instant::now(),

            audio,
            sound_file_display: sound_display,
            final_sound_file_display: final_sound_display,
            final_sound_path,
            current_alert_path: None,

            repeat_alert_enabled: false,
            repeat_alert_count: 1,
            repeat_alert_count_str: "1".to_string(),
            alert_plays_remaining: 0,

            config,

            pending_clear: false,
            needs_rebuild: false,
            timers_running: false,
            all_timers_completed: false,
            completed_timer_durations: Vec::new(),
        }
    }

    fn speed_multiplier(&self) -> f64 {
        self.speed_pct as f64 / 100.0
    }

    fn is_valid_time(s: &str) -> bool {
        let s = s.trim();
        !s.is_empty() && parse_time_str(s) > 0.0
    }

    fn is_valid_count(s: &str) -> bool {
        let s = s.trim();
        if s.is_empty() {
            return false;
        }
        s.parse::<u32>().is_ok_and(|v| v > 0)
    }

    fn is_valid_ratio(s: &str) -> bool {
        parse_ratio(s).is_some()
    }

    fn update_field_states(&mut self) {
        if self.randomize_enabled {
            self.update_field_states_random();
        } else if self.geometric_enabled {
            self.update_field_states_geometric();
        } else {
            // Leaving geometric: release a ratio-computed lock if one is held.
            if self.disabled_field == DisabledField::Ratio {
                self.set_disabled_field(DisabledField::None);
            }
            self.geom_invalid = false;
            self.update_field_states_normal();
        }
    }

    /// Switch which field is computed/locked, clearing the display buffer of the
    /// field that is being released back to user control.
    fn set_disabled_field(&mut self, new_df: DisabledField) {
        if self.disabled_field == new_df {
            return;
        }
        match self.disabled_field {
            DisabledField::Interval => self.input_interval.clear(),
            DisabledField::Count => self.input_count.clear(),
            DisabledField::Duration => self.input_duration.clear(),
            DisabledField::Ratio => self.input_ratio.clear(),
            DisabledField::None => {}
        }
        self.disabled_field = new_df;
    }

    /// Write the current computed value into the locked field's display buffer
    /// (full precision lives in calculated_*; the buffer is display-only).
    fn sync_computed_display(&mut self) {
        match self.disabled_field {
            DisabledField::Interval => {
                self.input_interval = format_secs_precise(self.calculated_interval)
            }
            DisabledField::Count => self.input_count = self.calculated_count.to_string(),
            DisabledField::Duration => {
                self.input_duration = format_secs_precise(self.calculated_duration)
            }
            DisabledField::Ratio => self.input_ratio = format!("{:.4}", self.calculated_ratio),
            DisabledField::None => {}
        }
    }

    /// Randomized intervals: Count and Duration are both plain inputs (no
    /// two-of-three solve) and Interval is greyed out. A cohort is drawn as
    /// soon as both are valid.
    fn update_field_states_random(&mut self) {
        self.set_disabled_field(DisabledField::None);
        self.geom_invalid = false;
        if Self::is_valid_count(&self.input_count) && Self::is_valid_time(&self.input_duration) {
            self.needs_rebuild = true;
        }
    }

    /// Toggle randomized intervals. Turning it on greys Interval and Geometric
    /// Series, so their inputs are dropped and geometric mode is switched off.
    fn set_randomize(&mut self, on: bool) {
        self.randomize_enabled = on;
        if on {
            self.set_disabled_field(DisabledField::None);
            self.input_interval.clear();
            self.geometric_enabled = false;
            self.geom_invalid = false;
            self.config.geometric_enabled = false;
        }
        self.config.randomize_enabled = on;
        self.config.save();
    }

    /// Normal (arithmetic) two-of-three among Interval / Count / Duration.
    fn update_field_states_normal(&mut self) {
        let interval_filled =
            self.disabled_field != DisabledField::Interval && Self::is_valid_time(&self.input_interval);
        let count_filled =
            self.disabled_field != DisabledField::Count && Self::is_valid_count(&self.input_count);
        let duration_filled =
            self.disabled_field != DisabledField::Duration && Self::is_valid_time(&self.input_duration);

        let filled = interval_filled as u8 + count_filled as u8 + duration_filled as u8;

        if filled >= 2 {
            let target = if !interval_filled {
                DisabledField::Interval
            } else if !count_filled {
                DisabledField::Count
            } else if !duration_filled {
                DisabledField::Duration
            } else {
                // All three filled — prefer interval+count, recalculate duration.
                DisabledField::Duration
            };
            self.set_disabled_field(target);
            match target {
                DisabledField::Interval => self.calculate_interval(),
                DisabledField::Count => self.calculate_count(),
                DisabledField::Duration => self.calculate_duration(),
                _ => {}
            }
            self.sync_computed_display();
            self.needs_rebuild = true;
        } else {
            self.set_disabled_field(DisabledField::None);
        }
    }

    /// Geometric two-of-three among Interval / Ratio / Duration. Count is always
    /// a required input (a geometric series needs an explicit term count).
    fn update_field_states_geometric(&mut self) {
        if !Self::is_valid_count(&self.input_count) {
            self.set_disabled_field(DisabledField::None);
            self.geom_invalid = false;
            return;
        }

        let i_user =
            self.disabled_field != DisabledField::Interval && Self::is_valid_time(&self.input_interval);
        let r_user =
            self.disabled_field != DisabledField::Ratio && Self::is_valid_ratio(&self.input_ratio);
        let d_user =
            self.disabled_field != DisabledField::Duration && Self::is_valid_time(&self.input_duration);

        let provided = i_user as u8 + r_user as u8 + d_user as u8;

        if provided >= 2 {
            // The blank field of the trio is solved & locked. If all three are
            // present (only transiently), keep Interval+Ratio and derive Duration.
            let target = if !i_user {
                DisabledField::Interval
            } else if !d_user {
                DisabledField::Duration
            } else if !r_user {
                DisabledField::Ratio
            } else {
                DisabledField::Duration
            };
            self.set_disabled_field(target);
            let ok = match target {
                DisabledField::Interval => self.calculate_interval_geom(),
                DisabledField::Duration => self.calculate_duration_geom(),
                DisabledField::Ratio => self.calculate_ratio_geom(),
                _ => false,
            };
            self.geom_invalid = !ok;
            if ok {
                self.sync_computed_display();
            } else {
                // No valid solution — blank the locked field (UI flags it red).
                match target {
                    DisabledField::Interval => self.input_interval.clear(),
                    DisabledField::Duration => self.input_duration.clear(),
                    DisabledField::Ratio => self.input_ratio.clear(),
                    _ => {}
                }
            }
            self.needs_rebuild = true;
        } else {
            self.set_disabled_field(DisabledField::None);
            self.geom_invalid = false;
        }
    }

    fn calculate_interval(&mut self) {
        let count: f64 = self.input_count.trim().parse().unwrap_or(0.0);
        let total_duration = parse_time_str(&self.input_duration);
        if count <= 1.0 || total_duration <= 0.0 {
            return;
        }
        let offset = parse_time_str(&self.input_offset);
        let interval = if offset > 0.0 {
            (total_duration - offset) / (count - 1.0)
        } else {
            total_duration / count
        };
        if interval > 0.0 {
            self.calculated_interval = interval; // full precision (no flooring)
        }
    }

    // ── Geometric solves (offset-free; offset is applied last at build time) ──
    // Series of `n` gaps a·r⁰ … a·r^(n-1); total D = a·(rⁿ−1)/(r−1).

    fn calculate_duration_geom(&mut self) -> bool {
        let a = parse_time_str(&self.input_interval);
        let r = parse_ratio(&self.input_ratio).unwrap_or(0.0);
        let n = self.input_count.trim().parse::<u32>().unwrap_or(0);
        if a <= 0.0 || r <= 0.0 || n == 0 {
            return false;
        }
        self.calculated_duration = a * geom_sum(r, n);
        self.calculated_duration > 0.0 && self.calculated_duration.is_finite()
    }

    fn calculate_interval_geom(&mut self) -> bool {
        let r = parse_ratio(&self.input_ratio).unwrap_or(0.0);
        let d = parse_time_str(&self.input_duration);
        let n = self.input_count.trim().parse::<u32>().unwrap_or(0);
        if r <= 0.0 || d <= 0.0 || n == 0 {
            return false;
        }
        let s = geom_sum(r, n);
        if s <= 0.0 || !s.is_finite() {
            return false;
        }
        self.calculated_interval = d / s;
        self.calculated_interval > 0.0 && self.calculated_interval.is_finite()
    }

    fn calculate_ratio_geom(&mut self) -> bool {
        let a = parse_time_str(&self.input_interval);
        let d = parse_time_str(&self.input_duration);
        let n = self.input_count.trim().parse::<u32>().unwrap_or(0);
        if a <= 0.0 || d <= 0.0 || n == 0 {
            return false;
        }
        if n == 1 {
            // One timer: its only gap is the base interval; ratio is irrelevant.
            // Valid only if the requested duration equals the base interval.
            self.calculated_ratio = 1.0;
            return (d - a).abs() <= 1e-6 * a.max(1.0);
        }
        let nn = n as f64;
        // f(r) = a·geom_sum(r,n) is strictly increasing on r>0, ranging (a, ∞),
        // with f(1) = a·n. So a valid r>0 exists iff d > a.
        if d <= a {
            self.calculated_ratio = 0.0;
            return false;
        }
        if (d - a * nn).abs() <= 1e-9 * (a * nn) {
            self.calculated_ratio = 1.0;
            return true;
        }
        let f = |r: f64| a * geom_sum(r, n);
        let (mut lo, mut hi) = if d < a * nn {
            (1e-9_f64, 1.0_f64)
        } else {
            let mut hi = 2.0_f64;
            let mut guard = 0;
            while f(hi) < d && guard < 300 {
                hi *= 2.0;
                guard += 1;
            }
            (1.0_f64, hi)
        };
        let mut r = 0.5 * (lo + hi);
        for _ in 0..200 {
            r = 0.5 * (lo + hi);
            let val = f(r);
            if (val - d).abs() <= 1e-9 * d.max(1.0) {
                break;
            }
            if val < d {
                lo = r;
            } else {
                hi = r;
            }
        }
        if r > 0.0 && r.is_finite() {
            self.calculated_ratio = r;
            true
        } else {
            self.calculated_ratio = 0.0;
            false
        }
    }

    fn calculate_count(&mut self) {
        let interval = parse_time_str(&self.input_interval);
        let total_duration = parse_time_str(&self.input_duration);
        if interval <= 0.0 || total_duration <= 0.0 {
            return;
        }
        let offset = parse_time_str(&self.input_offset);
        let count = if offset > 0.0 {
            if total_duration < offset {
                0.0
            } else {
                ((total_duration - offset) / interval).floor() + 1.0
            }
        } else {
            (total_duration / interval).floor()
        };
        if count > 0.0 {
            self.calculated_count = (count as u32).min(MAX_COUNT);
        }
    }

    fn calculate_duration(&mut self) {
        let interval = parse_time_str(&self.input_interval);
        let count: f64 = self.input_count.trim().parse().unwrap_or(0.0);
        if interval <= 0.0 || count <= 0.0 {
            return;
        }
        let offset = parse_time_str(&self.input_offset);
        let total = if offset > 0.0 {
            offset + (count - 1.0) * interval
        } else {
            count * interval
        };
        self.calculated_duration = total;
    }

    fn get_effective_interval(&self) -> f64 {
        if self.disabled_field == DisabledField::Interval {
            self.calculated_interval
        } else {
            parse_time_str(&self.input_interval)
        }
    }

    fn get_effective_count(&self) -> u32 {
        if self.disabled_field == DisabledField::Count {
            self.calculated_count
        } else {
            self.input_count.trim().parse().unwrap_or(0)
        }
    }

    fn get_effective_ratio(&self) -> f64 {
        if self.disabled_field == DisabledField::Ratio {
            self.calculated_ratio
        } else {
            parse_ratio(&self.input_ratio).unwrap_or(0.0)
        }
    }

    fn rebuild_timers(&mut self) {
        if self.timers_running {
            return;
        }

        self.timers.clear();

        let count = self.get_effective_count().min(MAX_COUNT);
        if count == 0 {
            return;
        }

        let offset_s = parse_time_str(&self.input_offset);

        if self.randomize_enabled {
            // Random schedule: draw `count` uniform samples on [0, 1) (or
            // `count - 1` when an offset claims the first slot), sort them, and
            // scale so the largest lands exactly on Total Duration. With an
            // offset the samples span [offset, Duration] instead, so the first
            // timer still fires at the offset as in the other modes.
            let duration_s = parse_time_str(&self.input_duration);
            if duration_s <= 0.0 {
                return;
            }
            let (base, span, n_random) = if offset_s > 0.0 {
                (offset_s, duration_s - offset_s, count - 1)
            } else {
                (0.0, duration_s, count)
            };
            if n_random > 0 && span <= 0.0 {
                return;
            }
            let mut rng = rand::rng();
            let mut fire_times: Vec<f64> = Vec::with_capacity(count as usize);
            if offset_s > 0.0 {
                fire_times.push(offset_s);
            }
            if self.randomize_gaussian {
                // Gaussian gaps: µ is pinned to the evenly-spaced interval that
                // the other inputs imply (span / n), σ comes from the user in
                // Total Duration's units. Draws are clamped at zero, then the
                // whole set is rescaled so the gaps sum exactly to `span` —
                // which also keeps their mean exactly at µ.
                if n_random > 0 {
                    let mu = span / n_random as f64;
                    let (unit_secs, _) = duration_unit(&self.input_duration);
                    let sigma = self.input_sigma.trim().parse::<f64>().unwrap_or(0.0)
                        .max(0.0)
                        * unit_secs;
                    let mut gaps: Vec<f64> = (0..n_random)
                        .map(|_| (mu + sigma * sample_standard_normal(&mut rng)).max(0.0))
                        .collect();
                    let total: f64 = gaps.iter().sum();
                    if total > 0.0 {
                        let scale = span / total;
                        for g in &mut gaps {
                            *g *= scale;
                        }
                    } else {
                        for g in &mut gaps {
                            *g = mu;
                        }
                    }
                    let mut acc = base;
                    for g in gaps {
                        acc += g;
                        fire_times.push(acc);
                    }
                    // Absorb accumulated rounding so the last timer lands on Duration.
                    if let Some(last) = fire_times.last_mut() {
                        *last = duration_s;
                    }
                }
            } else {
                let mut samples: Vec<f64> =
                    (0..n_random).map(|_| rng.random::<f64>()).collect();
                samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let max = samples.last().copied().unwrap_or(0.0);
                let scale = if max > 0.0 { span / max } else { 0.0 };
                fire_times.extend(samples.iter().map(|u| base + u * scale));
            }

            let mut prev = 0.0;
            for fire in fire_times {
                let gap = (fire - prev).max(0.0);
                prev = fire;
                let id = self.next_timer_id;
                self.next_timer_id += 1;
                self.timers.push(Timer::new(id, fire, gap));
            }

            self.completed_timer_durations =
                self.timers.iter().map(|t| t.total_seconds).collect();

            self.config.timer_count = format!("{count}");
            self.config.geometric_enabled = false;
            self.config.randomize_enabled = true;
            self.config.randomize_sigma = self.input_sigma.clone();
            self.config.save();
            return;
        }

        let interval_s = self.get_effective_interval();
        if interval_s <= 0.0 {
            return;
        }

        if self.geometric_enabled {
            let r = self.get_effective_ratio();
            if r <= 0.0 || self.geom_invalid {
                return;
            }
            // Each timer's countdown is the cumulative fire time. The first gap is
            // the base interval (a·r⁰), or the offset when set — which overrides it.
            // Every later gap k is a·rᵏ and never adapts to the offset, so the tail
            // keeps its original geometric spacing.
            let mut cumulative = 0.0;
            for i in 0..count {
                let gap = if i == 0 {
                    if offset_s > 0.0 { offset_s } else { interval_s }
                } else {
                    interval_s * r.powi(i as i32)
                };
                cumulative += gap;
                let id = self.next_timer_id;
                self.next_timer_id += 1;
                self.timers.push(Timer::new(id, cumulative, gap));
            }
        } else {
            let start_base = if offset_s > 0.0 { offset_s } else { interval_s };
            for i in 0..count {
                let duration = start_base + (i as f64) * interval_s;
                let gap = if i == 0 { start_base } else { interval_s };
                let id = self.next_timer_id;
                self.next_timer_id += 1;
                self.timers.push(Timer::new(id, duration, gap));
            }
        }

        self.completed_timer_durations = self.timers.iter().map(|t| t.total_seconds).collect();

        // Persist (full precision; these are launch defaults, not reloaded into the UI).
        self.config.interval = format!("{interval_s}");
        self.config.timer_count = format!("{count}");
        self.config.geometric_enabled = self.geometric_enabled;
        self.config.geometric_ratio = self.input_ratio.clone();
        self.config.save();
    }

    fn start_all(&mut self) {
        if self.timers.is_empty() {
            self.rebuild_timers();
        }
        for t in &mut self.timers {
            if !t.is_alerting && t.remaining_seconds > 0.0 {
                t.is_active = true;
            }
        }
        self.is_paused = false;
        self.timers_running = true;
        self.all_timers_completed = false;
    }

    fn pause_all(&mut self) {
        self.is_paused = true;
        for t in &mut self.timers {
            t.is_active = false;
        }
        self.stop_sound();
    }

    fn reset_all(&mut self) {
        self.stop_sound();
        // Restore the entire original regimen as dictated by the settings at build
        // time — including timers that already fired and were silenced (and were thus
        // removed from the list). `completed_timer_durations` is the snapshot taken in
        // `rebuild_timers`, so this rebuilds the full cohort rather than merely resetting
        // the survivors to their max time. Falls back to resetting whatever timers are
        // present if no snapshot exists (e.g. nothing has been built yet).
        if self.completed_timer_durations.is_empty() {
            for t in &mut self.timers {
                t.remaining_seconds = t.total_seconds;
                t.is_active = false;
                t.is_alerting = false;
                t.marked_for_removal = false;
            }
        } else {
            self.timers.clear();
            let mut prev = 0.0;
            for &dur in &self.completed_timer_durations {
                let gap = (dur - prev).max(0.0);
                prev = dur;
                let id = self.next_timer_id;
                self.next_timer_id += 1;
                self.timers.push(Timer::new(id, dur, gap));
            }
        }
        self.is_paused = true;
        self.timers_running = false;
        self.all_timers_completed = false;
    }

    fn clear_all(&mut self) {
        self.pause_all();
        self.timers.clear();
        // Drop the regimen snapshot so a subsequent Reset can't resurrect the cleared
        // cohort.
        self.completed_timer_durations.clear();
        self.timers_running = false;
        self.all_timers_completed = false;
        // Keep a user-typed interval (reusable rate), but drop an auto-computed one
        // so a stale autofilled value doesn't linger after a clear.
        if self.disabled_field == DisabledField::Interval {
            self.input_interval.clear();
        }
        self.input_count.clear();
        self.input_duration.clear();
        self.input_offset.clear();
        self.input_ratio.clear();
        self.input_sigma.clear();
        self.geom_invalid = false;
        self.geometric_enabled = false;
        self.randomize_enabled = false;
        self.speed_pct = 100; // reset to 1.00x
        self.disabled_field = DisabledField::None;
        // Persist the reset mode flags so they don't silently reappear on relaunch.
        // The uniform/gaussian switch is a preference and survives a clear.
        self.config.geometric_enabled = false;
        self.config.geometric_ratio = String::new();
        self.config.randomize_enabled = false;
        self.config.randomize_sigma = String::new();
        self.config.save();
    }

    fn adjust_time(&mut self, delta: f64) {
        let mut newly_alerting = false;
        for t in &mut self.timers {
            if t.remaining_seconds > 0.0 && !t.is_alerting {
                t.remaining_seconds = (t.remaining_seconds + delta).max(0.0);
                if t.remaining_seconds <= 0.0 {
                    t.is_active = false;
                    t.is_alerting = true;
                    newly_alerting = true;
                }
            }
        }
        if newly_alerting {
            let is_final = self.timers.iter().all(|t| t.is_alerting || t.marked_for_removal);
            self.stop_sound();
            if !self.play_sound(is_final) {
                self.silence_alerting_timers();
            }
        }
    }

    fn play_sound(&mut self, is_final: bool) -> bool {
        let path = if is_final && self.final_sound_path.is_some() {
            self.final_sound_path.clone()
        } else {
            self.audio.as_ref().and_then(|a| a.sound_path.clone())
        };

        let Some(path) = path else { return false };

        if self.audio.as_ref().is_some_and(|a| a.is_playing()) {
            return true;
        }

        if let Some(ref mut audio) = self.audio {
            audio.play_file(&path);
        }

        self.current_alert_path = Some(path);
        self.alert_plays_remaining = if self.repeat_alert_enabled {
            self.repeat_alert_count
        } else {
            0
        };
        true
    }

    fn silence_alerting_timers(&mut self) {
        for t in &mut self.timers {
            if t.is_alerting {
                t.silence();
            }
        }
        self.timers.retain(|t| !t.marked_for_removal);
        if self.timers.is_empty() && self.timers_running {
            self.all_timers_completed = true;
            self.timers_running = false;
        }
    }

    fn stop_sound(&mut self) {
        if let Some(ref mut audio) = self.audio {
            audio.stop();
        }
        self.alert_plays_remaining = 0;
        self.current_alert_path = None;
    }

    /// Rebind audio output to the current default device without disturbing any
    /// timer state. Fixes the case where the audio device is changed/unplugged
    /// mid-session and rodio's launch-time stream goes permanently silent.
    fn reload_audio(&mut self) {
        if let Some(ref mut audio) = self.audio {
            audio.reinit();
        } else {
            // Audio was unavailable at launch (no output device then). Bring it
            // up now and re-apply the configured alert sound.
            let mut audio = AudioPlayer::new();
            if let Some(ref mut a) = audio {
                if !self.config.sound_file.is_empty() {
                    let path = PathBuf::from(&self.config.sound_file);
                    if path.exists() {
                        a.set_sound(path);
                    }
                }
            }
            self.audio = audio;
        }
    }

    fn process_tick(&mut self) {
        // Handle preview fade-out
        if let Some(ref mut audio) = self.audio {
            audio.tick_preview();
        }

        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f64();
        self.last_tick = now;

        let speed = self.speed_multiplier();
        let mut any_new_alert = false;
        let mut any_alerting = false;

        for t in &mut self.timers {
            if t.tick(dt, speed) {
                any_new_alert = true;
            }
            if t.is_alerting {
                any_alerting = true;
            }
        }

        // Remove silenced timers
        self.timers.retain(|t| !t.marked_for_removal);

        if self.timers.is_empty() && self.timers_running {
            self.all_timers_completed = true;
            self.timers_running = false;
        }

        // Sound management (don't interfere with preview)
        let preview_active = self
            .audio
            .as_ref()
            .is_some_and(|a| a.preview_start.is_some());
        if any_new_alert {
            // New timer fired: stop current sound, play for the new alert
            let is_final = self.timers.iter().all(|t| t.is_alerting || t.marked_for_removal);
            self.stop_sound();
            if !self.play_sound(is_final) {
                // No alert sound selected — auto-silence instead of leaving timers in alert state
                self.silence_alerting_timers();
            }
        } else if any_alerting && !preview_active {
            // Check if sound finished playing
            let sound_playing = self.audio.as_ref().is_some_and(|a| a.is_playing());
            if !sound_playing {
                if self.alert_plays_remaining > 0 {
                    // Repeat the alert sound using the same sound as the initial alert
                    self.alert_plays_remaining -= 1;
                    let replay_path = self.current_alert_path.clone();
                    if let Some(ref mut audio) = self.audio {
                        if let Some(ref path) = replay_path {
                            audio.play_file(path);
                        } else {
                            audio.play_once();
                        }
                    }
                } else {
                    // All plays done — silence alerting timers
                    for t in &mut self.timers {
                        if t.is_alerting {
                            t.silence();
                        }
                    }
                    self.timers.retain(|t| !t.marked_for_removal);
                    if self.timers.is_empty() && self.timers_running {
                        self.all_timers_completed = true;
                        self.timers_running = false;
                    }
                }
            }
        } else if !any_alerting && !preview_active {
            self.stop_sound();
        }
    }
}

// ─── UI helpers ──────────────────────────────────────────────────────────────

fn accent_button_sized(ui: &mut egui::Ui, text: &str, size: egui::Vec2) -> egui::Response {
    let btn = egui::Button::new(
        egui::RichText::new(text)
            .color(TEXT_WHITE)
            .family(egui::FontFamily::Name(FONT_BOLD.into()))
            .size(14.0),
    )
    .fill(ACCENT)
    .corner_radius(4.0)
    .min_size(size);
    ui.add(btn)
}

fn secondary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let btn = egui::Button::new(egui::RichText::new(text).color(TEXT_WHITE).size(11.0))
        .fill(BTN_SECONDARY)
        .corner_radius(4.0);
    ui.add(btn)
}

fn styled_text_edit<'a>(text: &'a mut String, hint: &'a str, enabled: bool) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(text)
        .hint_text(if enabled { hint } else { "" })
        .text_color(if enabled { TEXT_WHITE } else { DISABLED_BORDER })
        .interactive(enabled)
}

/// Restrict a ratio buffer to digits, decimal points, and a fraction slash.
fn sanitize_ratio(s: &mut String) {
    s.retain(|c| c.is_ascii_digit() || c == '.' || c == '/');
}

/// Parse a geometric ratio. Accepts plain decimals ("1.5", ".5", "2") and
/// fractions ("3/2", "1.5/2"). Returns the value only if it is finite and > 0.
fn parse_ratio(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let v = if let Some((num, den)) = s.split_once('/') {
        let n = num.trim().parse::<f64>().ok()?;
        let d = den.trim().parse::<f64>().ok()?;
        if d == 0.0 {
            return None;
        }
        n / d
    } else {
        s.parse::<f64>().ok()?
    };
    (v > 0.0 && v.is_finite()).then_some(v)
}

/// On blur: pad a short decimal ratio out to 4 places ("1.5" → "1.5000"),
/// but leave fractions and longer-than-4-decimal values exactly as typed so a
/// deliberately precise ratio is preserved (just visually overflows the field).
fn auto_extend_ratio(s: &mut String) {
    let t = s.trim();
    if t.is_empty() || t.contains('/') {
        return;
    }
    if let Ok(v) = t.parse::<f64>() {
        let decimals = t.split('.').nth(1).map(|d| d.len()).unwrap_or(0);
        if decimals < 4 {
            *s = format!("{v:.4}");
        }
    }
}

// ─── eframe impl ─────────────────────────────────────────────────────────────

impl eframe::App for CascadingTimersApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Tick timers
        self.process_tick();

        // Request continuous repaint while timers are running or preview is playing
        let preview_active = self
            .audio
            .as_ref()
            .is_some_and(|a| a.preview_start.is_some());
        if self.timers_running || self.timers.iter().any(|t| t.is_alerting) || preview_active {
            ctx.request_repaint_after(Duration::from_millis(50));
        }

        // Process pending rebuild
        if self.needs_rebuild {
            self.needs_rebuild = false;
            let old_count = self.timers.len();
            self.rebuild_timers();
            let new_count = self.timers.len();
            if new_count != old_count {
                const BASE_HEIGHT: f32 = 405.0;
                const PER_TIMER: f32 = 46.0;
                let target = (BASE_HEIGHT + new_count as f32 * PER_TIMER).clamp(430.0, 1200.0);
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                    egui::vec2(ctx.screen_rect().width().max(400.0), target),
                ));
            }
        }

        // Apply dark theme styling
        let mut style = (*ctx.style()).clone();
        style.visuals.dark_mode = true;
        style.visuals.panel_fill = BG_DARK;
        style.visuals.window_fill = BG_DARK;
        style.visuals.extreme_bg_color = BG_WIDGET;
        style.visuals.widgets.noninteractive.bg_fill = BG_WIDGET;
        style.visuals.widgets.inactive.bg_fill = BG_WIDGET;
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, ACCENT);
        style.visuals.widgets.hovered.bg_fill = BG_WIDGET;
        style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT_HOVER);
        style.visuals.widgets.active.bg_fill = BG_WIDGET;
        style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT);
        style.visuals.selection.bg_fill = ACCENT;
        style.visuals.selection.stroke = egui::Stroke::new(1.0, TEXT_WHITE);
        ctx.set_style(style);

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG_DARK).inner_margin(12.0))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);

                // ── Settings Section ──
                ui.group(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);

                    let placeholder = "e.g. 1.5m, 3.75h, 0:10:00, 90 (=90m)";
                    let mut fields_changed = false;

                    // Interval
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Interval:").color(TEXT_WHITE).size(14.0),
                        );
                        let enabled = !self.timers_running
                            && !self.randomize_enabled
                            && self.disabled_field != DisabledField::Interval;
                        let te = styled_text_edit(&mut self.input_interval, placeholder, enabled);
                        let r = ui.add_sized([ui.available_width(), 24.0], te);
                        if r.changed() {
                            self.input_interval
                                .retain(|c| c.is_ascii_digit() || ".hmsHMS: ".contains(c));
                            fields_changed = true;
                        }
                    });

                    // Count
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Timer Count:").color(TEXT_WHITE).size(14.0),
                        );
                        let enabled =
                            !self.timers_running && self.disabled_field != DisabledField::Count;
                        let te = styled_text_edit(&mut self.input_count, "", enabled);
                        let r = ui.add_sized([ui.available_width(), 24.0], te);
                        if r.changed() {
                            self.input_count.retain(|c| c.is_ascii_digit());
                            if let Ok(v) = self.input_count.trim().parse::<u32>() {
                                if v > MAX_COUNT {
                                    self.input_count = MAX_COUNT.to_string();
                                }
                            }
                            fields_changed = true;
                        }
                    });

                    // Duration
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Total Duration:")
                                .color(TEXT_WHITE)
                                .size(14.0),
                        );
                        let enabled =
                            !self.timers_running && self.disabled_field != DisabledField::Duration;
                        let te = styled_text_edit(&mut self.input_duration, placeholder, enabled);
                        let r = ui.add_sized([ui.available_width(), 24.0], te);
                        if r.changed() {
                            self.input_duration
                                .retain(|c| c.is_ascii_digit() || ".hmsHMS: ".contains(c));
                            fields_changed = true;
                        }
                    });

                    // Offset
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Offset (First Timer):")
                                .color(TEXT_WHITE)
                                .size(14.0),
                        );
                        let te =
                            styled_text_edit(&mut self.input_offset, placeholder, !self.timers_running);
                        let r = ui.add_sized([ui.available_width(), 24.0], te);
                        if r.changed() {
                            self.input_offset
                                .retain(|c| c.is_ascii_digit() || ".hmsHMS: ".contains(c));
                            fields_changed = true;
                        }
                    });

                    // Speed slider — full width
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Speed:").color(TEXT_WHITE).size(14.0));
                        let value_width = 45.0;
                        let spacing = ui.spacing().item_spacing.x;
                        let slider_width = (ui.available_width() - value_width - spacing).max(60.0);
                        ui.spacing_mut().slider_width = slider_width;
                        ui.add(
                            egui::Slider::new(&mut self.speed_pct, 50..=200)
                                .show_value(false)
                                .step_by(5.0), // 0.05x steps
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "{:.2}x",
                                self.speed_pct as f64 / 100.0
                            ))
                            .color(TEXT_WHITE)
                            .size(14.0),
                        );
                    });

                    if fields_changed {
                        self.update_field_states();
                    }
                });

                // ── Geometric + Display mode (two stacked rows) · Time adjust ──
                // The ±15s buttons grow to span both checkbox rows; the top/bottom
                // margins to the settings group and the controls row stay put.
                ui.horizontal(|ui| {
                    let cohort_live = self.timers_running;
                    let row_h = 24.0_f32;
                    let gap = ui.spacing().item_spacing.y;
                    let tall_h = row_h * 2.0 + gap;

                    let btn_w = 50.0_f32;
                    let sp = ui.spacing().item_spacing.x;
                    let button_area = btn_w * 2.0 + sp;
                    let left_w = (ui.available_width() - button_area - sp).max(140.0);

                    let mut geo_changed = false;
                    // Left edge of the Randomize Intervals checkbox, captured in
                    // row 1 so the distribution switch in row 2 sits beneath it.
                    let mut rand_x: Option<f32> = None;

                    ui.vertical(|ui| {
                        ui.set_width(left_w);

                        // Row 1 (top): Geometric Series checkbox + ratio entry
                        ui.allocate_ui_with_layout(
                            egui::vec2(left_w, row_h),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                let old_pad = ui.spacing().button_padding;
                                ui.spacing_mut().button_padding.y = 3.5;
                                let randomized = self.randomize_enabled;
                                let geo_enabled = !cohort_live && !randomized;
                                let mut geo = self.geometric_enabled;
                                let cb = ui.add_enabled(
                                    geo_enabled,
                                    egui::Checkbox::new(
                                        &mut geo,
                                        egui::RichText::new("Geometric Series")
                                            .color(if geo_enabled { TEXT_WHITE } else { TEXT_DIM })
                                            .size(12.0),
                                    ),
                                );
                                ui.spacing_mut().button_padding = old_pad;
                                if cb.changed() {
                                    self.geometric_enabled = geo;
                                    self.config.geometric_enabled = geo;
                                    self.config.save();
                                    geo_changed = true;
                                }

                                // Ratio entry box (fits "1.0000"). Freely editable
                                // from a fresh state — no need to check the box first;
                                // it only greys/locks once a cohort is live, intervals
                                // are randomized, or it becomes the auto-computed field.
                                let ratio_computed = self.disabled_field == DisabledField::Ratio;
                                let ratio_enabled = !cohort_live && !randomized && !ratio_computed;
                                let ratio_color =
                                    if ratio_enabled { TEXT_WHITE } else { DISABLED_BORDER };
                                let te = egui::TextEdit::singleline(&mut self.input_ratio)
                                    .hint_text(if ratio_enabled { "ratio" } else { "" })
                                    .text_color(ratio_color)
                                    .horizontal_align(egui::Align::Center)
                                    .interactive(ratio_enabled);
                                let r = ui.add_sized([56.0, row_h - 2.0], te);
                                if r.changed() {
                                    sanitize_ratio(&mut self.input_ratio);
                                    // Only the cascade calc cares about the ratio, so
                                    // typing it while the mode is off does nothing yet.
                                    if self.geometric_enabled {
                                        geo_changed = true;
                                    }
                                }
                                if r.lost_focus() {
                                    auto_extend_ratio(&mut self.input_ratio);
                                }

                                if self.geometric_enabled && self.geom_invalid {
                                    ui.label(
                                        egui::RichText::new("!").color(ERR_RED).strong().size(15.0),
                                    )
                                    .on_hover_text(
                                        "No geometric series fits those values \
                                         (Total Duration must exceed the base Interval).",
                                    );
                                }

                                // Randomize Intervals checkbox. Only checkable from
                                // the blank/cleared state: once a non-random cohort
                                // exists it greys out until Clear All. It stays live
                                // while a random cohort exists so it can be unchecked.
                                let rand_enabled =
                                    !cohort_live && (self.timers.is_empty() || randomized);
                                let old_pad = ui.spacing().button_padding;
                                ui.spacing_mut().button_padding.y = 3.5;
                                let mut rnd = self.randomize_enabled;
                                let cb = ui.add_enabled(
                                    rand_enabled,
                                    egui::Checkbox::new(
                                        &mut rnd,
                                        egui::RichText::new("Randomize Intervals")
                                            .color(if rand_enabled { TEXT_WHITE } else { TEXT_DIM })
                                            .size(12.0),
                                    ),
                                );
                                ui.spacing_mut().button_padding = old_pad;
                                rand_x = Some(cb.rect.min.x);
                                if cb.changed() {
                                    self.set_randomize(rnd);
                                    geo_changed = true;
                                }

                                // σ entry — only exists while the switch is on
                                // "gaussian". Digits and '.' only; the unit is
                                // whatever Total Duration is expressed in.
                                if self.randomize_gaussian {
                                    let sigma_enabled = !cohort_live;
                                    let sigma_color =
                                        if sigma_enabled { TEXT_WHITE } else { DISABLED_BORDER };
                                    let te = egui::TextEdit::singleline(&mut self.input_sigma)
                                        .text_color(sigma_color)
                                        .horizontal_align(egui::Align::Center)
                                        .interactive(sigma_enabled);
                                    let r = ui.add_sized([56.0, row_h - 2.0], te);
                                    // The σ hint is painted by hand rather than via
                                    // hint_text: it reads small at the default 12.5
                                    // body size (so it's bumped one point), and
                                    // having no ascender its visual mass sits below
                                    // "ratio" in the sibling box, so it's nudged up
                                    // a touch. Typed digits keep the stock position.
                                    if sigma_enabled && self.input_sigma.is_empty() {
                                        ui.painter().text(
                                            r.rect.center() - egui::vec2(0.0, 2.0),
                                            egui::Align2::CENTER_CENTER,
                                            "σ",
                                            egui::FontId::proportional(13.5),
                                            ui.visuals().weak_text_color(),
                                        );
                                    }
                                    let (_, unit_name) = duration_unit(&self.input_duration);
                                    let r = r.on_hover_text(format!(
                                        "Std. deviation of each gap, in {unit_name} \
                                         (inherited from Total Duration). \
                                         The mean is fixed at the evenly-spaced interval."
                                    ));
                                    if r.changed() {
                                        sanitize_sigma(&mut self.input_sigma);
                                        if self.randomize_enabled {
                                            geo_changed = true;
                                        }
                                    }
                                }
                            },
                        );

                        // Row 2 (bottom): Show Progress Bars checkbox
                        ui.allocate_ui_with_layout(
                            egui::vec2(left_w, row_h),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                let old_pad = ui.spacing().button_padding;
                                ui.spacing_mut().button_padding.y = 3.5;
                                let mut pb = self.progress_bar_mode;
                                let cb = ui.checkbox(
                                    &mut pb,
                                    egui::RichText::new("Show Progress Bars")
                                        .color(TEXT_WHITE)
                                        .size(12.0),
                                );
                                ui.spacing_mut().button_padding = old_pad;
                                if cb.changed() {
                                    self.progress_bar_mode = pb;
                                    self.config.progress_bar_mode = pb;
                                    self.config.save();
                                }

                                // Distribution switch: uniform ⇄ gaussian, aligned
                                // under the Randomize Intervals checkbox above.
                                if let Some(x) = rand_x {
                                    let pad = x - ui.cursor().min.x - ui.spacing().item_spacing.x;
                                    if pad > 0.0 {
                                        ui.add_space(pad);
                                    }
                                }
                                let switch_enabled = !cohort_live;
                                let label_color =
                                    if switch_enabled { TEXT_WHITE } else { TEXT_DIM };
                                ui.label(
                                    egui::RichText::new("uniform").color(label_color).size(8.5),
                                );
                                let mut gaussian = self.randomize_gaussian;
                                let sw = toggle_switch(ui, &mut gaussian, switch_enabled);
                                ui.label(
                                    egui::RichText::new("gaussian").color(label_color).size(8.5),
                                );
                                if sw.changed() {
                                    self.randomize_gaussian = gaussian;
                                    self.config.randomize_gaussian = gaussian;
                                    self.config.save();
                                    if self.randomize_enabled {
                                        geo_changed = true;
                                    }
                                }
                            },
                        );
                    });

                    // Right: tall +15s / -15s buttons spanning both rows
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if accent_button_sized(ui, "+15s", egui::vec2(btn_w, tall_h)).clicked() {
                            self.adjust_time(15.0);
                        }
                        if accent_button_sized(ui, "-15s", egui::vec2(btn_w, tall_h)).clicked() {
                            self.adjust_time(-15.0);
                        }
                    });

                    if geo_changed {
                        self.update_field_states();
                        self.needs_rebuild = true;
                    }
                });

                // ── Controls ──
                ui.horizontal(|ui| {
                    let reset_width = 34.0;
                    let spacing = ui.spacing().item_spacing.x;
                    let btn_width =
                        (ui.available_width() - spacing * 3.0 - reset_width) / 3.0;

                    if accent_button_sized(ui, "Start All", egui::vec2(btn_width, 30.0)).clicked()
                    {
                        self.start_all();
                    }
                    if accent_button_sized(ui, "Pause All", egui::vec2(btn_width, 30.0)).clicked()
                    {
                        self.pause_all();
                    }

                    // Reset button with replay icon
                    let reset_icon = egui::Image::new(
                        egui::include_image!("../assets/replay.svg"),
                    )
                    .fit_to_exact_size(egui::vec2(18.0, 18.0))
                    .tint(TEXT_WHITE);
                    let reset_btn = egui::Button::image(reset_icon)
                        .fill(ACCENT)
                        .corner_radius(4.0);
                    let reset_resp = ui.add_sized(egui::vec2(reset_width, 30.0), reset_btn);
                    if reset_resp.clicked() {
                        self.reset_all();
                    }
                    reset_resp.on_hover_text("Reset all timers");

                    if self.all_timers_completed {
                        let reset_all_btn = egui::Button::new(
                            egui::RichText::new("Reset All")
                                .color(TEXT_WHITE)
                                .family(egui::FontFamily::Name(FONT_BOLD.into()))
                                .size(14.0),
                        )
                        .fill(RESET_GREEN)
                        .stroke(egui::Stroke::NONE)
                        .corner_radius(4.0)
                        .min_size(egui::vec2(btn_width, 30.0));
                        if ui.add(reset_all_btn).clicked() {
                            self.reset_all();
                        }
                    } else {
                        let clear_btn = egui::Button::new(
                            egui::RichText::new("Clear All")
                                .color(TEXT_WHITE)
                                .family(egui::FontFamily::Name(FONT_BOLD.into()))
                                .size(14.0),
                        )
                        .fill(CLEAR_RED)
                        .corner_radius(4.0)
                        .min_size(egui::vec2(btn_width, 30.0));
                        if ui.add(clear_btn).clicked() {
                            self.pending_clear = true;
                        }
                    }
                });

                // Clear confirmation dialog
                if self.pending_clear {
                    egui::Window::new("Confirm Clear")
                        .collapsible(false)
                        .resizable(false)
                        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                        .frame(
                            egui::Frame::new()
                                .fill(BG_WIDGET)
                                .stroke(egui::Stroke::new(1.0, ACCENT))
                                .corner_radius(8.0)
                                .inner_margin(16.0),
                        )
                        .show(ctx, |ui| {
                            ui.label(
                                egui::RichText::new(
                                    "Are you sure you want to clear all timers?",
                                )
                                .color(TEXT_WHITE)
                                .size(14.0),
                            );
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if accent_button_sized(ui, "Yes", egui::vec2(60.0, 28.0)).clicked()
                                {
                                    self.pending_clear = false;
                                    self.clear_all();
                                }
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new("No")
                                                .color(TEXT_WHITE)
                                                .family(egui::FontFamily::Name(FONT_BOLD.into())),
                                        )
                                        .fill(BTN_SECONDARY)
                                        .corner_radius(4.0)
                                        .min_size(egui::vec2(60.0, 28.0)),
                                    )
                                    .clicked()
                                {
                                    self.pending_clear = false;
                                }
                            });
                        });
                }

                ui.separator();

                // ── Timers List ──

                // Determine display mode for each timer:
                // 0 = countdown text, 1 = progress bar, 2 = dimmed interval label
                let next_timer_id: Option<u64> = self
                    .timers
                    .iter()
                    .filter(|t| !t.is_alerting && t.remaining_seconds > 0.0)
                    .min_by(|a, b| {
                        a.remaining_seconds
                            .partial_cmp(&b.remaining_seconds)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|t| t.id);

                let pb_mode = self.progress_bar_mode;
                // Per-timer gap label from the gap stored at creation (earlier
                // timers may already be removed, so neighbors can't be trusted).
                let gap_labels: Vec<String> = self
                    .timers
                    .iter()
                    .map(|t| {
                        let secs = t.gap_seconds as u64;
                        let h = secs / 3600;
                        let m = (secs % 3600) / 60;
                        let s = secs % 60;
                        if h > 0 {
                            format!("+ {h:02}:{m:02}:{s:02}")
                        } else {
                            format!("+ {m:02}:{s:02}")
                        }
                    })
                    .collect();

                // display_mode: 0=text, 1=progress bar, 2=dimmed interval
                let display_list: Vec<(usize, u8)> = self
                    .timers
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        if t.is_alerting {
                            (i, 0)
                        } else if pb_mode && Some(t.id) == next_timer_id {
                            (i, 1)
                        } else if pb_mode {
                            (i, 2)
                        } else {
                            (i, 0)
                        }
                    })
                    .collect();

                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .max_height(ui.available_height() - 100.0)
                    .show(ui, |ui| {
                        let mut silence_ids = Vec::new();
                        let mut cancel_ids = Vec::new();

                        for &(idx, display_mode) in &display_list {
                            let timer = &self.timers[idx];

                            let frame_color = if timer.is_alerting { ACCENT } else { BG_WIDGET };
                            let border_color = if timer.is_alerting {
                                TEXT_WHITE
                            } else if display_mode == 2 {
                                DISABLED_BORDER
                            } else {
                                ACCENT
                            };
                            let border_width = if timer.is_alerting { 2.0 } else { 1.0 };

                            egui::Frame::new()
                                .fill(frame_color)
                                .stroke(egui::Stroke::new(border_width, border_color))
                                .corner_radius(5.0)
                                .inner_margin(egui::Margin::symmetric(8, 6))
                                .outer_margin(egui::Margin::symmetric(0, 2))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let action_button_width = if timer.is_alerting { 74.0 } else { 30.0 };
                                        let spacing = ui.spacing().item_spacing.x;
                                        let content_width =
                                            (ui.available_width() - action_button_width - spacing)
                                                .max(0.0);

                                        match display_mode {
                                            1 => {
                                                // Progress bar for next timer, normalized to
                                                // its inter-timer gap so it starts full.
                                                let gap = timer.gap_seconds;
                                                let frac = if gap > 0.0 {
                                                    (timer.remaining_seconds / gap)
                                                        .clamp(0.0, 1.0)
                                                        as f32
                                                } else {
                                                    0.0
                                                };
                                                let pbar =
                                                    egui::ProgressBar::new(frac).fill(ACCENT);
                                                ui.add_sized([content_width, 20.0], pbar);
                                            }
                                            2 => {
                                                // Dimmed gap label (time since previous timer)
                                                ui.add_sized(
                                                    [content_width, 20.0],
                                                    egui::Label::new(
                                                        egui::RichText::new(&gap_labels[idx])
                                                            .color(TEXT_DIM)
                                                            .italics()
                                                            .family(egui::FontFamily::Name(
                                                                FONT_TIMER.into(),
                                                            ))
                                                            .size(14.0),
                                                    ),
                                                );
                                            }
                                            _ => {
                                                // Countdown text
                                                let timer_font = egui::FontFamily::Name(
                                                    FONT_TIMER.into(),
                                                );
                                                let text = if timer.is_alerting {
                                                    egui::RichText::new("00:00")
                                                        .color(TEXT_WHITE)
                                                        .family(timer_font)
                                                        .size(16.0)
                                                } else {
                                                    egui::RichText::new(timer.format_time())
                                                        .color(TEXT_WHITE)
                                                        .family(timer_font)
                                                        .size(16.0)
                                                };
                                                ui.add_sized(
                                                    [content_width, 20.0],
                                                    egui::Label::new(text),
                                                );
                                            }
                                        }

                                        if timer.is_alerting {
                                            let btn = egui::Button::new(
                                                egui::RichText::new("SILENCE")
                                                    .color(ACCENT)
                                                    .family(egui::FontFamily::Name(
                                                        FONT_BOLD.into(),
                                                    )),
                                            )
                                            .fill(TEXT_WHITE)
                                            .corner_radius(3.0)
                                            .min_size(egui::vec2(action_button_width, 20.0));
                                            if ui.add_sized([action_button_width, 20.0], btn).clicked() {
                                                silence_ids.push(timer.id);
                                            }
                                        } else {
                                            let btn = egui::Button::new(
                                                egui::RichText::new("X")
                                                    .color(TEXT_WHITE)
                                                    .family(egui::FontFamily::Name(
                                                        FONT_BOLD.into(),
                                                    )),
                                            )
                                            .fill(ACCENT)
                                            .corner_radius(3.0)
                                            .min_size(egui::vec2(action_button_width, 20.0));
                                            if ui.add_sized([action_button_width, 20.0], btn).clicked() {
                                                cancel_ids.push(timer.id);
                                            }
                                        }
                                    });
                                });
                        }

                        // Process button clicks
                        for id in silence_ids {
                            if let Some(t) = self.timers.iter_mut().find(|t| t.id == id) {
                                t.silence();
                            }
                            self.stop_sound();
                        }
                        for id in cancel_ids {
                            self.timers.retain(|t| t.id != id);
                        }
                    });

                // ── Sound Section (bottom) ──
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    // Row 1 (bottommost): Regular alert button + Preview + Repeat controls
                    ui.horizontal(|ui| {
                        if secondary_button(ui, "Select Alert Sound...").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Audio Files", &["wav", "mp3"])
                                .pick_file()
                            {
                                self.sound_file_display = path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "Unknown".to_string());
                                self.config.sound_file = path.to_string_lossy().to_string();
                                self.config.save();
                                if let Some(ref mut audio) = self.audio {
                                    audio.set_sound(path);
                                }
                            }
                        }

                        let has_sound =
                            self.audio.as_ref().is_some_and(|a| a.sound_path.is_some());
                        ui.add_enabled_ui(has_sound, |ui| {
                            if secondary_button(ui, "Preview").clicked() {
                                if let Some(ref mut audio) = self.audio {
                                    audio.preview();
                                }
                            }
                        });

                        let reload_resp = secondary_button(ui, "Reload Audio");
                        if reload_resp.clicked() {
                            self.reload_audio();
                        }
                        reload_resp.on_hover_text(
                            "Reconnect audio output — use after unplugging or switching your audio device",
                        );

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if self.repeat_alert_enabled {
                                ui.label(
                                    egui::RichText::new("x")
                                        .color(TEXT_WHITE)
                                        .size(12.0),
                                );

                                let te = egui::TextEdit::singleline(&mut self.repeat_alert_count_str)
                                    .desired_width(20.0)
                                    .horizontal_align(egui::Align::Center)
                                    .font(egui::TextStyle::Body);
                                let response = ui.add(te);

                                if response.has_focus() {
                                    let up = ui.input(|i| i.key_pressed(egui::Key::ArrowUp));
                                    let down = ui.input(|i| i.key_pressed(egui::Key::ArrowDown));
                                    if up {
                                        self.repeat_alert_count = self.repeat_alert_count.saturating_add(1);
                                        self.repeat_alert_count_str = self.repeat_alert_count.to_string();
                                    }
                                    if down && self.repeat_alert_count > 1 {
                                        self.repeat_alert_count -= 1;
                                        self.repeat_alert_count_str = self.repeat_alert_count.to_string();
                                    }
                                }

                                if response.changed() {
                                    if let Ok(n) = self.repeat_alert_count_str.trim().parse::<u32>() {
                                        if n >= 1 {
                                            self.repeat_alert_count = n;
                                        }
                                    }
                                }

                                if response.lost_focus() {
                                    self.repeat_alert_count_str = self.repeat_alert_count.to_string();
                                }
                            }

                            let old_pad = ui.spacing().button_padding;
                            ui.spacing_mut().button_padding.y = 3.5;
                            ui.checkbox(
                                &mut self.repeat_alert_enabled,
                                egui::RichText::new("Repeat Alert?")
                                    .color(TEXT_WHITE)
                                    .size(12.0),
                            );
                            ui.spacing_mut().button_padding = old_pad;
                        });
                    });

                    // Row 2: Final alert button + Preview
                    ui.horizontal(|ui| {
                        if secondary_button(ui, "Select Final Alert (optional)...").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Audio Files", &["wav", "mp3"])
                                .pick_file()
                            {
                                self.final_sound_file_display = path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "Unknown".to_string());
                                self.config.final_sound_file = path.to_string_lossy().to_string();
                                self.config.save();
                                self.final_sound_path = Some(path);
                            }
                        }

                        let has_final_sound = self.final_sound_path.is_some();
                        ui.add_enabled_ui(has_final_sound, |ui| {
                            if secondary_button(ui, "Preview").clicked() {
                                if let Some(path) = self.final_sound_path.clone() {
                                    if let Some(ref mut audio) = self.audio {
                                        audio.preview_file(&path);
                                    }
                                }
                            }
                        });
                    });

                    // Row 3: Regular sound name label (with optional ⛔ clear button)
                    ui.horizontal(|ui| {
                        let has_regular_sound =
                            self.audio.as_ref().is_some_and(|a| a.sound_path.is_some());
                        if has_regular_sound {
                            let clear = egui::Label::new(
                                egui::RichText::new("⛔").size(11.0),
                            )
                            .selectable(false)
                            .sense(egui::Sense::click());
                            let resp = ui.add(clear).on_hover_cursor(egui::CursorIcon::PointingHand);
                            if resp.on_hover_text("Clear alert sound").clicked() {
                                self.config.sound_file = String::new();
                                self.config.save();
                                self.sound_file_display.clear();
                                if let Some(ref mut audio) = self.audio {
                                    audio.sound_path = None;
                                }
                            }
                        }
                        ui.label(
                            egui::RichText::new(&self.sound_file_display)
                                .color(TEXT_DIM)
                                .italics()
                                .size(11.0),
                        );
                    });

                    // Row 4 (topmost): Final sound name (with ⛔ clear) or reserved blank space
                    if self.final_sound_path.is_some() {
                        ui.horizontal(|ui| {
                            let clear = egui::Label::new(
                                egui::RichText::new("⛔").size(11.0),
                            )
                            .selectable(false)
                            .sense(egui::Sense::click());
                            let resp = ui.add(clear).on_hover_cursor(egui::CursorIcon::PointingHand);
                            if resp.on_hover_text("Clear final alert sound").clicked() {
                                self.config.final_sound_file = String::new();
                                self.config.save();
                                self.final_sound_file_display.clear();
                                self.final_sound_path = None;
                            }
                            ui.label(
                                egui::RichText::new(&self.final_sound_file_display)
                                    .color(TEXT_DIM)
                                    .italics()
                                    .size(11.0),
                            );
                        });
                    } else {
                        ui.allocate_space(egui::vec2(ui.available_width(), 15.0));
                    }
                });
            });
    }
}

// ─── Entry point ─────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn enable_per_monitor_v2_dpi() {
    use std::ffi::c_void;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn SetProcessDpiAwarenessContext(value: *mut c_void) -> i32;
    }

    let ctx_per_monitor_v2 = -4isize as *mut c_void;
    unsafe {
        let _ = SetProcessDpiAwarenessContext(ctx_per_monitor_v2);
    }
}

fn main() -> eframe::Result {
    #[cfg(target_os = "windows")]
    enable_per_monitor_v2_dpi();

    let icon_data = load_icon();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 630.0])
            .with_min_inner_size([400.0, 430.0])
            .with_icon(icon_data),
        ..Default::default()
    };

    eframe::run_native(
        "Cascading Timers",
        options,
        Box::new(|cc| Ok(Box::new(CascadingTimersApp::new(cc)))),
    )
}

fn load_icon() -> egui::IconData {
    // Try current exe directory, then parent directories.
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(mut dir) = exe_path.parent().map(|p| p.to_path_buf()) {
            for _ in 0..5 {
                let path = dir.join("Untitled-5(1).ico");
                if let Ok(img) = image::open(&path) {
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    return egui::IconData {
                        rgba: rgba.into_raw(),
                        width: w,
                        height: h,
                    };
                }
                if !dir.pop() {
                    break;
                }
            }
        }
    }

    // Fallback: solid accent-colored icon
    let size = 32u32;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for _ in 0..(size * size) {
        pixels.extend_from_slice(&[0xFC, 0x03, 0x5E, 0xFF]);
    }
    egui::IconData {
        rgba: pixels,
        width: size,
        height: size,
    }
}
