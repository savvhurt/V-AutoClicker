use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use egui::{self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Ui, Vec2};
use egui_software_backend::{SoftwareBackend, SoftwareBackendAppConfiguration};

use super::config::{self, Theme};
use super::profiles::{self, ActiveProfile, Preset, Profile};
use super::worker::{Command, Event, Worker};
use super::{theme, widgets};

use crate::i18n::{self, Key, Lang, Language};
use crate::state::{
    Button, ClickMode, MAX_DELAY_MS, MAX_LIMIT, MAX_POINT_INTERVAL_MS, MAX_POINTS,
    MAX_RANDOM_INTERVAL_MS, MIN_RANDOM_INTERVAL_MS, Point, Settings, Status, StopReason,
    format_decimal,
};
use crate::update::{self, Release};

const APPLY_DEBOUNCE: Duration = Duration::from_millis(250);
const SYNC_GRACE: Duration = Duration::from_millis(800);
const TOGGLE_TIMEOUT: Duration = Duration::from_secs(3);
const STATUS_STALE_AFTER: Duration = Duration::from_secs(3);
const WORKER_RESTART_COOLDOWN: Duration = Duration::from_secs(10);
const START_GUARD: Duration = Duration::from_millis(1500);
const CONTENT_WIDTH: f32 = 424.0;
// Windows has no daemon footer, so the advanced layout can be shorter there.
#[cfg(windows)]
const WINDOW_SIZE: [f32; 2] = [460.0, 560.0];
#[cfg(not(windows))]
const WINDOW_SIZE: [f32; 2] = [460.0, 600.0];
#[cfg(windows)]
const MIN_WINDOW_SIZE: [f32; 2] = [420.0, 480.0];
#[cfg(not(windows))]
const MIN_WINDOW_SIZE: [f32; 2] = [420.0, 520.0];
const SIMPLE_WINDOW_SIZE: [f32; 2] = [380.0, 260.0];
const SIMPLE_MIN_WINDOW_SIZE: [f32; 2] = [360.0, 250.0];
const LABEL_WIDTH: f32 = 124.0;
const MIN_AUTO_SCALE: f32 = 0.75;
const MAX_AUTO_SCALE: f32 = 1.0;
const SCREEN_MARGIN_X: f32 = 48.0;
const SCREEN_MARGIN_Y: f32 = 96.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Tab {
    Clicks,
    Mode,
    Hotkeys,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
enum RecordTarget {
    Toggle,
    Hold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PipetteTarget {
    Point,
    Sequence(usize),
    NewSequence,
}

struct Pipette {
    target: PipetteTarget,
    receiver: Receiver<std::result::Result<Option<Point>, String>>,
}

/// State of the System tab's update controls. All network work happens on a
/// background thread and reports back through `UpdateEvent`.
enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading { progress: f32 },
    Ready { release: Release, bytes: Vec<u8> },
    Installing,
    Failed(String),
}

enum UpdateEvent {
    Checked(std::result::Result<Option<Release>, String>),
    Progress(f32),
    Downloaded(std::result::Result<(Release, Vec<u8>), (Release, String)>),
}

struct SliderSpec<'a> {
    range: std::ops::RangeInclusive<f64>,
    logarithmic: bool,
    step: f64,
    decimals: usize,
    suffix: &'a str,
}

struct Confirmation<'a> {
    id: &'a str,
    title: Key,
    question: Key,
    text: Key,
    confirm: Key,
    solid: bool,
}

enum ProfileDialog {
    SaveAs { name: String },
    Export { path: String },
    Import { path: String },
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DraftSettings {
    cps: f64,
    button: Button,
    hold_ms: f64,
    jitter_pct: f64,
    limit: u64,
    delay_ms: f64,
    multi_click: u8,
    mode: ClickMode,
    target: Option<Point>,
    points: [Option<Point>; MAX_POINTS],
    point_interval_ms: f64,
    random_interval_ms: f64,
}

impl From<Settings> for DraftSettings {
    fn from(settings: Settings) -> Self {
        Self {
            cps: settings.cps,
            button: settings.button,
            hold_ms: settings.hold_ms(),
            jitter_pct: settings.jitter_pct,
            limit: settings.limit,
            delay_ms: settings.delay_ms(),
            multi_click: settings.multi_click,
            mode: settings.mode,
            target: settings.target,
            points: settings.points,
            point_interval_ms: settings.point_interval_ms(),
            random_interval_ms: settings.random_interval_ms(),
        }
    }
}

impl DraftSettings {
    fn points_vec(&self) -> Vec<Point> {
        self.points.iter().flatten().copied().collect()
    }

    fn point_count(&self) -> usize {
        self.points.iter().flatten().count()
    }

    fn point_at(&self, index: usize) -> Option<Point> {
        self.points.iter().flatten().nth(index).copied()
    }

    fn set_point(&mut self, index: usize, point: Point) {
        for (current, existing) in self.points.iter_mut().flatten().enumerate() {
            if current == index {
                *existing = point;
                return;
            }
        }
    }

    fn add_point(&mut self, point: Point) -> Option<usize> {
        let slot = self.points.iter_mut().find(|slot| slot.is_none())?;
        *slot = Some(point);
        Some(self.point_count() - 1)
    }

    fn remove_point(&mut self, index: usize) {
        let mut points = self.points_vec();
        if index < points.len() {
            points.remove(index);
        }
        self.points = [None; MAX_POINTS];
        for (slot, point) in self.points.iter_mut().zip(points) {
            *slot = Some(point);
        }
    }

    fn to_settings(self) -> Result<Settings, String> {
        let points = self.points_vec();
        let interval = self
            .point_interval_ms
            .round()
            .clamp(1.0, MAX_POINT_INTERVAL_MS as f64) as u32;
        let random_interval = self
            .random_interval_ms
            .round()
            .clamp(MIN_RANDOM_INTERVAL_MS as f64, MAX_RANDOM_INTERVAL_MS as f64)
            as u32;

        Settings::new(self.cps, self.button, self.hold_ms, self.jitter_pct)
            .and_then(|settings| settings.with_limit(self.limit))
            .and_then(|settings| settings.with_delay_ms(self.delay_ms))
            .and_then(|settings| settings.with_multi_click(self.multi_click))
            .map(|settings| settings.with_mode(self.mode))
            .and_then(|settings| settings.with_target(self.target))
            .and_then(|settings| settings.with_points(&points))
            .and_then(|settings| settings.with_point_interval_ms(interval))
            .and_then(|settings| settings.with_random_interval_ms(random_interval))
            .map_err(|error| format!("{error:#}"))
    }
}

pub struct GuiApp {
    settings: DraftSettings,
    tab: Tab,
    hotkey: String,
    hold_hotkey: String,
    recording: Option<RecordTarget>,
    theme: Theme,
    language: Language,
    lang: Lang,
    applied_scale: Option<f32>,
    simple_mode: bool,
    always_on_top: bool,
    minimize_to_tray: bool,
    check_updates: bool,
    update: UpdateState,
    update_events: Receiver<UpdateEvent>,
    update_sender: Sender<UpdateEvent>,
    auto_check_pending: bool,
    auto_check_at: Option<Instant>,
    confirm_update: bool,
    #[cfg_attr(not(windows), allow(dead_code))]
    start_hidden: bool,
    #[cfg(windows)]
    elevated: bool,
    #[cfg(windows)]
    tray: Option<super::tray::Tray>,
    pipette: Option<Pipette>,
    profiles: Vec<Profile>,
    active_profile: ActiveProfile,
    profile_dialog: Option<ProfileDialog>,
    confirm_profile_delete: bool,
    confirm_defaults: bool,
    autostart_enabled: bool,
    online: bool,
    enabled: bool,
    actual_cps: f64,
    session_clicks: u64,
    starting: bool,
    delay_left_ms: u64,
    stop_reason: Option<StopReason>,
    hold_active: bool,
    daemon_error: Option<String>,
    daemon_hotkey_active: String,
    daemon_hold_hotkey_active: String,
    daemon_hotkey_paused: bool,
    daemon_hotkey_error: Option<String>,
    daemon_settings: Option<Settings>,
    daemon_protocol: u32,
    #[cfg_attr(not(windows), allow(dead_code))]
    legacy_restart_attempted: bool,
    #[cfg_attr(not(windows), allow(dead_code))]
    legacy_restart_pending: bool,
    #[cfg_attr(not(windows), allow(dead_code))]
    last_hold_hotkey_sync: Option<Instant>,
    last_hotkey_sync: Option<Instant>,
    last_status_at: Option<Instant>,
    last_worker_restart: Option<Instant>,
    notice: Option<String>,
    offline_reason: Option<String>,
    initialized: bool,
    draft_changed_at: Option<Instant>,
    last_apply_sent: Option<Instant>,
    pending_toggle: Option<(bool, Instant)>,
    start_guard: Option<u64>,
    start_token: Arc<AtomicU64>,
    #[cfg(not(windows))]
    confirm_quit: bool,
    commands: Sender<Command>,
    events: Receiver<Event>,
    worker: Option<thread::JoinHandle<()>>,
    worker_stop: Arc<AtomicBool>,
    saver: config::Saver,
}

impl GuiApp {
    fn new(ctx: egui::Context, start_hidden: bool) -> Self {
        let loaded = config::load();
        let config = loaded.config;
        let lang = config.language.resolve();
        theme::install(&ctx, config.theme);
        let worker = Worker::spawn(
            ctx.clone(),
            config.hotkey.clone(),
            config.hold_hotkey.clone(),
            config.language,
        )
        .expect("failed to start the V-AutoClicker GUI IPC worker");
        let worker_stop = worker.stop_flag();
        let autostart_enabled = crate::autostart::is_enabled();
        let (update_sender, update_events) = mpsc::channel();
        // A stale `.old-` or `.new-` file from an interrupted update can be
        // removed before the next install starts.
        update::cleanup_stale_files();

        #[cfg_attr(not(windows), allow(unused_mut))]
        let mut app = Self {
            settings: config.settings.into(),
            tab: Tab::Clicks,
            hotkey: config.hotkey,
            hold_hotkey: config.hold_hotkey,
            recording: None,
            theme: config.theme,
            language: config.language,
            lang,
            applied_scale: None,
            simple_mode: config.simple_mode,
            // Keep the saved preference on unsupported sessions (Wayland) so
            // switching back to X11 or Windows restores it at startup.
            always_on_top: config.always_on_top,
            minimize_to_tray: config.minimize_to_tray,
            check_updates: config.check_updates,
            update: UpdateState::Idle,
            update_events,
            update_sender,
            auto_check_pending: false,
            auto_check_at: (config.check_updates && update_supported())
                .then(|| Instant::now() + Duration::from_secs(1)),
            confirm_update: false,
            start_hidden,
            #[cfg(windows)]
            elevated: crate::platform::is_elevated(),
            #[cfg(windows)]
            tray: None,
            pipette: None,
            profiles: config.profiles,
            active_profile: config.active_profile,
            profile_dialog: None,
            confirm_profile_delete: false,
            confirm_defaults: false,
            autostart_enabled,
            online: false,
            enabled: false,
            actual_cps: 0.0,
            session_clicks: 0,
            starting: false,
            delay_left_ms: 0,
            stop_reason: None,
            hold_active: false,
            daemon_error: None,
            daemon_hotkey_active: String::new(),
            daemon_hold_hotkey_active: String::new(),
            daemon_hotkey_paused: false,
            daemon_hotkey_error: None,
            daemon_settings: None,
            daemon_protocol: 0,
            legacy_restart_attempted: false,
            legacy_restart_pending: false,
            last_hold_hotkey_sync: None,
            last_hotkey_sync: None,
            last_status_at: None,
            last_worker_restart: None,
            notice: loaded.warning,
            offline_reason: None,
            initialized: false,
            draft_changed_at: None,
            last_apply_sent: None,
            pending_toggle: None,
            start_guard: None,
            start_token: Arc::new(AtomicU64::new(0)),
            #[cfg(not(windows))]
            confirm_quit: false,
            commands: worker.commands,
            events: worker.events,
            worker: worker.handle,
            worker_stop,
            saver: config::Saver::new(),
        };

        #[cfg(windows)]
        {
            // Start the icon before the first frame: an autostart launch can be
            // hidden, and a hidden winit window does not necessarily receive a
            // natural redraw that would otherwise create the icon.
            app.sync_tray(&ctx);
            if app.start_hidden {
                if app.tray.is_some() {
                    ctx.request_repaint();
                } else {
                    // The tray icon could not be created. Never leave a hidden
                    // process without any way to open or close it.
                    app.start_hidden = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                }
            }

            if !app.elevated {
                let warning = app.t(Key::ElevationWarn).to_string();
                app.notice = Some(match app.notice.take() {
                    Some(existing) => format!("{existing}\n{warning}"),
                    None => warning,
                });
            }
        }

        app
    }

    fn t(&self, key: Key) -> &'static str {
        i18n::tr(self.lang, key)
    }

    /// Fill `{}`/`{:.1}` placeholders in a translated template in order.
    fn fill(&self, key: Key, args: &[String]) -> String {
        let mut result = String::new();
        let mut rest = self.t(key);

        for argument in args {
            let Some(start) = rest.find('{') else {
                break;
            };
            let Some(end) = rest[start..].find('}') else {
                break;
            };
            result.push_str(&rest[..start]);
            result.push_str(argument);
            rest = &rest[start + end + 1..];
        }

        result.push_str(rest);
        result
    }

    fn clear_daemon_state(&mut self) {
        self.online = false;
        self.enabled = false;
        self.actual_cps = 0.0;
        self.starting = false;
        self.delay_left_ms = 0;
        self.hold_active = false;
        self.stop_reason = None;
        self.daemon_error = None;
        self.daemon_settings = None;
        self.daemon_protocol = 0;
        self.daemon_hotkey_active.clear();
        self.daemon_hold_hotkey_active.clear();
        self.daemon_hotkey_paused = false;
        self.daemon_hotkey_error = None;
        self.last_hotkey_sync = None;
        self.last_hold_hotkey_sync = None;
        self.last_status_at = None;
        self.initialized = false;
        self.last_apply_sent = None;
    }

    /// Restart the IPC thread if it stopped delivering status updates while
    /// the daemon is online. A detached old thread is asked to stop through
    /// its atomic flag; a fresh thread reconnects and brings the window back
    /// to live data without restarting the whole GUI.
    fn restart_worker(&mut self, ctx: &egui::Context) {
        self.worker_stop.store(true, Ordering::Relaxed);
        // Do not join: a stuck old thread must not freeze the interface. The
        // stop flag makes it exit at the next timeout.
        self.worker.take();
        self.last_worker_restart = Some(Instant::now());
        self.last_status_at = None;

        match Worker::spawn(
            ctx.clone(),
            self.hotkey.clone(),
            self.hold_hotkey.clone(),
            self.language,
        ) {
            Ok(worker) => {
                self.worker_stop = worker.stop_flag();
                self.commands = worker.commands;
                self.events = worker.events;
                self.worker = worker.handle;
                self.online = false;
            }
            Err(error) => {
                self.notice = Some(format!("{}: {error}", self.t(Key::WorkerReconnect)));
            }
        }
    }

    fn check_worker_health(&mut self, ctx: &egui::Context) {
        if !self.online {
            return;
        }
        let Some(last) = self.last_status_at else {
            return;
        };
        if last.elapsed() < STATUS_STALE_AFTER {
            return;
        }
        if self
            .last_worker_restart
            .is_some_and(|restart| restart.elapsed() < WORKER_RESTART_COOLDOWN)
        {
            return;
        }

        self.notice = Some(self.t(Key::WorkerReconnect).to_string());
        self.restart_worker(ctx);
    }

    fn cancel_start_guard(&mut self) {
        if self.start_guard.take().is_some() {
            self.start_token.fetch_add(1, Ordering::Relaxed);
            self.notice = None;
        }
    }

    /// In cursor mode the first click lands wherever the pointer is, which may
    /// be the power button itself. Give the user a moment to move the pointer
    /// outside the window before the session starts.
    fn request_start(&mut self) {
        if self.settings.mode != ClickMode::Cursor {
            self.request_toggle(true);
            return;
        }

        let settings = match self.settings.to_settings() {
            Ok(settings) => settings,
            Err(error) => {
                self.notice = Some(error);
                return;
            }
        };

        let generation = self.start_token.fetch_add(1, Ordering::Relaxed) + 1;
        let token = self.start_token.clone();
        let commands = self.commands.clone();
        let spawned = thread::Builder::new()
            .name("v-autoclicker-start-guard".to_string())
            .spawn(move || {
                thread::sleep(START_GUARD);
                if token.load(Ordering::Relaxed) == generation {
                    let _ = commands.send(Command::Start(settings));
                }
            });

        match spawned {
            Ok(_) => {
                self.start_guard = Some(generation);
                self.notice = Some(self.t(Key::StartGuardNotice).to_string());
            }
            Err(error) => {
                self.start_token.fetch_add(1, Ordering::Relaxed);
                self.notice = Some(format!("{}: {error}", self.t(Key::StartGuardNotice)));
            }
        }
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Status(status) => {
                    self.last_status_at = Some(Instant::now());
                    self.apply_status(*status);
                }
                Event::Offline(reason) => {
                    self.clear_daemon_state();
                    self.offline_reason = Some(reason);
                    self.pending_toggle = None;
                }
                Event::Error(error) => {
                    self.notice = Some(error);
                    self.pending_toggle = None;
                }
                #[cfg(not(windows))]
                Event::Stopped => {
                    self.clear_daemon_state();
                    self.offline_reason = None;
                    self.pending_toggle = None;
                    self.notice = Some(self.t(Key::DaemonStoppedNotice).to_string());
                }
            }
        }

        if self
            .pending_toggle
            .is_some_and(|(_, sent_at)| sent_at.elapsed() > TOGGLE_TIMEOUT)
        {
            self.pending_toggle = None;
            self.notice = Some(self.t(Key::ToggleTimeout).to_string());
        }

        if self.pending_toggle.is_some() || self.starting {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    /// Drain update events and start the delayed startup check when enabled.
    fn poll_updates(&mut self, ctx: &egui::Context) {
        if let Some(at) = self.auto_check_at {
            let now = Instant::now();
            if now >= at {
                self.auto_check_at = None;
                if self.check_updates {
                    self.start_update_check(ctx, true);
                }
            } else {
                ctx.request_repaint_after(at - now);
            }
        }

        while let Ok(event) = self.update_events.try_recv() {
            match event {
                UpdateEvent::Checked(result) => {
                    let automatic = std::mem::take(&mut self.auto_check_pending);
                    self.update = match result {
                        Ok(Some(release)) => UpdateState::Available(release),
                        Ok(None) => UpdateState::UpToDate,
                        Err(_) if automatic => {
                            // A startup check must never interrupt the user
                            // with a network error.
                            UpdateState::Idle
                        }
                        Err(error) => {
                            UpdateState::Failed(self.fill(Key::UpdateCheckFailed, &[error]))
                        }
                    };
                }
                UpdateEvent::Progress(fraction) => {
                    if let UpdateState::Downloading { progress } = &mut self.update {
                        *progress = fraction;
                    }
                }
                UpdateEvent::Downloaded(result) => {
                    self.update = match result {
                        Ok((release, bytes)) => UpdateState::Ready { release, bytes },
                        Err((release, error)) => {
                            self.notice = Some(self.fill(Key::UpdateDownloadFailed, &[error]));
                            UpdateState::Available(release)
                        }
                    };
                }
            }
        }

        if matches!(
            self.update,
            UpdateState::Checking | UpdateState::Downloading { .. } | UpdateState::Installing
        ) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn start_update_check(&mut self, ctx: &egui::Context, automatic: bool) {
        if matches!(
            self.update,
            UpdateState::Checking | UpdateState::Downloading { .. } | UpdateState::Installing
        ) {
            return;
        }

        self.auto_check_pending = automatic;
        self.update = UpdateState::Checking;
        let sender = self.update_sender.clone();
        let ctx = ctx.clone();
        let spawned = thread::Builder::new()
            .name("v-autoclicker-update-check".to_string())
            .spawn(move || {
                let result = update::check().map_err(|error| format!("{error:#}"));
                let _ = sender.send(UpdateEvent::Checked(result));
                ctx.request_repaint();
            });

        if let Err(error) = spawned {
            let detail = format!("{error}");
            self.notice = Some(self.fill(Key::UpdateCheckFailed, &[detail]));
            self.update = UpdateState::Idle;
        }
    }

    fn start_update_download(&mut self, ctx: &egui::Context) {
        let UpdateState::Available(release) = &self.update else {
            return;
        };
        let release = release.clone();
        self.update = UpdateState::Downloading { progress: 0.0 };
        let fallback = release.clone();

        let sender = self.update_sender.clone();
        let progress_sender = sender.clone();
        let progress_ctx = ctx.clone();
        let thread_ctx = ctx.clone();
        let spawned = thread::Builder::new()
            .name("v-autoclicker-update-download".to_string())
            .spawn(move || {
                let mut last = -1.0f32;
                let result = update::download(&release, &mut |fraction| {
                    if fraction - last >= 0.01 || fraction >= 1.0 {
                        last = fraction;
                        let _ = progress_sender.send(UpdateEvent::Progress(fraction));
                        progress_ctx.request_repaint();
                    }
                });
                let event = match result {
                    Ok(bytes) => UpdateEvent::Downloaded(Ok((release, bytes))),
                    Err(error) => UpdateEvent::Downloaded(Err((release, format!("{error:#}")))),
                };
                let _ = sender.send(event);
                thread_ctx.request_repaint();
            });

        if let Err(error) = spawned {
            let detail = format!("{error}");
            self.notice = Some(self.fill(Key::UpdateDownloadFailed, &[detail]));
            self.update = UpdateState::Available(fallback);
        }
    }

    /// Stop the daemon, replace the executable and relaunch the app.
    fn install_update(&mut self, ctx: &egui::Context) {
        if !matches!(self.update, UpdateState::Ready { .. }) {
            return;
        }
        let UpdateState::Ready { release, bytes } =
            std::mem::replace(&mut self.update, UpdateState::Installing)
        else {
            unreachable!("state was checked above");
        };

        self.save_config();
        self.notice = Some(self.t(Key::UpdateInstalling).to_string());
        self.auto_check_at = None;

        // Let the worker stop clicking, resume the hotkey and shut the daemon
        // down before the binary it runs is replaced. On Linux the daemon
        // outlives the window, so it needs an explicit QUIT first.
        #[cfg(not(windows))]
        let _ = self.commands.send(Command::QuitDaemon);
        let _ = self.commands.send(Command::Shutdown);
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
        self.online = false;
        self.initialized = false;

        // The new window spawns a fresh daemon. Wait for the old one to release
        // its socket or pipe, otherwise the replacement could talk to the old
        // process instead.
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if crate::ipc::send("STATUS").is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }

        match update::install(&bytes) {
            Ok(executable) => match update::relaunch(&executable) {
                Ok(()) => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Err(error) => {
                    self.restart_worker(ctx);
                    self.notice =
                        Some(self.fill(Key::UpdateRestartFailed, &[format!("{error:#}")]));
                    self.update = UpdateState::Ready { release, bytes };
                }
            },
            Err(error) => {
                self.restart_worker(ctx);
                self.notice = Some(self.fill(Key::UpdateInstallFailed, &[format!("{error:#}")]));
                self.update = UpdateState::Ready { release, bytes };
            }
        }
    }

    fn draw_update_confirmation(&mut self, ctx: &egui::Context) {
        if !self.confirm_update {
            return;
        }

        match self.confirmation_modal(
            ctx,
            Confirmation {
                id: "install_update",
                title: Key::UpdateConfirmTitle,
                question: Key::UpdateConfirmQuestion,
                text: Key::UpdateConfirmText,
                confirm: Key::UpdateConfirm,
                solid: false,
            },
        ) {
            Some(true) => {
                self.confirm_update = false;
                self.install_update(ctx);
            }
            Some(false) => self.confirm_update = false,
            None => {}
        }
    }

    fn draw_update_section(&mut self, ui: &mut Ui) {
        theme::rule(ui);
        ui.add_space(2.0);

        let version = format!("v{}", update::CURRENT_VERSION);
        let mut check_clicked = false;
        let mut install_clicked = false;
        let mut download_clicked = false;
        let mut open_notes: Option<String> = None;
        let mut status: Option<(String, Color32)> = None;
        let mut failure: Option<String> = None;
        let mut show_check = false;
        let mut show_install = false;
        let mut notes_url: Option<String> = None;
        let ready = matches!(self.update, UpdateState::Ready { .. });

        if !update_supported() {
            status = Some((
                self.t(Key::UpdatePlatformUnavailable).to_string(),
                theme::muted(),
            ));
        } else {
            match &self.update {
                UpdateState::Idle => show_check = true,
                UpdateState::Checking => {
                    status = Some((self.t(Key::UpdateChecking).to_string(), theme::muted()));
                }
                UpdateState::UpToDate => {
                    status = Some((self.t(Key::UpdateCurrent).to_string(), theme::muted()));
                    show_check = true;
                }
                UpdateState::Available(release) => {
                    status = Some((
                        self.fill(Key::UpdateAvailable, &[release.version.to_string()]),
                        theme::accent_ink(),
                    ));
                    show_install = true;
                    notes_url = Some(release.page_url.clone());
                }
                UpdateState::Ready { release, .. } => {
                    status = Some((self.t(Key::UpdateReady).to_string(), theme::accent_ink()));
                    show_install = true;
                    notes_url = Some(release.page_url.clone());
                }
                UpdateState::Downloading { progress } => {
                    let percent = (progress * 100.0).round().clamp(0.0, 100.0) as u32;
                    status = Some((
                        self.fill(Key::UpdateDownloading, &[percent.to_string()]),
                        theme::muted(),
                    ));
                }
                UpdateState::Installing => {
                    status = Some((self.t(Key::UpdateInstalling).to_string(), theme::warning()));
                }
                UpdateState::Failed(message) => {
                    failure = Some(message.clone());
                    show_check = true;
                }
            }
        }

        // One dense header row: title and version on the left, the state and
        // the re-check action on the right. Only an available update adds the
        // action row below the startup checkbox.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let _ = row_label(ui, self.t(Key::RowUpdates), self.t(Key::HintUpdates), 20.0);
            ui.add_space(6.0);
            ui.label(
                RichText::new(&version)
                    .font(theme::mono(9.5))
                    .color(theme::dim()),
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if show_check
                    && widgets::text_button(
                        ui,
                        self.t(Key::UpdateCheck),
                        theme::muted(),
                        theme::accent(),
                    )
                    .clicked()
                {
                    check_clicked = true;
                }

                if let Some((text, color)) = &status {
                    ui.add(
                        egui::Label::new(RichText::new(text).font(theme::sans(10.0)).color(*color))
                            .truncate(),
                    )
                    .on_hover_text(text);
                }
                // Keep the status clear of the version even when the row is
                // tight; otherwise the two labels touch at 460 px.
                ui.add_space(10.0);
            });
        });

        if let Some(message) = failure {
            ui.add_space(2.0);
            ui.add(
                egui::Label::new(
                    RichText::new(message)
                        .font(theme::sans(10.0))
                        .color(theme::warning()),
                )
                .wrap(),
            );
        }

        ui.add_space(2.0);
        let mut enabled = self.check_updates;
        let mut startup_changed = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let response = widgets::checkbox(ui, &mut enabled, self.t(Key::UpdateCheckOnStart));
            let _ = response.on_hover_text(self.t(Key::UpdateCheckOnStartHint));
            startup_changed = enabled != self.check_updates;
        });

        // Only an available update adds a third row. Keeping these actions
        // here avoids squeezing them against a long startup-checkbox label in
        // languages such as Russian.
        if show_install {
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.add_enabled_ui(!cfg!(debug_assertions), |ui| {
                        if widgets::text_button(
                            ui,
                            self.t(Key::UpdateInstall),
                            theme::accent(),
                            theme::accent(),
                        )
                        .clicked()
                        {
                            if ready {
                                install_clicked = true;
                            } else {
                                download_clicked = true;
                            }
                        }
                    });

                    if let Some(url) = &notes_url {
                        if widgets::text_button(
                            ui,
                            self.t(Key::UpdateNotes),
                            theme::muted(),
                            theme::text(),
                        )
                        .clicked()
                        {
                            open_notes = Some(url.clone());
                        }
                    }
                });
            });
        }

        if startup_changed {
            self.check_updates = enabled;
            if enabled {
                self.auto_check_at = Some(Instant::now());
            } else {
                self.auto_check_at = None;
            }
            self.save_config();
        }

        if cfg!(debug_assertions) && show_install {
            ui.add_space(2.0);
            ui.label(
                RichText::new(self.t(Key::UpdateDevBuild))
                    .font(theme::sans(10.0))
                    .color(theme::warning()),
            );
        }

        if check_clicked {
            self.start_update_check(ui.ctx(), false);
        }
        if download_clicked {
            self.start_update_download(ui.ctx());
        }
        if install_clicked {
            self.confirm_update = true;
        }
        if let Some(url) = open_notes {
            if let Err(error) = crate::platform::open_url(&url) {
                self.notice = Some(format!("{error:#}"));
            }
        }
    }

    fn apply_status(&mut self, status: Status) {
        let first_status = !self.initialized;
        let draft_was_dirty = self.draft_changed_at.is_some();

        self.online = true;
        self.initialized = true;
        self.offline_reason = None;
        self.enabled = status.enabled;
        self.actual_cps = status.actual_cps;
        self.session_clicks = status.session_clicks;
        self.starting = status.starting;
        self.delay_left_ms = status.delay_left_ms;
        self.stop_reason = status.stop_reason;
        self.hold_active = status.hold_active;
        if status.enabled || status.starting {
            self.start_guard = None;
        }
        self.daemon_error = status.last_error;
        self.daemon_hotkey_active = status.hotkey_active;
        self.daemon_hold_hotkey_active = status.hold_hotkey_active;
        self.daemon_hotkey_paused = status.hotkey_paused;
        self.daemon_hotkey_error = status.hotkey_error;
        self.daemon_settings = Some(status.settings);
        self.daemon_protocol = status.protocol;

        if status.protocol < crate::state::PROTOCOL && !self.legacy_restart_attempted {
            // Daemons before protocol 2 reject the new click limit and start
            // delay. Restart them once, preserving a running session.
            self.legacy_restart_attempted = true;
            self.legacy_restart_pending = true;
            self.initialized = false;
            self.daemon_settings = None;
            self.daemon_hotkey_error = None;
            let resume = status.enabled.then_some(status.settings);
            self.notice = Some(self.t(Key::LegacyRestart).to_string());
            let _ = self.commands.send(Command::RestartDaemon {
                hotkey: self.hotkey.clone(),
                hold_hotkey: self.hold_hotkey.clone(),
                resume,
            });
            return;
        }

        if self.legacy_restart_pending && status.protocol >= crate::state::PROTOCOL {
            self.legacy_restart_pending = false;
            self.notice = None;
        }

        if status.hotkey == self.hotkey {
            self.last_hotkey_sync = None;
        } else if self
            .last_hotkey_sync
            .is_none_or(|sent_at| sent_at.elapsed() >= SYNC_GRACE)
        {
            let _ = self.commands.send(Command::SetHotkey(self.hotkey.clone()));
            self.last_hotkey_sync = Some(Instant::now());
        }

        #[cfg(windows)]
        {
            if status.hold_hotkey == self.hold_hotkey {
                self.last_hold_hotkey_sync = None;
            } else if self
                .last_hold_hotkey_sync
                .is_none_or(|sent_at| sent_at.elapsed() >= SYNC_GRACE)
            {
                let _ = self
                    .commands
                    .send(Command::SetHoldHotkey(self.hold_hotkey.clone()));
                self.last_hold_hotkey_sync = Some(Instant::now());
            }
        }

        if first_status {
            if status.enabled {
                if !draft_was_dirty {
                    self.settings = status.settings.into();
                }
            } else if let Ok(current) = self.settings.to_settings() {
                if !current.equivalent(&status.settings) {
                    let _ = self.commands.send(Command::Apply(current));
                    self.last_apply_sent = Some(Instant::now());
                }
            }
        } else if self.draft_changed_at.is_none()
            && self
                .last_apply_sent
                .is_none_or(|sent_at| sent_at.elapsed() >= SYNC_GRACE)
        {
            self.settings = status.settings.into();
        }

        if let Ok(current) = self.settings.to_settings() {
            if current.equivalent(&status.settings) {
                self.last_apply_sent = None;
                if self.draft_changed_at.take().is_some() {
                    self.save_config();
                }
            }
        }

        if self
            .pending_toggle
            .is_some_and(|(expected, _)| expected == status.enabled)
        {
            self.pending_toggle = None;
        }
    }

    fn maybe_apply_settings(&mut self) {
        let Some(changed_at) = self.draft_changed_at else {
            return;
        };
        if changed_at.elapsed() < APPLY_DEBOUNCE {
            return;
        }
        self.flush_settings();
    }

    fn flush_settings(&mut self) {
        let settings = match self.settings.to_settings() {
            Ok(settings) => settings,
            Err(error) => {
                self.draft_changed_at = None;
                self.notice = Some(error);
                return;
            }
        };

        self.draft_changed_at = None;

        if self.online {
            let already_current = self
                .daemon_settings
                .is_some_and(|current| current.equivalent(&settings));
            if !already_current {
                let _ = self.commands.send(Command::Apply(settings));
                self.last_apply_sent = Some(Instant::now());
            }
        }

        self.save_config();
    }

    fn config_snapshot(&self) -> Option<config::Config> {
        self.settings
            .to_settings()
            .ok()
            .map(|settings| config::Config {
                settings,
                hotkey: self.hotkey.clone(),
                hold_hotkey: self.hold_hotkey.clone(),
                theme: self.theme,
                language: self.language,
                simple_mode: self.simple_mode,
                always_on_top: self.always_on_top,
                minimize_to_tray: self.minimize_to_tray,
                check_updates: self.check_updates,
                profiles: self.profiles.clone(),
                active_profile: self.active_profile.clone(),
            })
    }

    fn save_config(&mut self) {
        self.poll_save_errors();
        let Some(config) = self.config_snapshot() else {
            return;
        };
        self.saver.save(config);
    }

    fn poll_save_errors(&mut self) {
        if let Some(error) = self.saver.take_error() {
            self.notice = Some(self.fill(Key::SaveFailed, &[error]));
        }
    }

    fn mark_dirty(&mut self) {
        self.draft_changed_at = Some(Instant::now());
        self.notice = None;
    }

    fn set_theme(&mut self, ctx: &egui::Context, theme: Theme) {
        if theme == self.theme {
            return;
        }
        self.theme = theme;
        theme::set_theme(ctx, theme);
        self.save_config();
        self.notice = Some(self.fill(Key::ThemeNotice, &[self.theme_label(theme).to_string()]));
    }

    fn theme_label(&self, theme: Theme) -> &'static str {
        self.t(theme.label_key())
    }

    fn set_language(&mut self, language: Language) {
        self.language = language;
        self.lang = language.resolve();
        #[cfg(windows)]
        super::tray::set_language(self.lang);
        let _ = self.commands.send(Command::SetLanguage(language));
        self.save_config();
    }

    fn apply_auto_scale(&mut self, ctx: &egui::Context) {
        let Some(monitor) = ctx.input(|input| input.viewport().monitor_size) else {
            return;
        };
        let size = window_size(self.simple_mode);
        let scale = auto_scale_for(monitor, ctx.zoom_factor(), size);

        if (scale - ctx.zoom_factor()).abs() > 0.001 {
            ctx.set_zoom_factor(scale);
        }

        // The zoom factor changes how many winit logical pixels one egui point
        // occupies. Scale the outer window as well so the layout still gets
        // its designed points after zooming.
        if self.applied_scale != Some(scale) {
            self.applied_scale = Some(scale);
            let min = min_window_size(self.simple_mode);
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(Vec2::new(
                min[0] * scale,
                min[1] * scale,
            )));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                size[0] * scale,
                size[1] * scale,
            )));
        }
    }

    fn set_simple_mode(&mut self, ctx: &egui::Context, simple: bool) {
        if self.simple_mode == simple {
            return;
        }

        self.simple_mode = simple;
        self.save_config();

        let scale = ctx.zoom_factor();
        let size = window_size(simple);
        let min = min_window_size(simple);
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(Vec2::new(
            min[0] * scale,
            min[1] * scale,
        )));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
            size[0] * scale,
            size[1] * scale,
        )));
    }

    fn set_always_on_top(&mut self, ctx: &egui::Context, enabled: bool) {
        if self.always_on_top == enabled {
            return;
        }

        self.always_on_top = enabled;
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if enabled {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        }));
        self.save_config();
    }

    fn start_pipette(&mut self, target: PipetteTarget) {
        if self.pipette.is_some() {
            return;
        }

        let (sender, receiver) = mpsc::channel();
        let spawned = thread::Builder::new()
            .name("v-autoclicker-pipette".to_string())
            .spawn(move || {
                let result = crate::backend::capture_point().map_err(|error| format!("{error:#}"));
                let _ = sender.send(result);
            });

        match spawned {
            Ok(_) => {
                self.pipette = Some(Pipette { target, receiver });
                self.notice = Some(self.t(Key::PickWaiting).to_string());
            }
            Err(error) => {
                self.notice = Some(self.fill(Key::PickFailed, &[format!("{error:#}")]));
            }
        }
    }

    fn poll_pipette(&mut self, ctx: &egui::Context) {
        let (target, result) = match self.pipette.as_ref() {
            Some(pipette) => (pipette.target, pipette.receiver.try_recv()),
            None => return,
        };

        match result {
            Ok(Ok(Some(point))) => {
                self.pipette = None;
                match target {
                    PipetteTarget::Point => {
                        self.settings.mode = ClickMode::Point;
                        self.settings.target = Some(point);
                    }
                    PipetteTarget::Sequence(index) => {
                        self.settings.set_point(index, point);
                    }
                    PipetteTarget::NewSequence => {
                        if self.settings.add_point(point).is_none() {
                            self.notice = Some(self.t(Key::PointsFull).to_string());
                        }
                    }
                }
                self.mark_dirty();
                self.notice = Some(self.fill(
                    Key::CurrentPicked,
                    &[point.x.to_string(), point.y.to_string()],
                ));
            }
            Ok(Ok(None)) => {
                self.pipette = None;
                self.notice = Some(self.t(Key::PickCancelled).to_string());
            }
            Ok(Err(error)) => {
                self.pipette = None;
                self.notice = Some(self.fill(Key::PickFailed, &[error]));
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pipette = None;
                self.notice = Some(self.t(Key::PickCancelled).to_string());
            }
        }
    }

    fn add_sequence_point(&mut self) {
        if self.settings.point_count() >= MAX_POINTS {
            self.notice = Some(self.t(Key::PointsFull).to_string());
            return;
        }

        self.start_pipette(PipetteTarget::NewSequence);
    }

    fn restore_defaults(&mut self, ctx: &egui::Context) {
        let defaults = Settings::default();
        self.settings = defaults.into();
        self.pipette = None;
        self.recording = None;
        self.hotkey = crate::hotkey::DEFAULT.to_string();
        self.hold_hotkey.clear();
        self.theme = Theme::Paper;
        theme::set_theme(ctx, self.theme);
        self.language = Language::Auto;
        self.lang = self.language.resolve();
        self.daemon_hotkey_error = None;
        self.last_hotkey_sync = Some(Instant::now());
        self.last_hold_hotkey_sync = Some(Instant::now());
        let _ = self.commands.send(Command::SetLanguage(self.language));
        let _ = self.commands.send(Command::SetHotkey(self.hotkey.clone()));
        let _ = self.commands.send(Command::SetHoldHotkey(String::new()));
        #[cfg(windows)]
        super::tray::set_language(self.lang);
        self.mark_dirty();
        self.flush_settings();
        self.notice = Some(self.t(Key::DefaultsDone).to_string());
    }

    #[cfg(windows)]
    fn sync_tray(&mut self, ctx: &egui::Context) {
        super::tray::set_close_to_tray(self.minimize_to_tray);
        if self.minimize_to_tray {
            if self.tray.is_none() {
                match super::tray::Tray::start(ctx.clone(), self.lang) {
                    Ok(tray) => {
                        self.tray = Some(tray);
                        self.notice = Some(self.t(Key::TrayEnabled).to_string());
                    }
                    Err(error) => {
                        self.minimize_to_tray = false;
                        self.notice = Some(self.fill(Key::TrayFailed, &[format!("{error:#}")]));
                        self.save_config();
                    }
                }
            }
        } else {
            self.tray = None;
        }
    }

    #[cfg(windows)]
    fn start_recording(&mut self, target: RecordTarget) {
        self.recording = Some(target);
        self.notice = None;
        let _ = self.commands.send(Command::PauseHotkey);
    }

    fn stop_recording(&mut self, resume: bool) {
        if self.recording.take().is_none() {
            return;
        }
        if resume {
            #[cfg(windows)]
            let _ = self.commands.send(Command::ResumeHotkey);
        }
    }

    fn capture_hotkey(&mut self, ctx: &egui::Context) {
        let Some(target) = self.recording else {
            return;
        };

        if !ctx.input(|input| input.focused) {
            self.stop_recording(true);
            return;
        }

        let mut pressed = None;
        ctx.input(|input| {
            for event in &input.events {
                if let egui::Event::Key {
                    key,
                    physical_key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } = event
                {
                    pressed = Some((
                        physical_key.unwrap_or(*key),
                        modifiers.ctrl,
                        modifiers.alt,
                        modifiers.shift,
                    ));
                    break;
                }
            }
        });

        let Some((key, ctrl, alt, shift)) = pressed else {
            return;
        };

        let mut combo = String::new();
        for (enabled, name) in [(ctrl, "ctrl"), (alt, "alt"), (shift, "shift")] {
            if enabled {
                if !combo.is_empty() {
                    combo.push('+');
                }
                combo.push_str(name);
            }
        }
        if !combo.is_empty() {
            combo.push('+');
        }
        combo.push_str(&key.name().to_ascii_lowercase());

        self.apply_binding(target, &combo);
    }

    fn apply_binding(&mut self, target: RecordTarget, combo: &str) {
        let hotkey = match crate::hotkey::Hotkey::parse(combo) {
            Ok(hotkey) => hotkey,
            Err(error) => {
                self.stop_recording(true);
                self.notice = Some(self.fill(Key::BindingFailed, &[format!("{error:#}")]));
                return;
            }
        };
        let combo = hotkey.to_string();

        match target {
            RecordTarget::Toggle => {
                if !self.hold_hotkey.is_empty() && combo == self.hold_hotkey {
                    self.stop_recording(true);
                    self.notice = Some(self.t(Key::KeyAlreadyHold).to_string());
                    return;
                }
                self.stop_recording(true);
                if combo == self.hotkey && self.daemon_hotkey_error.is_none() {
                    return;
                }
                self.hotkey = combo;
                self.daemon_hotkey_error = None;
                self.last_hotkey_sync = Some(Instant::now());
                let _ = self.commands.send(Command::SetHotkey(self.hotkey.clone()));
            }
            RecordTarget::Hold => {
                if combo == self.hotkey {
                    self.stop_recording(true);
                    self.notice = Some(self.t(Key::KeyAlreadyToggle).to_string());
                    return;
                }
                self.stop_recording(true);
                if combo == self.hold_hotkey && self.daemon_hotkey_error.is_none() {
                    return;
                }
                self.hold_hotkey = combo;
                self.daemon_hotkey_error = None;
                self.last_hold_hotkey_sync = Some(Instant::now());
                let _ = self
                    .commands
                    .send(Command::SetHoldHotkey(self.hold_hotkey.clone()));
            }
        }

        let bound = match target {
            RecordTarget::Toggle => self.hotkey.as_str(),
            RecordTarget::Hold => self.hold_hotkey.as_str(),
        };
        let label = crate::hotkey::Hotkey::parse(bound)
            .map(|hotkey| hotkey.label())
            .unwrap_or_else(|_| bound.to_ascii_uppercase());
        self.notice = Some(
            self.fill(
                Key::BoundNotice,
                &[
                    match target {
                        RecordTarget::Toggle => self.t(Key::RecordTargetToggle),
                        RecordTarget::Hold => self.t(Key::RecordTargetHold),
                    }
                    .to_string(),
                    label,
                ],
            ),
        );
        self.save_config();
    }

    #[cfg(windows)]
    fn clear_hold_hotkey(&mut self) {
        self.hold_hotkey.clear();
        self.daemon_hotkey_error = None;
        self.last_hold_hotkey_sync = Some(Instant::now());
        let _ = self.commands.send(Command::SetHoldHotkey(String::new()));
        self.notice = Some(self.t(Key::HoldDisabled).to_string());
        self.save_config();
    }

    fn request_toggle(&mut self, start: bool) {
        if start {
            match self.settings.to_settings() {
                Ok(settings) => {
                    self.notice = None;
                    self.pending_toggle = Some((true, Instant::now()));
                    let _ = self.commands.send(Command::Start(settings));
                }
                Err(error) => self.notice = Some(error),
            }
        } else {
            self.cancel_start_guard();
            self.notice = None;
            self.pending_toggle = Some((false, Instant::now()));
            let _ = self.commands.send(Command::Stop);
        }
    }

    #[cfg(not(windows))]
    fn request_quit(&mut self) {
        self.pending_toggle = None;
        self.notice = Some(self.t(Key::Quitting).to_string());
        let _ = self.commands.send(Command::QuitDaemon);
    }

    fn draw_topbar(&mut self, ui: &mut Ui) {
        let mut toggle_mode = false;
        let mut toggle_pin = false;

        ui.horizontal(|ui| {
            let (mark, _) = ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
            ui.painter()
                .rect_filled(mark, CornerRadius::same(2), theme::accent());

            ui.label(
                RichText::new("V-AutoClicker")
                    .font(theme::sans_semibold(16.0))
                    .color(theme::text()),
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let toggle = if self.simple_mode {
                    Key::ModeAdvanced
                } else {
                    Key::ModeSimple
                };
                if widgets::text_button(ui, self.t(toggle), theme::muted(), theme::accent())
                    .clicked()
                {
                    toggle_mode = true;
                }

                if always_on_top_supported() {
                    ui.add_space(4.0);
                    if widgets::pin_button(ui, self.always_on_top)
                        .on_hover_text(self.t(Key::RowAlwaysOnTop))
                        .clicked()
                    {
                        toggle_pin = true;
                    }
                }
            });
        });

        if toggle_mode {
            let simple = !self.simple_mode;
            self.set_simple_mode(ui.ctx(), simple);
        }
        if toggle_pin {
            let enabled = !self.always_on_top;
            self.set_always_on_top(ui.ctx(), enabled);
        }
    }

    fn draw_simple_body(&mut self, ui: &mut Ui) {
        theme::card_show(ui, |ui| {
            ui.set_width(ui.available_width());
            self.draw_hero(ui);
        });

        // The compact window keeps only notices that need attention; the idle
        // "ready" strip would push the controls card out of 380x260.
        if self.simple_notice_visible() {
            ui.add_space(6.0);
            self.draw_notice(ui);
        }
        ui.add_space(8.0);

        theme::card_show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut draft = self.settings;
            let mut changed = false;

            changed |=
                compact_choice_row(ui, self.t(Key::RowButton), self.t(Key::HintButton), |ui| {
                    widgets::segmented(
                        ui,
                        "simple_mouse_button",
                        &mut draft.button,
                        &[
                            (Button::Left, self.t(Key::ButtonLeft)),
                            (Button::Right, self.t(Key::ButtonRight)),
                            (Button::Middle, self.t(Key::ButtonMiddle)),
                        ],
                    )
                });

            theme::rule(ui);
            changed |= if draft.mode == ClickMode::Random {
                compact_slider_row(
                    ui,
                    self.t(Key::RowRandomInterval),
                    self.t(Key::HintRandomInterval),
                    &mut draft.random_interval_ms,
                    SliderSpec {
                        range: MIN_RANDOM_INTERVAL_MS as f64..=MAX_RANDOM_INTERVAL_MS as f64,
                        logarithmic: true,
                        step: 1.0,
                        decimals: 0,
                        suffix: self.t(Key::UnitMs),
                    },
                )
            } else {
                compact_slider_row(
                    ui,
                    self.t(Key::RowSpeed),
                    self.t(Key::HintSpeed),
                    &mut draft.cps,
                    SliderSpec {
                        range: 0.2..=1000.0,
                        logarithmic: true,
                        step: 0.1,
                        decimals: 1,
                        suffix: "CPS",
                    },
                )
            };

            if changed {
                self.settings = draft;
                self.mark_dirty();
            }
        });
    }

    fn stopped_hint(&self) -> &'static str {
        #[cfg(windows)]
        {
            self.t(Key::HeroPressStart)
        }
        #[cfg(not(windows))]
        {
            if self.online {
                self.t(Key::HeroPressStart)
            } else {
                self.t(Key::HeroDaemonAuto)
            }
        }
    }

    fn draw_hero(&mut self, ui: &mut Ui) {
        let active = self.online && self.enabled;
        let pending = self.pending_toggle.map(|(start, _)| start);

        let (title, description) = if let Some(start) = pending {
            (
                if start {
                    self.t(Key::HeroStarting)
                } else {
                    self.t(Key::HeroStopping)
                }
                .to_string(),
                self.t(Key::HeroWaitDaemon).to_string(),
            )
        } else if self.start_guard.is_some() {
            (
                self.t(Key::HeroMovePointer).to_string(),
                self.t(Key::StartGuardText).to_string(),
            )
        } else if self.starting {
            (
                self.fill(
                    Key::HeroDelayTitle,
                    &[format!("{:.1}", self.delay_left_ms as f64 / 1000.0)],
                ),
                self.t(Key::HeroDelayText).to_string(),
            )
        } else if self.hold_active {
            (
                self.t(Key::HeroHoldTitle).to_string(),
                self.t(Key::HeroHoldText).to_string(),
            )
        } else if active {
            let description = if self.settings.limit > 0 {
                self.fill(
                    Key::HeroSessionProgress,
                    &[
                        group_digits(self.session_clicks),
                        group_digits(self.settings.limit),
                    ],
                )
            } else if self.settings.mode == ClickMode::Sequence {
                self.fill(
                    Key::HeroSequenceClicks,
                    &[self.settings.point_count().to_string()],
                )
            } else if self.settings.mode == ClickMode::Random {
                self.t(Key::HeroRandomClicks).to_string()
            } else if self.settings.target.is_some() {
                self.t(Key::HeroTargetClicks).to_string()
            } else {
                self.t(Key::HeroCursorClicks).to_string()
            };
            (self.t(Key::HeroClicking).to_string(), description)
        } else if self.stop_reason == Some(StopReason::Limit) {
            (
                self.t(Key::HeroLimitDone).to_string(),
                self.fill(Key::HeroLimitText, &[group_digits(self.session_clicks)]),
            )
        } else {
            (
                self.t(Key::HeroStopped).to_string(),
                self.stopped_hint().to_string(),
            )
        };

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let busy = pending.is_some() || self.start_guard.is_some();
            let response = widgets::power_button(ui, active, busy, 60.0);
            if response.clicked() && pending.is_none() {
                if self.start_guard.is_some() {
                    self.cancel_start_guard();
                } else if active {
                    self.request_toggle(false);
                } else {
                    self.request_start();
                }
            }

            ui.add_space(12.0);
            let width = ui.available_width();
            ui.allocate_ui_with_layout(
                Vec2::new(width, 60.0),
                Layout::top_down(Align::LEFT),
                |ui| {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(title)
                            .font(theme::sans_semibold(19.0))
                            .color(theme::text()),
                    );

                    ui.add_space(1.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(description)
                                .font(theme::sans(10.5))
                                .color(theme::muted()),
                        )
                        .wrap(),
                    );
                },
            );
        });
    }

    fn draw_metrics(&self, ui: &mut Ui) {
        ui.add_space(2.0);
        theme::rule(ui);
        ui.add_space(4.0);
        ui.columns(2, |columns| {
            metric(
                &mut columns[0],
                self.t(Key::MetricCps),
                &format!("{:.1}", self.actual_cps),
                "CPS",
            );
            let session = group_digits(self.session_clicks);
            let limit = if self.settings.limit > 0 {
                format!("/ {}", group_digits(self.settings.limit))
            } else {
                self.t(Key::UnitPieces).to_string()
            };
            metric(
                &mut columns[1],
                self.t(Key::MetricSession),
                &session,
                &limit,
            );
        });
        ui.add_space(1.0);
    }

    fn notice_data(&self) -> (Color32, &'static str, Cow<'_, str>) {
        if let Some(error) = &self.daemon_error {
            return (
                theme::danger(),
                self.t(Key::NoticeError),
                Cow::Borrowed(error),
            );
        }
        if let Some(notice) = &self.notice {
            return (
                theme::warning(),
                self.t(Key::NoticeSystem),
                Cow::Borrowed(notice),
            );
        }
        if let Some(error) = &self.daemon_hotkey_error {
            return (
                theme::danger(),
                self.t(Key::NoticeHotkey),
                Cow::Borrowed(error),
            );
        }
        if self.starting {
            return (
                theme::warning(),
                self.t(Key::NoticeStart),
                Cow::Owned(self.fill(
                    Key::NoticeStartsIn,
                    &[format!("{:.1}", self.delay_left_ms as f64 / 1000.0)],
                )),
            );
        }
        if self.stop_reason == Some(StopReason::Limit) && !self.enabled {
            return (
                theme::warning(),
                self.t(Key::NoticeLimit),
                Cow::Owned(self.fill(Key::NoticeLimitDone, &[group_digits(self.session_clicks)])),
            );
        }
        if !self.online {
            let detail = self
                .offline_reason
                .as_deref()
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Borrowed(self.t(Key::NoticeDaemonDown)));
            return (theme::dim(), self.t(Key::NoticeDaemon), detail);
        }
        (
            theme::dim(),
            self.t(Key::NoticeReady),
            Cow::Borrowed(self.t(Key::NoticeAutoApply)),
        )
    }

    fn simple_notice_visible(&self) -> bool {
        let (_, title, _) = self.notice_data();
        title != self.t(Key::NoticeReady)
    }

    fn draw_notice(&self, ui: &mut Ui) {
        let (tone, title, text) = self.notice_data();
        theme::notice_frame(tone).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), 20.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    widgets::notice_line(ui, tone, title, &text);
                },
            );
        });
    }

    fn draw_tabs(&mut self, ui: &mut Ui) {
        let mut tab = self.tab;
        let changed = widgets::tabs(
            ui,
            "main_tabs",
            &mut tab,
            &[
                (Tab::Clicks, self.t(Key::TabClicks)),
                (Tab::Mode, self.t(Key::TabMode)),
                (Tab::Hotkeys, self.t(Key::TabHotkeys)),
                (Tab::System, self.t(Key::TabSystem)),
            ],
        );
        if changed {
            self.tab = tab;
            self.notice = None;
        }
    }

    fn draw_clicks_tab(&mut self, ui: &mut Ui) {
        // Six parameter rows plus the hint should fit without scrolling at the
        // default window sizes. The card layout adds padding, so use the
        // compact rhythm below 600 px and let only the absolute minimum
        // 420x520 window scroll.
        ui.spacing_mut().item_spacing.y = if ui.ctx().content_rect().height() < 600.0 {
            1.0
        } else {
            3.0
        };
        let mut draft = self.settings;
        let mut changed = false;

        if draft.mode == ClickMode::Random {
            let random_hint = format!(
                "{} {}",
                self.t(Key::HintRandomInterval),
                self.t(Key::HintRandomClicks)
            );
            changed |= compact_slider_row(
                ui,
                self.t(Key::RowRandomInterval),
                &random_hint,
                &mut draft.random_interval_ms,
                SliderSpec {
                    range: MIN_RANDOM_INTERVAL_MS as f64..=MAX_RANDOM_INTERVAL_MS as f64,
                    logarithmic: true,
                    step: 1.0,
                    decimals: 0,
                    suffix: self.t(Key::UnitMs),
                },
            );
        } else {
            let speed_hint = if draft.mode == ClickMode::Sequence {
                format!(
                    "{} {}",
                    self.t(Key::HintSpeed),
                    self.t(Key::SequenceSpeedHint)
                )
            } else {
                self.t(Key::HintSpeed).to_string()
            };
            changed |= compact_slider_row(
                ui,
                self.t(Key::RowSpeed),
                &speed_hint,
                &mut draft.cps,
                SliderSpec {
                    range: 0.2..=1000.0,
                    logarithmic: true,
                    step: 0.1,
                    decimals: 1,
                    suffix: "CPS",
                },
            );
        }

        theme::rule(ui);

        changed |= compact_choice_row(ui, self.t(Key::RowButton), self.t(Key::HintButton), |ui| {
            widgets::segmented(
                ui,
                "mouse_button",
                &mut draft.button,
                &[
                    (Button::Left, self.t(Key::ButtonLeft)),
                    (Button::Right, self.t(Key::ButtonRight)),
                    (Button::Middle, self.t(Key::ButtonMiddle)),
                ],
            )
        });

        theme::rule(ui);

        let ms = self.t(Key::UnitMs);
        if draft.mode != ClickMode::Random {
            changed |= compact_slider_row(
                ui,
                self.t(Key::RowHold),
                self.t(Key::HintHold),
                &mut draft.hold_ms,
                SliderSpec {
                    range: 0.1..=5000.0,
                    logarithmic: false,
                    step: 0.1,
                    decimals: 2,
                    suffix: ms,
                },
            );

            theme::rule(ui);

            changed |= compact_slider_row(
                ui,
                self.t(Key::RowJitter),
                self.t(Key::HintJitter),
                &mut draft.jitter_pct,
                SliderSpec {
                    range: 0.0..=100.0,
                    logarithmic: false,
                    step: 0.5,
                    decimals: 2,
                    suffix: "%",
                },
            );

            theme::rule(ui);
        }

        let pieces = self.t(Key::UnitPieces);
        changed |= compact_integer_row(
            ui,
            self.t(Key::RowLimit),
            self.t(Key::HintLimit),
            &mut draft.limit,
            0..=MAX_LIMIT,
            pieces,
        );

        theme::rule(ui);

        changed |= compact_slider_row(
            ui,
            self.t(Key::RowDelay),
            self.t(Key::HintDelay),
            &mut draft.delay_ms,
            SliderSpec {
                range: 0.0..=MAX_DELAY_MS,
                logarithmic: false,
                step: 10.0,
                decimals: 0,
                suffix: ms,
            },
        );

        let period_ms = match draft.mode {
            ClickMode::Sequence => draft.point_interval_ms,
            ClickMode::Random => draft.random_interval_ms,
            ClickMode::Cursor | ClickMode::Point => 1000.0 / draft.cps.max(0.2),
        };
        let effective_hold = draft.hold_ms.min((period_ms - 0.2).max(0.1));
        let feedback = if draft.mode != ClickMode::Random && draft.hold_ms > effective_hold + 0.001
        {
            Some((
                theme::warning(),
                self.fill(Key::HintHoldClamped, &[format_decimal(effective_hold, 2)]),
            ))
        } else if draft.limit > 0 {
            Some((
                theme::muted(),
                self.fill(Key::HintLimitOn, &[group_digits(draft.limit)]),
            ))
        } else {
            None
        };
        if let Some((color, text)) = feedback {
            ui.add_space(2.0);
            ui.add(
                egui::Label::new(RichText::new(text).font(theme::sans(10.0)).color(color)).wrap(),
            );
        }

        if changed {
            self.settings = draft;
            self.mark_dirty();
        }
    }

    fn draw_mode_tab(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = if ui.ctx().content_rect().height() < 600.0 {
            1.0
        } else {
            3.0
        };
        let mut draft = self.settings;
        let mut changed = false;
        let mut pipette_target = None;
        let mut add_point = false;
        let mut remove_point = None;

        let mut mode = draft.mode;
        let mut mode_changed = false;
        let target_hint = match draft.mode {
            ClickMode::Cursor => format!(
                "{} {}",
                self.t(Key::HintTarget),
                self.t(Key::CursorModeHint)
            ),
            ClickMode::Random => format!(
                "{} {}",
                self.t(Key::HintTarget),
                self.t(Key::HintRandomMode)
            ),
            _ => self.t(Key::HintTarget).to_string(),
        };
        let target_row = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let _ = row_label_bare(ui, self.t(Key::RowTarget), 24.0);

            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), 24.0),
                Layout::top_down(Align::Min),
                |ui| {
                    mode_changed = widgets::segmented(
                        ui,
                        "click_mode",
                        &mut mode,
                        &[
                            (ClickMode::Cursor, self.t(Key::TargetCursor)),
                            (ClickMode::Point, self.t(Key::TargetPoint)),
                            (ClickMode::Sequence, self.t(Key::TargetSequence)),
                            (ClickMode::Random, self.t(Key::TargetRandom)),
                        ],
                    );
                },
            );
        });
        let _ = target_row.response.on_hover_text(&target_hint);
        if mode_changed {
            match mode {
                ClickMode::Point if draft.target.is_none() => {
                    draft.target = crate::backend::cursor_position().ok();
                }
                ClickMode::Sequence if draft.point_count() == 0 => {
                    // Leave the list empty: the user adds points with the
                    // eyedropper, and readiness asks for two before starting.
                }
                _ => {}
            }
            draft.mode = mode;
            changed = true;
        }

        let blocked = self.target_blocked_reason();
        if blocked.is_some() && draft.mode != ClickMode::Cursor {
            if let Some(reason) = blocked {
                ui.add_space(2.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(self.t(reason))
                            .font(theme::sans(10.0))
                            .color(theme::warning()),
                    )
                    .wrap(),
                );
            }
        }

        if draft.mode == ClickMode::Point && draft.target.is_none() {
            draft.target = Some(crate::backend::cursor_position().unwrap_or(Point::new(0, 0)));
            changed = true;
        }

        if draft.mode == ClickMode::Sequence {
            theme::rule(ui);
            changed |= compact_slider_row(
                ui,
                self.t(Key::RowInterval),
                self.t(Key::HintInterval),
                &mut draft.point_interval_ms,
                SliderSpec {
                    range: 1.0..=MAX_POINT_INTERVAL_MS as f64,
                    logarithmic: false,
                    step: 1.0,
                    decimals: 0,
                    suffix: self.t(Key::UnitMs),
                },
            );

            theme::rule(ui);
            let points_row = ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let _ = row_label_bare(ui, self.t(Key::RowPoints), 22.0);

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let enabled = draft.point_count() < MAX_POINTS;
                    ui.add_enabled_ui(enabled, |ui| {
                        if widgets::text_button(
                            ui,
                            self.t(Key::AddPoint),
                            theme::muted(),
                            theme::accent(),
                        )
                        .clicked()
                        {
                            add_point = true;
                        }
                    });
                    ui.label(
                        RichText::new(format!("{} / {}", draft.point_count(), MAX_POINTS))
                            .font(theme::mono(9.0))
                            .color(theme::dim()),
                    );
                });
            });
            let _ = points_row.response.on_hover_text(self.t(Key::HintPoints));

            if draft.point_count() == 0 {
                ui.add_space(2.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(self.t(Key::PointsEmpty))
                            .font(theme::sans(10.0))
                            .color(theme::muted()),
                    )
                    .wrap(),
                );
            }

            for index in 0..draft.point_count() {
                let Some(mut point) = draft.point_at(index) else {
                    continue;
                };
                let original = point;
                let active = self
                    .pipette
                    .as_ref()
                    .is_some_and(|pipette| pipette.target == PipetteTarget::Sequence(index));

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(
                        RichText::new(format!("{:02}", index + 1))
                            .font(theme::mono_medium(9.0))
                            .color(if active {
                                theme::accent()
                            } else {
                                theme::dim()
                            }),
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_enabled_ui(blocked.is_none(), |ui| {
                            let response = widgets::pipette_button(ui, active);
                            let response = response.on_hover_text(self.t(Key::PickHint));
                            if response.clicked() {
                                pipette_target = Some(PipetteTarget::Sequence(index));
                            }
                        });
                        if widgets::text_button(ui, "×", theme::dim(), theme::danger())
                            .on_hover_text(self.t(Key::RemovePoint))
                            .clicked()
                        {
                            remove_point = Some(index);
                        }

                        let remaining = ui.available_width();
                        ui.allocate_ui_with_layout(
                            Vec2::new(remaining, 20.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                changed |= widgets::coordinate_field(ui, &mut point.x, "X");
                                changed |= widgets::coordinate_field(ui, &mut point.y, "Y");
                            },
                        );
                    });
                });

                if point != original {
                    draft.set_point(index, point);
                    changed = true;
                }
            }
        } else if draft.mode == ClickMode::Point {
            if let Some(point) = &mut draft.target {
                theme::rule(ui);
                let active = self
                    .pipette
                    .as_ref()
                    .is_some_and(|pipette| pipette.target == PipetteTarget::Point);
                let mut point_value = *point;

                let coords_row = ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let _ = row_label_bare(ui, self.t(Key::RowCoords), 22.0);

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_enabled_ui(blocked.is_none(), |ui| {
                            let response = widgets::pipette_button(ui, active);
                            let response = response.on_hover_text(self.t(Key::PickHint));
                            if response.clicked() {
                                pipette_target = Some(PipetteTarget::Point);
                            }
                        });
                        let remaining = ui.available_width();
                        ui.allocate_ui_with_layout(
                            Vec2::new(remaining, 20.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                changed |= widgets::coordinate_field(ui, &mut point_value.x, "X");
                                changed |= widgets::coordinate_field(ui, &mut point_value.y, "Y");
                            },
                        );
                    });
                });
                let _ = coords_row.response.on_hover_text(self.t(Key::HintCoords));

                if point_value != *point {
                    *point = point_value;
                    changed = true;
                }
            }
        }

        theme::rule(ui);

        let mut multi = draft.multi_click;
        let multi_hint = format!("{} {}", self.t(Key::HintMulti), self.t(Key::MultiHint));
        let multi_changed = compact_choice_row(ui, self.t(Key::RowMulti), &multi_hint, |ui| {
            widgets::segmented(
                ui,
                "multi_click",
                &mut multi,
                &[
                    (1, self.t(Key::MultiOne)),
                    (2, self.t(Key::MultiDouble)),
                    (3, self.t(Key::MultiTriple)),
                ],
            )
        });
        if multi_changed {
            draft.multi_click = multi;
            changed = true;
        }

        if let Some(index) = remove_point {
            draft.remove_point(index);
            changed = true;
            if self
                .pipette
                .as_ref()
                .is_some_and(|pipette| pipette.target == PipetteTarget::Sequence(index))
            {
                self.pipette = None;
            }
        }

        if changed {
            self.settings = draft;
            self.mark_dirty();
        }

        if add_point {
            self.add_sequence_point();
        } else if let Some(target) = pipette_target {
            self.start_pipette(target);
        }
    }

    fn profile_label(&self, active: &ActiveProfile) -> String {
        match active {
            ActiveProfile::Preset(preset) => self.t(preset.label_key()).to_string(),
            ActiveProfile::User(name) => name.clone(),
        }
    }

    fn active_profile_settings(&self) -> Option<Settings> {
        match &self.active_profile {
            ActiveProfile::Preset(preset) => Some(preset.settings()),
            ActiveProfile::User(name) => self
                .profiles
                .iter()
                .find(|profile| profile.name.eq_ignore_ascii_case(name))
                .map(|profile| profile.settings),
        }
    }

    fn profile_modified(&self) -> bool {
        let Ok(current) = self.settings.to_settings() else {
            return true;
        };
        match self.active_profile_settings() {
            Some(saved) => !current.equivalent(&saved),
            None => true,
        }
    }

    fn button_label(&self, button: Button) -> &'static str {
        self.t(match button {
            Button::Left => Key::ButtonLeft,
            Button::Right => Key::ButtonRight,
            Button::Middle => Key::ButtonMiddle,
        })
    }

    fn profile_summary(&self) -> Option<String> {
        let settings = self.settings.to_settings().ok()?;
        Some(format!(
            "{} CPS · {} · {} ms",
            format_decimal(settings.cps, 2),
            self.button_label(settings.button),
            format_decimal(settings.hold_ms(), 2)
        ))
    }

    fn select_profile(&mut self, active: ActiveProfile) {
        let settings = match &active {
            ActiveProfile::Preset(preset) => preset.settings(),
            ActiveProfile::User(name) => {
                let Some(profile) = self
                    .profiles
                    .iter()
                    .find(|profile| profile.name.eq_ignore_ascii_case(name))
                else {
                    return;
                };
                profile.settings
            }
        };

        self.settings = settings.into();
        self.active_profile = active;
        self.mark_dirty();
        let label = self.profile_label(&self.active_profile);
        self.notice = Some(self.fill(Key::ProfileApplied, &[label]));
    }

    fn save_current_profile(&mut self) {
        let ActiveProfile::User(name) = self.active_profile.clone() else {
            self.open_save_as();
            return;
        };
        let Ok(settings) = self.settings.to_settings() else {
            return;
        };
        if let Some(profile) = self
            .profiles
            .iter_mut()
            .find(|profile| profile.name.eq_ignore_ascii_case(&name))
        {
            profile.settings = settings;
        }
        self.notice = Some(self.t(Key::ProfileSaved).to_string());
        self.save_config();
    }

    fn unique_profile_name(&self, base: &str) -> String {
        let name = profiles::unique_name(base, &self.profiles);
        let reserved = Preset::ALL.iter().any(|preset| {
            self.t(preset.label_key()).eq_ignore_ascii_case(&name)
                || preset.id().eq_ignore_ascii_case(&name)
        });
        if reserved {
            profiles::unique_name(&format!("{name} (2)"), &self.profiles)
        } else {
            name
        }
    }

    fn open_save_as(&mut self) {
        let name = match &self.active_profile {
            ActiveProfile::Preset(preset) => self.t(preset.label_key()).to_string(),
            ActiveProfile::User(name) => name.clone(),
        };
        self.profile_dialog = Some(ProfileDialog::SaveAs { name });
    }

    fn open_export(&mut self) {
        let name = self.profile_label(&self.active_profile);
        let path = profiles::default_export_path(&name);
        self.profile_dialog = Some(ProfileDialog::Export {
            path: path.to_string_lossy().into_owned(),
        });
    }

    fn commit_save_as(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            self.notice = Some(self.t(Key::ProfileNameEmpty).to_string());
            return false;
        }
        if name.chars().count() > profiles::MAX_PROFILE_NAME_CHARS {
            self.notice = Some(self.t(Key::ProfileNameTooLong).to_string());
            return false;
        }
        let Ok(settings) = self.settings.to_settings() else {
            return false;
        };

        let name = self.unique_profile_name(name);
        self.profiles.push(Profile::new(name.clone(), settings));
        self.active_profile = ActiveProfile::User(name.clone());
        self.notice = Some(self.fill(Key::ProfileSavedAs, &[name]));
        self.save_config();
        true
    }

    fn commit_export(&mut self, path: &str) -> bool {
        let name = self.profile_label(&self.active_profile);
        let result = profiles::resolve_text_path(path, &name).and_then(|path| {
            let Ok(settings) = self.settings.to_settings() else {
                anyhow::bail!("the click settings are invalid");
            };
            profiles::export_file(&path, &name, &settings)
        });

        match result {
            Ok(path) => {
                self.notice =
                    Some(self.fill(Key::ProfileExported, &[path.to_string_lossy().into_owned()]));
                true
            }
            Err(error) => {
                self.notice = Some(self.fill(Key::ProfileExportFailed, &[format!("{error:#}")]));
                false
            }
        }
    }

    fn commit_import(&mut self, path: &str) -> bool {
        let result = profiles::resolve_text_path(path, "profile")
            .and_then(|path| profiles::import_file(&path));

        match result {
            Ok(profile) => {
                let name = self.unique_profile_name(&profile.name);
                let settings = profile.settings;
                self.profiles.push(Profile::new(name.clone(), settings));
                self.settings = settings.into();
                self.active_profile = ActiveProfile::User(name.clone());
                self.mark_dirty();
                self.notice = Some(self.fill(Key::ProfileImported, &[name]));
                self.save_config();
                true
            }
            Err(error) => {
                self.notice = Some(self.fill(Key::ProfileImportFailed, &[format!("{error:#}")]));
                false
            }
        }
    }

    fn draw_profile_section(&mut self, ui: &mut Ui) {
        let entries: Vec<(ActiveProfile, String)> = Preset::ALL
            .iter()
            .map(|preset| ActiveProfile::Preset(*preset))
            .chain(
                self.profiles
                    .iter()
                    .map(|profile| ActiveProfile::User(profile.name.clone())),
            )
            .map(|active| {
                let label = self.profile_label(&active);
                (active, label)
            })
            .collect();

        let mut selected = self.active_profile.clone();
        let selected_text = self.profile_label(&selected);
        let width = ui.available_width().min(210.0);
        let _ = compact_choice_row(
            ui,
            self.t(Key::RowProfile),
            self.t(Key::HintProfile),
            |ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    egui::ComboBox::from_id_salt("profile")
                        .width(width)
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            for (active, label) in &entries {
                                ui.selectable_value(&mut selected, active.clone(), label);
                            }
                        });
                });
                false
            },
        );
        if selected != self.active_profile {
            self.select_profile(selected);
        }

        if let Some(summary) = self.profile_summary() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.add(
                    egui::Label::new(
                        RichText::new(summary)
                            .font(theme::sans(10.5))
                            .color(theme::muted()),
                    )
                    .truncate(),
                );
                if self.profile_modified() {
                    ui.label(
                        RichText::new(self.t(Key::ProfileModified))
                            .font(theme::mono_medium(8.0))
                            .color(theme::warning()),
                    );
                }
            });
        }

        let is_user = matches!(self.active_profile, ActiveProfile::User(_));
        let mut save = false;
        let mut save_as = false;
        let mut export = false;
        let mut import = false;
        let mut delete = false;

        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if widgets::text_button(
                ui,
                self.t(Key::ProfileSave),
                theme::muted(),
                theme::accent(),
            )
            .clicked()
            {
                save = true;
            }
            if widgets::text_button(
                ui,
                self.t(Key::ProfileSaveAs),
                theme::muted(),
                theme::accent(),
            )
            .clicked()
            {
                save_as = true;
            }
            if widgets::text_button(
                ui,
                self.t(Key::ProfileExport),
                theme::muted(),
                theme::accent(),
            )
            .clicked()
            {
                export = true;
            }
            if widgets::text_button(
                ui,
                self.t(Key::ProfileImport),
                theme::muted(),
                theme::accent(),
            )
            .clicked()
            {
                import = true;
            }
            ui.add_enabled_ui(is_user, |ui| {
                if widgets::text_button(
                    ui,
                    self.t(Key::ProfileDelete),
                    theme::muted(),
                    theme::danger(),
                )
                .clicked()
                {
                    delete = true;
                }
            });
        });

        if save {
            self.save_current_profile();
        }
        if save_as {
            self.open_save_as();
        }
        if export {
            self.open_export();
        }
        if import {
            self.profile_dialog = Some(ProfileDialog::Import {
                path: String::new(),
            });
        }
        if delete {
            self.confirm_profile_delete = true;
        }
    }

    fn draw_profile_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.profile_dialog.take() else {
            return;
        };

        let (id, title, question, text, label, action, hint) = match &dialog {
            ProfileDialog::SaveAs { .. } => (
                "profile_save_as",
                Key::ProfileSaveAsTitle,
                Key::ProfileSaveAsQuestion,
                Key::ProfileSaveAsText,
                Key::ProfileNameLabel,
                Key::ProfileDialogSave,
                self.t(Key::ProfileNameHint),
            ),
            ProfileDialog::Export { .. } => (
                "profile_export",
                Key::ProfileExportTitle,
                Key::ProfileExportQuestion,
                Key::ProfileExportText,
                Key::ProfilePathLabel,
                Key::ProfileDialogSave,
                self.t(Key::ProfilePathHint),
            ),
            ProfileDialog::Import { .. } => (
                "profile_import",
                Key::ProfileImportTitle,
                Key::ProfileImportQuestion,
                Key::ProfileImportText,
                Key::ProfilePathLabel,
                Key::ProfileDialogOpen,
                self.t(Key::ProfilePathHint),
            ),
        };

        let overwrite = match &dialog {
            ProfileDialog::Export { path } => {
                profiles::resolve_text_path(path, "profile").is_ok_and(|path| path.exists())
            }
            _ => false,
        };

        let mut confirmed = false;
        let mut closed = false;
        let modal = egui::Modal::new(egui::Id::new(id))
            .backdrop_color(Color32::from_black_alpha(120))
            .frame(theme::dialog())
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(
                    RichText::new(self.t(title))
                        .font(theme::mono_medium(9.0))
                        .color(theme::dim()),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(self.t(question))
                        .font(theme::sans_semibold(20.0))
                        .color(theme::text()),
                );
                ui.add_space(8.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(self.t(text))
                            .font(theme::sans(11.5))
                            .color(theme::muted()),
                    )
                    .wrap(),
                );
                ui.add_space(14.0);
                ui.label(
                    RichText::new(self.t(label))
                        .font(theme::mono_medium(8.0))
                        .color(theme::dim()),
                );
                ui.add_space(4.0);

                let response = match &mut dialog {
                    ProfileDialog::SaveAs { name }
                    | ProfileDialog::Export { path: name }
                    | ProfileDialog::Import { path: name } => widgets::text_field(ui, name, hint),
                };

                if overwrite {
                    ui.add_space(6.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(self.t(Key::ProfileOverwriteWarning))
                                .font(theme::sans(10.0))
                                .color(theme::warning()),
                        )
                        .wrap(),
                    );
                }

                if response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    confirmed = true;
                }

                ui.add_space(18.0);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::solid_button(
                            ui,
                            self.t(action),
                            theme::accent(),
                            theme::on_accent(),
                        )
                        .clicked()
                        {
                            confirmed = true;
                        }
                        if widgets::text_button(
                            ui,
                            self.t(Key::Cancel),
                            theme::muted(),
                            theme::text(),
                        )
                        .clicked()
                        {
                            closed = true;
                        }
                    });
                });
            });

        theme::paint_dialog_edge(ctx, modal.response.layer_id, modal.response.rect);

        if modal.should_close() {
            closed = true;
        }

        let mut keep = !confirmed && !closed;
        if confirmed {
            keep = match &dialog {
                ProfileDialog::SaveAs { name } => !self.commit_save_as(name),
                ProfileDialog::Export { path } => !self.commit_export(path),
                ProfileDialog::Import { path } => !self.commit_import(path),
            };
        }

        if keep {
            self.profile_dialog = Some(dialog);
        }
    }

    fn draw_profile_delete(&mut self, ctx: &egui::Context) {
        if !self.confirm_profile_delete {
            return;
        }
        let ActiveProfile::User(name) = self.active_profile.clone() else {
            self.confirm_profile_delete = false;
            return;
        };

        let question = self.fill(Key::ProfileDeleteQuestion, std::slice::from_ref(&name));
        let mut confirmed = false;
        let mut closed = false;
        let modal = egui::Modal::new(egui::Id::new("profile_delete"))
            .backdrop_color(Color32::from_black_alpha(120))
            .frame(theme::dialog())
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.label(
                    RichText::new(self.t(Key::ProfileDeleteTitle))
                        .font(theme::mono_medium(9.0))
                        .color(theme::dim()),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(&question)
                        .font(theme::sans_semibold(22.0))
                        .color(theme::text()),
                );
                ui.add_space(8.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(self.t(Key::ProfileDeleteText))
                            .font(theme::sans(12.0))
                            .color(theme::muted()),
                    )
                    .wrap(),
                );
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::solid_button(
                            ui,
                            self.t(Key::ProfileDelete),
                            theme::danger(),
                            theme::panel(),
                        )
                        .clicked()
                        {
                            confirmed = true;
                        }
                        if widgets::text_button(
                            ui,
                            self.t(Key::Cancel),
                            theme::muted(),
                            theme::text(),
                        )
                        .clicked()
                        {
                            closed = true;
                        }
                    });
                });
            });

        theme::paint_dialog_edge(ctx, modal.response.layer_id, modal.response.rect);

        if modal.should_close() {
            closed = true;
        }

        if confirmed {
            self.profiles
                .retain(|profile| !profile.name.eq_ignore_ascii_case(&name));
            self.active_profile = ActiveProfile::Preset(Preset::Default);
            self.notice = Some(self.fill(Key::ProfileDeleted, &[name]));
            self.save_config();
            self.confirm_profile_delete = false;
        } else if closed {
            self.confirm_profile_delete = false;
        }
    }

    fn draw_system_tab(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = if ui.ctx().content_rect().height() < 600.0 {
            1.0
        } else {
            3.0
        };

        self.draw_profile_section(ui);
        theme::rule(ui);

        let mut selected = self.language;
        let _ = compact_choice_row(
            ui,
            self.t(Key::RowLanguage),
            self.t(Key::HintLanguage),
            |ui| {
                let width = ui.available_width().min(210.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    egui::ComboBox::from_id_salt("language")
                        .width(width)
                        .selected_text(selected.native_label())
                        .show_ui(ui, |ui| {
                            for language in Language::ALL {
                                ui.selectable_value(
                                    &mut selected,
                                    language,
                                    language.native_label(),
                                );
                            }
                        });
                });
                false
            },
        );
        if selected != self.language {
            self.set_language(selected);
        }

        theme::rule(ui);

        let mut selected = self.theme;
        let _ = compact_choice_row(ui, self.t(Key::RowTheme), self.t(Key::HintTheme), |ui| {
            let width = ui.available_width().min(210.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                egui::ComboBox::from_id_salt("theme")
                    .width(width)
                    .selected_text(self.theme_label(selected))
                    .show_ui(ui, |ui| {
                        for theme in Theme::ALL {
                            ui.selectable_value(&mut selected, theme, self.theme_label(theme));
                        }
                    });
            });
            false
        });
        if selected != self.theme {
            self.set_theme(ui.ctx(), selected);
        }

        theme::rule(ui);

        if always_on_top_supported() {
            let mut enabled = self.always_on_top;
            let changed = compact_choice_row(
                ui,
                self.t(Key::RowAlwaysOnTop),
                self.t(Key::HintAlwaysOnTop),
                |ui| {
                    widgets::checkbox(ui, &mut enabled, self.t(Key::Enabled));
                    enabled != self.always_on_top
                },
            );

            if changed {
                self.set_always_on_top(ui.ctx(), enabled);
            }

            theme::rule(ui);
        }

        if crate::autostart::SUPPORTED {
            let mut enabled = self.autostart_enabled;
            let hint = format!(
                "{} {}",
                self.t(Key::HintAutostart),
                self.t(Key::AutostartHint)
            );
            let changed = compact_choice_row(ui, self.t(Key::RowAutostart), &hint, |ui| {
                widgets::checkbox(ui, &mut enabled, self.t(Key::Enabled));
                enabled != self.autostart_enabled
            });

            if changed && enabled != self.autostart_enabled {
                let start_hidden = enabled && self.minimize_to_tray;
                match crate::autostart::set_enabled(enabled, start_hidden) {
                    Ok(()) => {
                        self.autostart_enabled = enabled;
                        self.notice = Some(
                            self.t(if enabled {
                                Key::AutostartOn
                            } else {
                                Key::AutostartOff
                            })
                            .to_string(),
                        );
                    }
                    Err(error) => {
                        self.notice =
                            Some(self.fill(Key::AutostartFailed, &[format!("{error:#}")]));
                    }
                }
            }
        } else {
            let _ = row_label(
                ui,
                self.t(Key::RowAutostart),
                self.t(Key::HintAutostart),
                22.0,
            );
            ui.add_space(3.0);
            ui.add(
                egui::Label::new(
                    RichText::new(self.t(Key::AutostartUnavailable))
                        .font(theme::sans(10.0))
                        .color(theme::warning()),
                )
                .wrap(),
            );
        }

        #[cfg(windows)]
        {
            theme::rule(ui);
            let mut tray = self.minimize_to_tray;
            let changed = compact_choice_row(
                ui,
                self.t(Key::RowMinimizeTray),
                self.t(Key::HintMinimizeTray),
                |ui| {
                    widgets::checkbox(ui, &mut tray, self.t(Key::Enabled));
                    tray != self.minimize_to_tray
                },
            );
            if changed {
                self.minimize_to_tray = tray;
                self.sync_tray(ui.ctx());
                if self.autostart_enabled {
                    // Keep the autostart entry in sync with the new tray
                    // preference, otherwise it would open a hidden window
                    // without a tray icon (or show a window unexpectedly).
                    let _ = crate::autostart::set_enabled(true, tray);
                }
                self.save_config();
            }

            if !self.elevated {
                theme::rule(ui);
                ui.add_space(2.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(self.t(Key::ElevationWarn))
                            .font(theme::sans(10.0))
                            .color(theme::warning()),
                    )
                    .wrap(),
                );
                ui.add_space(3.0);
                if widgets::text_button(
                    ui,
                    self.t(Key::ElevationRestart),
                    theme::accent(),
                    theme::accent(),
                )
                .clicked()
                {
                    match crate::platform::restart_elevated() {
                        Ok(()) => {
                            self.notice = Some(self.t(Key::ElevationRestarting).to_string());
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        Err(error) => {
                            self.notice =
                                Some(self.fill(Key::ElevationFailed, &[format!("{error:#}")]));
                        }
                    }
                }
            }
        }

        self.draw_update_section(ui);

        theme::rule(ui);
        ui.add_space(2.0);
        if widgets::text_button(
            ui,
            self.t(Key::DefaultsButton),
            theme::muted(),
            theme::danger(),
        )
        .clicked()
        {
            self.confirm_defaults = true;
        }
    }

    /// Shared layout for the two destructive confirmations. Returns
    /// `Some(true)` for the primary action, `Some(false)` when the modal was
    /// dismissed, and `None` while it stays open.
    fn confirmation_modal(&self, ctx: &egui::Context, spec: Confirmation<'_>) -> Option<bool> {
        let mut close = false;
        let mut confirmed = false;

        let modal = egui::Modal::new(egui::Id::new(spec.id))
            .backdrop_color(Color32::from_black_alpha(120))
            .frame(theme::dialog())
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.label(
                    RichText::new(self.t(spec.title))
                        .font(theme::mono_medium(9.0))
                        .color(theme::dim()),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(self.t(spec.question))
                        .font(theme::sans_semibold(22.0))
                        .color(theme::text()),
                );
                ui.add_space(8.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(self.t(spec.text))
                            .font(theme::sans(12.0))
                            .color(theme::muted()),
                    )
                    .wrap(),
                );
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let response = if spec.solid {
                            widgets::solid_button(
                                ui,
                                self.t(spec.confirm),
                                theme::danger(),
                                theme::panel(),
                            )
                        } else {
                            widgets::text_button(
                                ui,
                                self.t(spec.confirm),
                                theme::danger(),
                                theme::danger(),
                            )
                        };
                        if response.clicked() {
                            confirmed = true;
                        }
                        if widgets::text_button(
                            ui,
                            self.t(Key::Cancel),
                            theme::muted(),
                            theme::text(),
                        )
                        .clicked()
                        {
                            close = true;
                        }
                    });
                });
            });

        theme::paint_dialog_edge(ctx, modal.response.layer_id, modal.response.rect);

        if modal.should_close() {
            close = true;
        }

        if confirmed {
            Some(true)
        } else if close {
            Some(false)
        } else {
            None
        }
    }

    fn draw_defaults_confirmation(&mut self, ctx: &egui::Context) {
        if !self.confirm_defaults {
            return;
        }

        match self.confirmation_modal(
            ctx,
            Confirmation {
                id: "restore_defaults",
                title: Key::DefaultsTitle,
                question: Key::DefaultsQuestion,
                text: Key::DefaultsText,
                confirm: Key::DefaultsConfirm,
                solid: false,
            },
        ) {
            Some(true) => {
                self.confirm_defaults = false;
                self.restore_defaults(ctx);
            }
            Some(false) => self.confirm_defaults = false,
            None => {}
        }
    }

    fn target_blocked_reason(&self) -> Option<Key> {
        if crate::backend::target_supported() {
            return None;
        }

        #[cfg(target_os = "linux")]
        {
            if crate::backend::linux::is_wayland() {
                return Some(Key::TargetWayland);
            }
        }

        Some(Key::TargetNoDisplay)
    }

    #[cfg(windows)]
    fn draw_hotkeys_tab(&mut self, ui: &mut Ui) {
        let mut start = None;

        ui.horizontal(|ui| {
            let _ = row_label(
                ui,
                self.t(Key::RowToggleHotkey),
                self.t(Key::HintToggleHotkey),
                22.0,
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let recording = self.recording == Some(RecordTarget::Toggle);
                let response = hotkey_field(
                    ui,
                    &self.hotkey,
                    recording,
                    self.t(Key::RecordWaiting),
                    self.t(Key::NotSet),
                );
                if response.clicked() {
                    start = Some(RecordTarget::Toggle);
                }
                if self.daemon_hotkey_active.is_empty() {
                    response.on_hover_text(self.t(Key::HotkeyHint));
                } else {
                    response.on_hover_text(
                        self.fill(Key::Registered, &[hotkey_label(&self.daemon_hotkey_active)]),
                    );
                }
            });
        });

        theme::rule(ui);

        ui.horizontal(|ui| {
            let _ = row_label(
                ui,
                self.t(Key::RowHoldHotkey),
                self.t(Key::HintHoldHotkey),
                22.0,
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let recording = self.recording == Some(RecordTarget::Hold);
                let response = hotkey_field(
                    ui,
                    &self.hold_hotkey,
                    recording,
                    self.t(Key::RecordWaiting),
                    self.t(Key::NotSet),
                );
                if response.clicked() {
                    start = Some(RecordTarget::Hold);
                }
                if !self.hold_hotkey.is_empty()
                    && widgets::text_button(ui, self.t(Key::Clear), theme::dim(), theme::danger())
                        .clicked()
                {
                    self.clear_hold_hotkey();
                }
                if !self.daemon_hold_hotkey_active.is_empty() {
                    let _ = response.on_hover_text(self.fill(
                        Key::Registered,
                        &[hotkey_label(&self.daemon_hold_hotkey_active)],
                    ));
                } else {
                    let _ = response.on_hover_text(self.t(Key::HotkeyHint));
                }
            });
        });

        let hint = if let Some(target) = self.recording {
            Some((
                theme::accent(),
                self.fill(
                    Key::RecordHint,
                    &[match target {
                        RecordTarget::Toggle => self.t(Key::RecordTargetToggle),
                        RecordTarget::Hold => self.t(Key::RecordTargetHold),
                    }
                    .to_string()],
                ),
            ))
        } else if let Some(error) = &self.daemon_hotkey_error {
            Some((theme::danger(), error.clone()))
        } else if self.daemon_hotkey_paused {
            Some((theme::muted(), self.t(Key::HotkeyPaused).to_string()))
        } else {
            None
        };
        if let Some((color, text)) = hint {
            ui.add_space(6.0);
            ui.add(
                egui::Label::new(RichText::new(text).font(theme::sans(10.0)).color(color)).wrap(),
            );
        }

        if let Some(target) = start {
            if self.recording == Some(target) {
                self.stop_recording(true);
            } else {
                ui.ctx().memory_mut(|memory| memory.stop_text_input());
                self.start_recording(target);
            }
        }
    }

    #[cfg(not(windows))]
    fn draw_hotkeys_tab(&mut self, ui: &mut Ui) {
        let bind = self.toggle_bind();

        let global_hint = format!(
            "{} {} {}",
            self.t(Key::HintGlobal),
            self.t(Key::BindHint),
            self.t(Key::HoldWindowsOnly)
        );
        ui.horizontal(|ui| {
            let _ = row_label(ui, self.t(Key::RowGlobal), &global_hint, 22.0);

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let response = ui.add(
                    egui::Label::new(
                        RichText::new("MOD+ALT+C")
                            .font(theme::mono_medium(10.0))
                            .color(theme::text()),
                    )
                    .sense(Sense::click()),
                );
                let response = response
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(format!("{}\n{bind}", self.t(Key::CopyBind)));
                if response.clicked() {
                    ui.ctx().copy_text(bind.clone());
                    self.notice = Some(self.t(Key::BindCopied).to_string());
                }
            });
        });
    }

    #[cfg(not(windows))]
    fn toggle_bind(&self) -> String {
        let executable = std::env::current_exe()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "v-autoclicker".to_string());
        let executable = executable.replace('"', "\\\"");
        format!("Mod+Alt+C {{ spawn \"{executable}\" \"toggle\"; }}")
    }

    #[cfg(not(windows))]
    fn draw_footer(&mut self, ui: &mut Ui) {
        let mut quit_clicked = false;

        ui.horizontal(|ui| {
            let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
            ui.painter().circle_filled(
                dot.center(),
                2.5,
                if self.online {
                    theme::accent()
                } else {
                    theme::dim()
                },
            );

            let response = ui.label(
                RichText::new(self.t(if self.online {
                    Key::DaemonRunning
                } else {
                    Key::DaemonStopped
                }))
                .font(theme::mono(9.0))
                .color(theme::muted()),
            );
            if let Some(reason) = &self.offline_reason {
                let _ = response.on_hover_text(reason);
            }

            ui.add_space(3.0);
            ui.label(
                RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .font(theme::mono(8.5))
                    .color(theme::dim()),
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_enabled_ui(self.online, |ui| {
                    if widgets::text_button(
                        ui,
                        self.t(Key::QuitDaemon),
                        theme::muted(),
                        theme::danger(),
                    )
                    .clicked()
                    {
                        quit_clicked = true;
                    }
                });
            });
        });

        if quit_clicked {
            self.confirm_quit = true;
        }
    }

    #[cfg(not(windows))]
    fn draw_quit_confirmation(&mut self, ctx: &egui::Context) {
        if !self.confirm_quit {
            return;
        }

        match self.confirmation_modal(
            ctx,
            Confirmation {
                id: "quit_daemon",
                title: Key::QuitTitle,
                question: Key::QuitQuestion,
                text: Key::QuitText,
                confirm: Key::QuitConfirm,
                solid: true,
            },
        ) {
            Some(true) => {
                self.confirm_quit = false;
                self.request_quit();
            }
            Some(false) => self.confirm_quit = false,
            None => {}
        }
    }
}

impl egui_software_backend::App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _backend: &mut SoftwareBackend) {
        if ui.ctx().input(|input| input.viewport().close_requested()) {
            // Persist the draft even if the window is closed before the next
            // debounce tick has a chance to save it.
            self.save_config();
        }

        self.apply_auto_scale(ui.ctx());
        #[cfg(windows)]
        self.sync_tray(ui.ctx());
        #[cfg(windows)]
        if self.start_hidden {
            // If the tray icon could not be created, never leave the app
            // running without any window to interact with.
            self.start_hidden = false;
            if self.tray.is_none() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Visible(true));
            }
        }
        self.poll_pipette(ui.ctx());
        self.poll_save_errors();

        self.capture_hotkey(ui.ctx());
        self.drain_events(ui.ctx());
        self.check_worker_health(ui.ctx());
        self.maybe_apply_settings();
        self.poll_updates(ui.ctx());

        theme::paint_backdrop(ui);

        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(theme::backdrop_fill())
                    .inner_margin(Margin::symmetric(16, 10)),
            )
            .show_inside(ui, |ui| {
                #[cfg(not(windows))]
                if !self.simple_mode {
                    // Keep the footer pinned, but draw it as a regular card so
                    // it matches the hero and tab sections instead of reading
                    // as a flat full-width bar.
                    egui::Panel::bottom("daemon_footer")
                        .frame(
                            Frame::new()
                                .fill(Color32::TRANSPARENT)
                                .inner_margin(Margin::symmetric(0, 0)),
                        )
                        .show_separator_line(false)
                        .show_inside(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.set_max_width(ui.available_width().min(CONTENT_WIDTH));
                                ui.add_space(6.0);
                                theme::card_show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    self.draw_footer(ui);
                                });
                                ui.add_space(8.0);
                            });
                        });
                }

                ui.vertical_centered(|ui| {
                    ui.set_max_width(ui.available_width().min(CONTENT_WIDTH));
                    self.draw_topbar(ui);
                    ui.add_space(8.0);

                    if self.simple_mode {
                        self.draw_simple_body(ui);
                    } else {
                        theme::card_show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            self.draw_hero(ui);
                            self.draw_metrics(ui);
                        });
                        ui.add_space(6.0);
                        self.draw_notice(ui);
                        ui.add_space(8.0);
                        self.draw_tabs(ui);
                        ui.add_space(8.0);

                        egui::ScrollArea::vertical()
                            .id_salt(("tab_scroll", self.tab))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                theme::card_show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    match self.tab {
                                        Tab::Clicks => self.draw_clicks_tab(ui),
                                        Tab::Mode => self.draw_mode_tab(ui),
                                        Tab::Hotkeys => self.draw_hotkeys_tab(ui),
                                        Tab::System => self.draw_system_tab(ui),
                                    }
                                });
                            });
                    }
                });
            });

        #[cfg(not(windows))]
        self.draw_quit_confirmation(ui.ctx());
        self.draw_defaults_confirmation(ui.ctx());
        self.draw_update_confirmation(ui.ctx());
        self.draw_profile_dialog(ui.ctx());
        self.draw_profile_delete(ui.ctx());

        // The debounce needs its own wake-up: the software backend waits for
        // events, so an idle window would otherwise not apply or save the
        // pending draft until the user interacts again.
        if let Some(changed_at) = self.draft_changed_at {
            ui.ctx()
                .request_repaint_after(APPLY_DEBOUNCE.saturating_sub(changed_at.elapsed()));
        }
    }
}

impl Drop for GuiApp {
    fn drop(&mut self) {
        // The worker owns the daemon cleanup (resume, OFF and, on Windows,
        // QUIT). Join it so the process does not exit before that best-effort
        // IPC completes.
        let _ = self.commands.send(Command::Shutdown);
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
        self.save_config();
    }
}

pub fn run(start_hidden: bool) -> Result<(), Box<dyn std::error::Error>> {
    // A hidden start only makes sense together with a tray icon; the config is
    // read again by the app, but the visibility and the initial layout have to
    // be known before the window is created.
    let loaded = config::load();
    let start_hidden = start_hidden && cfg!(windows) && loaded.config.minimize_to_tray;
    let window = window_size(loaded.config.simple_mode);
    let min_window = min_window_size(loaded.config.simple_mode);

    let mut viewport = egui::ViewportBuilder::default()
        .with_app_id("v-autoclicker")
        .with_inner_size(window)
        .with_min_inner_size(min_window)
        .with_title("V-AutoClicker")
        .with_visible(!start_hidden);

    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }

    if loaded.config.always_on_top && always_on_top_supported() {
        viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnTop);
    }

    let config = SoftwareBackendAppConfiguration::new().viewport_builder(viewport);

    egui_software_backend::run_app_with_software_backend(config, move |ctx| {
        GuiApp::new(ctx, start_hidden)
    })
    .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
}

fn load_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../../assets/icon/v-autoclicker-256.png");
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    buffer.truncate(info.buffer_size());

    if info.color_type != png::ColorType::Rgba {
        return None;
    }

    Some(egui::IconData {
        rgba: buffer,
        width: info.width,
        height: info.height,
    })
}

/// Always-on-top is a real window hint on Windows and X11 only. On Wayland
/// (and every other platform) winit ignores `set_window_level`, so the
/// controls stay hidden instead of pretending to work.
fn always_on_top_supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        !crate::backend::linux::is_wayland()
    }
    #[cfg(not(target_os = "linux"))]
    {
        cfg!(windows)
    }
}

fn window_size(simple: bool) -> [f32; 2] {
    if simple {
        SIMPLE_WINDOW_SIZE
    } else {
        WINDOW_SIZE
    }
}

fn min_window_size(simple: bool) -> [f32; 2] {
    if simple {
        SIMPLE_MIN_WINDOW_SIZE
    } else {
        MIN_WINDOW_SIZE
    }
}

fn auto_scale_for(monitor_size: Vec2, zoom_factor: f32, window: [f32; 2]) -> f32 {
    // `monitor_size` is reported in egui points at the current zoom level.
    // Convert it back to the unscaled logical size so the result does not
    // depend on the scale we already applied.
    let logical = monitor_size * zoom_factor;
    if !logical.is_finite() || logical.x <= 0.0 || logical.y <= 0.0 {
        return MAX_AUTO_SCALE;
    }

    let fit = ((logical.x - SCREEN_MARGIN_X) / window[0])
        .min((logical.y - SCREEN_MARGIN_Y) / window[1])
        .min(MAX_AUTO_SCALE);
    fit.clamp(MIN_AUTO_SCALE, MAX_AUTO_SCALE)
}

fn row_label(ui: &mut Ui, title: &str, hint: &str, height: f32) -> egui::Response {
    row_label_bare(ui, title, height).on_hover_text(hint)
}

/// Row title without its own tooltip. Rows that own the hover surface use this
/// so the hint is shown once, from the full row, instead of twice.
fn row_label_bare(ui: &mut Ui, title: &str, height: f32) -> egui::Response {
    let galley =
        ui.painter()
            .layout_no_wrap(title.to_owned(), theme::sans_medium(12.5), theme::text());
    let (rect, response) = ui.allocate_exact_size(Vec2::new(LABEL_WIDTH, height), Sense::hover());
    ui.painter().galley(
        egui::pos2(rect.left(), rect.center().y - galley.size().y * 0.5),
        galley,
        theme::text(),
    );
    response
}

fn compact_slider_row(
    ui: &mut Ui,
    title: &str,
    hint: &str,
    value: &mut f64,
    spec: SliderSpec<'_>,
) -> bool {
    let mut changed = false;

    let row = ui.horizontal(|ui| {
        // Zero implicit spacing + explicit gaps keeps the right edge of the
        // input group exactly on the card grid for every suffix width.
        ui.spacing_mut().item_spacing.x = 0.0;
        let _ = row_label_bare(ui, title, 20.0);
        ui.add_space(6.0);

        let field_width = widgets::numeric_row_width();
        let slider_width = (ui.available_width() - field_width - 6.0).max(60.0);
        ui.allocate_ui_with_layout(
            Vec2::new(slider_width, 20.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.spacing_mut().slider_width = slider_width;
                changed |= widgets::parameter_slider(
                    ui,
                    value,
                    spec.range.clone(),
                    spec.logarithmic,
                    spec.step,
                )
                .changed();
            },
        );
        ui.add_space(6.0);

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed |= widgets::numeric_field(
                ui,
                value,
                spec.range,
                spec.step,
                spec.decimals,
                spec.suffix,
            );
        });
    });
    let _ = row.response.on_hover_text(hint);

    changed
}

fn compact_integer_row(
    ui: &mut Ui,
    title: &str,
    hint: &str,
    value: &mut u64,
    range: std::ops::RangeInclusive<u64>,
    suffix: &str,
) -> bool {
    let mut changed = false;

    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let _ = row_label_bare(ui, title, 20.0);
        ui.add_space(6.0);

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed |= widgets::integer_field(ui, value, range, 1.0, suffix);
        });
    });
    let _ = row.response.on_hover_text(hint);

    changed
}

fn update_supported() -> bool {
    crate::update::asset_name_for(std::env::consts::OS, std::env::consts::ARCH).is_some()
}

fn compact_choice_row(
    ui: &mut Ui,
    title: &str,
    hint: &str,
    add: impl FnOnce(&mut Ui) -> bool,
) -> bool {
    let mut changed = false;

    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let _ = row_label_bare(ui, title, 20.0);
        ui.add_space(6.0);

        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), 20.0),
            Layout::left_to_right(Align::Center),
            |ui| changed = add(ui),
        );
    });
    let _ = row.response.on_hover_text(hint);

    changed
}

#[cfg(windows)]
fn hotkey_field(
    ui: &mut Ui,
    combo: &str,
    recording: bool,
    waiting: &str,
    empty: &str,
) -> egui::Response {
    let width = ui.available_width().clamp(120.0, 190.0);
    let label = if recording {
        waiting.to_string()
    } else if combo.is_empty() {
        empty.to_string()
    } else {
        hotkey_label(combo)
    };
    let text_color = if recording {
        theme::accent()
    } else if combo.is_empty() {
        theme::dim()
    } else {
        theme::text()
    };
    let button = egui::Button::new(
        RichText::new(label)
            .font(theme::mono_medium(9.0))
            .color(text_color),
    )
    .fill(if recording {
        theme::alpha(theme::accent(), 10)
    } else {
        theme::field()
    })
    .stroke(egui::Stroke::new(
        1.0_f32,
        if recording {
            theme::accent()
        } else {
            theme::line()
        },
    ))
    .corner_radius(CornerRadius::same(7))
    .min_size(Vec2::new(width, 24.0));

    ui.add(button)
}

#[cfg(windows)]
fn hotkey_label(combo: &str) -> String {
    crate::hotkey::Hotkey::parse(combo)
        .map(|hotkey| hotkey.label())
        .unwrap_or_else(|_| combo.to_ascii_uppercase())
}

fn metric(ui: &mut Ui, label: &str, value: &str, unit: &str) {
    ui.label(
        RichText::new(label)
            .font(theme::mono_medium(7.0))
            .color(theme::muted()),
    );
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(
            RichText::new(value)
                .font(theme::mono_medium(16.0))
                .color(theme::text()),
        );
        ui.label(
            RichText::new(unit)
                .font(theme::mono(7.0))
                .color(theme::dim()),
        );
    });
}

fn group_digits(value: u64) -> String {
    let text = value.to_string();
    let mut grouped = String::with_capacity(text.len() + text.len() / 3);

    for (index, character) in text.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push('\u{00A0}');
        }
        grouped.push(character);
    }

    grouped.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scale(monitor: Vec2, window: [f32; 2]) -> f32 {
        auto_scale_for(monitor, 1.0, window)
    }

    #[test]
    fn automatic_scale_fits_the_screen() {
        let advanced = [460.0, 560.0];
        assert!((scale(Vec2::new(1920.0, 1080.0), advanced) - 1.0).abs() < 0.001);
        assert!((scale(Vec2::new(1366.0, 768.0), advanced) - 1.0).abs() < 0.001);
        assert!((scale(Vec2::new(1092.0, 614.0), advanced) - 0.925).abs() < 0.001);
        assert!((scale(Vec2::new(910.0, 512.0), advanced) - 0.75).abs() < 0.001);

        // The shorter Windows layout needs less zoom on the same screen.
        let compact = [460.0, 520.0];
        assert!((scale(Vec2::new(1092.0, 614.0), compact) - 0.996).abs() < 0.001);
        assert!((scale(Vec2::new(910.0, 512.0), compact) - 0.8).abs() < 0.001);
    }

    #[test]
    fn automatic_scale_is_stable_after_zooming() {
        let unscaled = Vec2::new(910.0, 512.0);
        let first = auto_scale_for(unscaled, 1.0, WINDOW_SIZE);
        assert!((MIN_AUTO_SCALE..=MAX_AUTO_SCALE).contains(&first));

        // At the applied zoom the viewport reports the monitor in smaller points.
        let reported = unscaled / first;
        assert!((auto_scale_for(reported, first, WINDOW_SIZE) - first).abs() < 0.001);
    }

    #[test]
    fn simple_window_uses_its_own_scale() {
        let monitor = Vec2::new(400.0, 330.0);
        assert!((auto_scale_for(monitor, 1.0, SIMPLE_WINDOW_SIZE) - 0.9).abs() < 0.001);
    }
}
