//! mqttc · Iced shell.
//!
//! All business state lives in `mqttc-core`'s `AppRuntime`; this module maps
//! widget messages onto runtime actions. The sidebar holds connection/settings
//! pages; subscription, auto-reply and preset panels live in the session tabs.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use iced::widget::canvas::{self, Canvas, LineCap, Path, Stroke};
use iced::widget::text::Wrapping;
use iced::widget::{
    button, checkbox, column, container, mouse_area, pick_list, responsive, row, scrollable, text,
    text_editor, text_input, tooltip, Space, Stack,
};
use iced::{Alignment, Length, Padding, Point, Rectangle, Size, Task};
use iced_widget::image::{Handle, Image};
use mqttc_core::cli::BenchOptions;
use mqttc_core::config::{FONT_SCALE_MAX, FONT_SCALE_MIN};
use mqttc_core::form::{FormState, PresetRow};
use mqttc_core::i18n::{is_en, t};
use mqttc_core::model::{default_client_id, ConnStatus, LogRecord, PayloadFormat, Skin};
use mqttc_core::payload;
use mqttc_core::runtime::AppRuntime;
use mqttc_core::theme::{to_hex, PaletteField, Rgb};

use crate::max_width;
use crate::style::{rgb, ButtonKind, Colors};

type El<'a> = iced::Element<'a, Msg>;

/// Sidebar width in logical pixels. Collapsing the sidebar shrinks the window
/// by the same amount so the page keeps its width instead of being stretched.
const SIDEBAR_WIDTH: f32 = 180.0;
/// Must match the window minimum in `main.rs`; the hidden-sidebar minimum
/// drops by the sidebar width so the window really can get narrower.
const MIN_WINDOW_WIDTH: f32 = 980.0;
const MIN_WINDOW_HEIGHT: f32 = 640.0;

/// How many of the newest log rows are put into the widget tree. The store
/// still keeps everything (export, filter and search see all rows); building
/// hundreds of bubbles on every 100 ms tick dominates CPU while streaming.
const LOG_RENDER_ROWS: usize = 80;

/// Widget id of the log list: scroll events identify a manual review, and
/// "回到最新" snaps this list back to the end.
const LOG_SCROLL_ID: &str = "log-scroll";

/// Widest share of the log list a bubble may reach. Bubbles shrink to their
/// content below this, so short messages stay narrow while long payloads get
/// a line length the whole panel can offer.
const LOG_BUBBLE_MAX: f32 = 0.85;

/// How long the toolbar confirms a right-click copy before clearing it.
const COPY_FLASH: Duration = Duration::from_millis(1600);

// ------------------------------------------------------------- font scale

thread_local! {
    /// Set once per frame from `PrefsState::font_scale`; every text size goes
    /// through [`fs`] so all pages share one scaled type scale.
    static FONT_SCALE: Cell<f32> = const { Cell::new(1.0) };
}

/// Scales a design-time font size (or checkbox box) by the user preference.
fn fs(size: f32) -> f32 {
    FONT_SCALE.with(|scale| size * scale.get())
}

fn set_font_scale(scale: f32) {
    FONT_SCALE.with(|cell| cell.set(scale.clamp(FONT_SCALE_MIN, FONT_SCALE_MAX)));
}

/// Runs a window action against the latest window (the shell only opens one).
fn window_task(
    action: impl Fn(iced::window::Id) -> Task<Msg> + Send + 'static,
) -> Task<Msg> {
    iced::window::latest().and_then(action)
}

// ------------------------------------------------------------ vector icons

/// Small stroke icons drawn with the canvas renderer. The bundled system
/// fonts have no dependable glyph set for caption/nav symbols, and a canvas
/// keeps the three window buttons optically identical.
#[derive(Clone, Copy)]
enum Icon {
    /// Caption controls.
    Minus,
    Square,
    Close,
    /// Sidebar navigation.
    Gear,
    Info,
    Plus,
    /// Title-strip sidebar toggle (panel outline with a divider).
    Panel,
    /// Title-strip always-on-top toggle (pushpin), diagonal when released.
    Pin,
    /// The same pushpin turned needle-down while the window stays on top; the
    /// changed silhouette reads at a glance next to the colour shift.
    PinDown,
    /// Preset-row reordering: one step up/down, or jump to the top/bottom.
    ArrowUp,
    ArrowDown,
    MoveTop,
    MoveBottom,
}

struct IconCanvas {
    icon: Icon,
    color: iced::Color,
    /// Stroke width in logical pixels (scaled by the caller if needed).
    stroke: f32,
    /// Half-extent of the glyph box; the icon stays centred in its cell.
    radius: f32,
    /// Widget size / 15, so nav icons follow the text scale while caption
    /// icons keep their native chrome size.
    scale: f32,
}

impl IconCanvas {
    fn new(icon: Icon, color: iced::Color) -> Self {
        let (stroke, radius) = match icon {
            Icon::Minus | Icon::Square | Icon::Close => (1.05, 5.4),
            Icon::Gear | Icon::Info => (1.35, 6.0),
            Icon::Plus => (1.25, 6.0),
            Icon::Panel => (1.15, 6.2),
            Icon::Pin => (1.05, 7.1),
            // Rotated to point straight down, the diagonal glyph's tip would
            // reach past the cell, so the turned variant is drawn smaller.
            Icon::PinDown => (1.05, 6.15),
            Icon::ArrowUp | Icon::ArrowDown => (1.3, 5.6),
            Icon::MoveTop | Icon::MoveBottom => (1.2, 6.0),
        };
        Self {
            icon,
            color,
            stroke,
            radius,
            scale: 1.0,
        }
    }

    fn widget<'a>(mut self, size: f32) -> iced::Element<'a, Msg> {
        self.scale = size / 15.0;
        Canvas::new(self)
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .into()
    }
}

impl canvas::Program<Msg> for IconCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let (w, h) = (bounds.width, bounds.height);
        let (cx, cy) = (w / 2.0, h / 2.0);
        let r = self.radius * self.scale;
        let stroke = Stroke::default()
            .with_color(self.color)
            .with_width(self.stroke * self.scale);
        let round = stroke.with_line_cap(LineCap::Round);
        let line = |x1: f32, y1: f32, x2: f32, y2: f32| {
            Path::line(Point::new(cx + x1, cy + y1), Point::new(cx + x2, cy + y2))
        };
        match self.icon {
            Icon::Minus => {
                frame.stroke(&line(-r, 0.0, r, 0.0), round);
            }
            Icon::Plus => {
                frame.stroke(&line(-r, 0.0, r, 0.0), round);
                frame.stroke(&line(0.0, -r, 0.0, r), round);
            }
            Icon::Square => {
                let side = r * 1.55;
                frame.stroke(
                    &Path::rectangle(
                        Point::new(cx - side / 2.0, cy - side / 2.0),
                        Size::new(side, side),
                    ),
                    stroke,
                );
            }
            Icon::Close => {
                let d = r * 0.82;
                frame.stroke(&line(-d, -d, d, d), round);
                frame.stroke(&line(-d, d, d, -d), round);
            }
            Icon::Gear => {
                frame.stroke(&Path::circle(Point::new(cx, cy), r * 0.72), stroke);
                frame.stroke(&Path::circle(Point::new(cx, cy), r * 0.30), stroke);
                // Eight short teeth around the ring read as a cog.
                for i in 0..8 {
                    let a = std::f32::consts::TAU * i as f32 / 8.0;
                    let (s, c) = a.sin_cos();
                    frame.stroke(
                        &line(c * r * 0.70, s * r * 0.70, c * r * 0.98, s * r * 0.98),
                        stroke,
                    );
                }
            }
            Icon::Info => {
                frame.stroke(&Path::circle(Point::new(cx, cy), r * 0.92), stroke);
                frame.stroke(&line(0.0, -r * 0.45, 0.0, r * 0.45), round);
                let dot = self.stroke * self.scale;
                frame.fill_rectangle(
                    Point::new(cx - dot * 0.6, cy - r * 0.72),
                    Size::new(dot * 1.2, dot * 1.2),
                    self.color,
                );
            }
            Icon::Panel => {
                let side = Size::new(r * 2.0, r * 1.7);
                let top_left = Point::new(cx - side.width / 2.0, cy - side.height / 2.0);
                frame.stroke(
                    &Path::rounded_rectangle(top_left, side, (2.5 * self.scale).into()),
                    stroke,
                );
                let x = cx - side.width / 2.0 + side.width * 0.36;
                let inset = self.scale;
                frame.stroke(
                    &Path::line(
                        Point::new(x, top_left.y + inset),
                        Point::new(x, top_left.y + side.height - inset),
                    ),
                    stroke,
                );
            }
            Icon::Pin | Icon::PinDown => {
                // Diagonal pushpin taken from the 24×24 outline glyph the
                // user picked: outer silhouette plus an inner cut-out,
                // filled with the even-odd rule so the cap stays hollow.
                // Pinned turns the glyph 45° counter-clockwise so the needle
                // points straight down, drops the cut-out so the whole shape
                // reads as a solid block, and switches to the accent colour.
                let angle = if matches!(self.icon, Icon::PinDown) {
                    -std::f32::consts::FRAC_PI_4
                } else {
                    0.0
                };
                let (sin, cos) = angle.sin_cos();
                let k = r / 12.0;
                let p = |x: f32, y: f32| {
                    let dx = x - 12.147;
                    let dy = y - 11.7735;
                    Point::new(
                        cx + (dx * cos - dy * sin) * k,
                        cy + (dx * sin + dy * cos) * k,
                    )
                };
                let pin = Path::new(|b| {
                    b.move_to(p(17.1218, 1.87023));
                    b.bezier_curve_to(p(15.7573, 0.505682), p(13.4779, 0.76575), p(12.4558, 2.40261));
                    b.line_to(p(9.61062, 6.95916));
                    b.bezier_curve_to(p(9.61033, 6.95965), p(9.60913, 6.96167), p(9.6038, 6.96549));
                    b.bezier_curve_to(p(9.59728, 6.97016), p(9.58336, 6.97822), p(9.56001, 6.9848));
                    b.bezier_curve_to(p(9.50899, 6.99916), p(9.44234, 6.99805), p(9.38281, 6.97599));
                    b.bezier_curve_to(p(8.41173, 6.61599), p(6.74483, 6.22052), p(5.01389, 6.87251));
                    b.bezier_curve_to(p(4.08132, 7.22378), p(3.61596, 8.03222), p(3.56525, 8.85243));
                    b.bezier_curve_to(p(3.51687, 9.63502), p(3.83293, 10.4395), p(4.41425, 11.0208));
                    b.line_to(p(7.94975, 14.5563));
                    b.line_to(p(1.26973, 21.2363));
                    b.bezier_curve_to(p(0.879206, 21.6269), p(0.879206, 22.26), p(1.26973, 22.6506));
                    b.bezier_curve_to(p(1.66025, 23.0411), p(2.29342, 23.0411), p(2.68394, 22.6506));
                    b.line_to(p(9.36397, 15.9705));
                    b.line_to(p(12.8995, 19.5061));
                    b.bezier_curve_to(p(13.4808, 20.0874), p(14.2853, 20.4035), p(15.0679, 20.3551));
                    b.bezier_curve_to(p(15.8881, 20.3044), p(16.6966, 19.839), p(17.0478, 18.9065));
                    b.bezier_curve_to(p(17.6998, 17.1755), p(17.3043, 15.5086), p(16.9444, 14.5375));
                    b.bezier_curve_to(p(16.9223, 14.478), p(16.9212, 14.4114), p(16.9355, 14.3603));
                    b.bezier_curve_to(p(16.9421, 14.337), p(16.9502, 14.3231), p(16.9549, 14.3165));
                    b.bezier_curve_to(p(16.9587, 14.3112), p(16.9606, 14.31), p(16.9611, 14.3098));
                    b.line_to(p(21.5177, 11.4645));
                    b.bezier_curve_to(p(23.1546, 10.4424), p(23.4147, 8.16307), p(22.0501, 6.79853));
                    b.close();
                    // Only the released glyph keeps the cap cut-out. Pinned
                    // fills the outer contour alone, so at 16 px the turned
                    // pin reads as a solid block instead of a hollow outline.
                    if matches!(self.icon, Icon::Pin) {
                        b.move_to(p(14.1523, 3.46191));
                        b.bezier_curve_to(p(14.493, 2.91629), p(15.2528, 2.8296), p(15.7076, 3.28445));
                        b.line_to(p(20.6359, 8.21274));
                        b.bezier_curve_to(p(21.0907, 8.66759), p(21.0041, 9.42737), p(20.4584, 9.76806));
                        b.line_to(p(15.9019, 12.6133));
                        b.bezier_curve_to(p(14.9572, 13.2032), p(14.7469, 14.3637), p(15.0691, 15.2327));
                        b.bezier_curve_to(p(15.3549, 16.0037), p(15.5829, 17.1217), p(15.1762, 18.2015));
                        b.bezier_curve_to(p(15.1484, 18.2752), p(15.1175, 18.3018), p(15.0985, 18.3149));
                        b.bezier_curve_to(p(15.0743, 18.3316), p(15.0266, 18.3538), p(14.9445, 18.3589));
                        b.bezier_curve_to(p(14.767, 18.3699), p(14.5135, 18.2916), p(14.3137, 18.0919));
                        b.line_to(p(5.82846, 9.6066));
                        b.bezier_curve_to(p(5.62872, 9.40686), p(5.55046, 9.15333), p(5.56144, 8.97583));
                        b.bezier_curve_to(p(5.56651, 8.8937), p(5.58877, 8.84605), p(5.60548, 8.82181));
                        b.bezier_curve_to(p(5.61855, 8.80285), p(5.64516, 8.7719), p(5.71886, 8.74414));
                        b.bezier_curve_to(p(6.79869, 8.33741), p(7.91661, 8.56545), p(8.68762, 8.85128));
                        b.bezier_curve_to(p(9.55668, 9.17345), p(10.7171, 8.96318), p(11.3071, 8.01845));
                        b.line_to(p(14.1523, 3.46191));
                        b.close();
                    }
                });
                frame.fill(
                    &pin,
                    canvas::Fill {
                        style: canvas::Style::Solid(self.color),
                        rule: canvas::fill::Rule::EvenOdd,
                    },
                );
            }
            Icon::ArrowUp | Icon::ArrowDown | Icon::MoveTop | Icon::MoveBottom => {
                let up = matches!(self.icon, Icon::ArrowUp | Icon::MoveTop);
                let jump = matches!(self.icon, Icon::MoveTop | Icon::MoveBottom);
                let half_w = r * 0.58;
                let half_h = r * 0.34;
                // One chevron with its point offset `y` from the glyph centre.
                let chevron = |y: f32| {
                    Path::new(|b| {
                        let (tip, tail) = if up {
                            (y - half_h, y + half_h)
                        } else {
                            (y + half_h, y - half_h)
                        };
                        b.move_to(Point::new(cx - half_w, cy + tail));
                        b.line_to(Point::new(cx, cy + tip));
                        b.line_to(Point::new(cx + half_w, cy + tail));
                    })
                };
                if jump {
                    // Two stacked chevrons plus a bar at the destination edge:
                    // "jump all the way up / down" instead of one step.
                    let bar = if up { -r * 0.70 } else { r * 0.70 };
                    frame.stroke(&line(-r * 0.70, bar, r * 0.70, bar), round);
                    let (near, far) = if up {
                        (-r * 0.20, r * 0.22)
                    } else {
                        (r * 0.20, -r * 0.22)
                    };
                    frame.stroke(&chevron(near), round);
                    frame.stroke(&chevron(far), round);
                } else {
                    frame.stroke(&chevron(0.0), round);
                }
            }
        }
        vec![frame.into_geometry()]
    }
}

// ---------------------------------------------------------------- fields

/// Every single-line string in `FormState` that a text input can edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Host,
    Port,
    ClientId,
    Username,
    Password,
    KeepAlive,
    Timeout,
    SubTopic,
    WillTopic,
    WillPayload,
    PubTopic,
    TimedInterval,
    PubProps,
    PubExpiry,
    Ca,
    Cert,
    Key,
}

impl Field {
    fn get(self, f: &FormState) -> &str {
        match self {
            Self::Name => &f.name,
            Self::Host => &f.host,
            Self::Port => &f.port,
            Self::ClientId => &f.client_id,
            Self::Username => &f.username,
            Self::Password => &f.password,
            Self::KeepAlive => &f.keep_alive,
            Self::Timeout => &f.connect_timeout,
            Self::SubTopic => &f.sub_topic,
            Self::WillTopic => &f.will_topic,
            Self::WillPayload => &f.will_payload,
            Self::PubTopic => &f.pub_topic,
            Self::TimedInterval => &f.timed_interval,
            Self::PubProps => &f.pub_props,
            Self::PubExpiry => &f.pub_expiry,
            Self::Ca => &f.ssl_ca,
            Self::Cert => &f.ssl_cert,
            Self::Key => &f.ssl_key,
        }
    }

    fn set(self, f: &mut FormState, value: String) {
        match self {
            Self::Name => f.name = value,
            Self::Host => f.host = value,
            Self::Port => f.port = value,
            Self::ClientId => f.client_id = value,
            Self::Username => f.username = value,
            Self::Password => f.password = value,
            Self::KeepAlive => f.keep_alive = value,
            Self::Timeout => f.connect_timeout = value,
            Self::SubTopic => f.sub_topic = value,
            Self::WillTopic => f.will_topic = value,
            Self::WillPayload => f.will_payload = value,
            Self::PubTopic => f.pub_topic = value,
            Self::TimedInterval => f.timed_interval = value,
            Self::PubProps => f.pub_props = value,
            Self::PubExpiry => f.pub_expiry = value,
            Self::Ca => f.ssl_ca = value,
            Self::Cert => f.ssl_cert = value,
            Self::Key => f.ssl_key = value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Num {
    Protocol,
    SubQos,
    PubQos,
    PubFormat,
    WillQos,
    Echo,
}

impl Num {
    fn set(self, f: &mut FormState, value: i32) {
        match self {
            Self::Protocol => f.protocol = value,
            Self::SubQos => f.sub_qos = value,
            Self::PubQos => f.pub_qos = value,
            Self::PubFormat => f.pub_format = value,
            Self::WillQos => f.will_qos = value,
            Self::Echo => f.auto_reply_echo = value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    CleanSession,
    AutoReconnect,
    Ssl,
    SslVerify,
    WillRetain,
    PubRetain,
    SubNoLocal,
    SubRap,
}

impl Flag {
    fn set(self, f: &mut FormState, value: bool) {
        match self {
            Self::CleanSession => f.clean_session = value,
            Self::AutoReconnect => f.auto_reconnect = value,
            Self::Ssl => f.ssl = value,
            Self::SslVerify => f.ssl_verify = value,
            Self::WillRetain => f.will_retain = value,
            Self::PubRetain => f.pub_retain = value,
            Self::SubNoLocal => f.sub_no_local = value,
            Self::SubRap => f.sub_retain_as_published = value,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Select(String),
    New,
    /// Asks for confirmation instead of deleting immediately.
    Delete(String),
    ConfirmDelete,
    CancelDelete,
    Page(i32),
    SessionTab(SessionTab),
    Connect,
    Disconnect,
    Subscribe,
    Unsubscribe(usize),
    FlipSubOption(usize, bool),
    ToggleSub(String),
    /// Cancel the subscription-row topic filter from the log toolbar.
    ClearSubFilter,
    Publish,
    /// Ctrl+Enter from anywhere, including the payload editor. The editor also
    /// turns Enter into a newline, so the shortcut restores the pre-edit text.
    PublishShortcut,
    PublishPreset(usize),
    /// Fold the remaining preset buttons open below the publish form.
    TogglePresetPalette,
    /// Reorder one preset row: to the top, one step up, one step down, bottom.
    PresetMoveTop(usize),
    PresetMoveUp(usize),
    PresetMoveDown(usize),
    PresetMoveBottom(usize),
    Format,
    Timed,
    GenId,
    SaveLogs,
    SaveVisibleLogs,
    TogglePause,
    LogLimit(i32),
    OpenLog(u64),
    CloseLog,
    /// Detail-page format picker (local to the open row, not persisted).
    DetailFormat(i32),
    /// Widen the rendered tail window by one page (older rows stay in the store).
    ShowOlderLogs,
    ClearLogs,
    /// Render the full payload only when the user actually copies it.
    CopyLog(u64, i32),
    /// Right-click on a bubble's topic line: copy just the topic.
    CopyTopic(u64),
    /// Right-click on a bubble's payload: copy the rendered body.
    CopyBody(u64),
    CopyRow,
    /// Copy the open detail row as one exported log line.
    CopyDetailRow,
    FocusSearch,
    DroppedFile(PathBuf),
    Skin(i32),
    /// Custom palette slot edited: raw text is kept while the user types.
    CustomColor(PaletteField, String),
    ResetCustomColor(PaletteField),
    CustomBase(i32),
    RefillCustom,
    ToggleMoreColors,
    /// Open (or close with `None`) the preset chip panel for one slot.
    SwatchPanel(Option<PaletteField>),
    PickSwatch(Rgb),
    Language(i32),
    /// Global text-size multiplier chosen in Settings → Appearance.
    FontScale(f32),
    /// Start moving the undecorated window from the title strip.
    DragWindow,
    /// Pin / unpin the window above other applications.
    ToggleAlwaysOnTop,
    MinimizeWindow,
    ToggleMaximize,
    /// Result of a custom maximize: the pre-maximize rect to restore later.
    #[allow(dead_code)]
    Maximized(Option<[i32; 4]>),
    CloseWindow,
    /// Start a native resize from one of the invisible window edges.
    ResizeWindow(iced::window::Direction),
    /// The window exists and platform chrome (rounded corners) has been applied.
    WindowReady,
    /// Fold the connection list away; the window shrinks by its width.
    ToggleSidebar,
    /// Open the project page in the default browser.
    OpenRepo,
    AddRule,
    DelRule(usize),
    MoveRule(usize, isize),
    RuleEnabled(usize, bool),
    RuleRetain(usize, bool),
    RuleNum(usize, i32),
    RuleFormat(usize, i32),
    RuleText(usize, RuleTextField, String),
    RuleInterval(usize, String),
    ResetRuleStats,
    AddPreset,
    DelPreset(usize),
    PresetRetain(usize, bool),
    PresetNum(usize, i32),
    PresetFormat(usize, i32),
    PresetText(usize, PresetTextField, String),
    LogFilter(String),
    LogKind(i32),
    LogFormat(i32),
    AutoScroll(bool),
    /// Vertical offset reported by the log list. Scrolling away from the end
    /// switches auto-scroll off so the stream stops moving the view.
    LogScrolled(f32),
    ResetCounts,
    Field(Field, String),
    Num(Num, i32),
    Flag(Flag, bool),
    PubEdit(text_editor::Action),
    Tick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleTextField {
    MatchTopic,
    Contains,
    ReplyTopic,
    ReplyPayload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetTextField {
    Name,
    Topic,
    Payload,
}

/// Connection-scoped panels shown below the session header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTab {
    Logs,
    Subs,
    Rules,
    Presets,
}

// ---------------------------------------------------------------- options

/// A pick-list option. Labels are built per frame so switching language
/// re-localizes every dropdown instead of freezing the startup strings.
#[derive(Clone, PartialEq, Eq)]
struct Opt {
    value: i32,
    label: String,
}

impl fmt::Display for Opt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

fn opt(value: i32, label: impl Into<String>) -> Opt {
    Opt {
        value,
        label: label.into(),
    }
}

fn qos_opts() -> Vec<Opt> {
    vec![opt(0, "QoS 0"), opt(1, "QoS 1"), opt(2, "QoS 2")]
}

fn format_opts() -> Vec<Opt> {
    PayloadFormat::ALL
        .iter()
        .map(|f| {
            let label = match f {
                PayloadFormat::Text => "Text",
                PayloadFormat::Json => "JSON",
                PayloadFormat::Base64 => "Base64",
                PayloadFormat::Hex => "Hex",
            };
            opt(f.to_index(), label)
        })
        .collect()
}

fn proto_opts() -> Vec<Opt> {
    vec![opt(0, "3.1.1"), opt(1, "5.0")]
}

fn skin_opts() -> Vec<Opt> {
    Skin::ALL
        .iter()
        .map(|s| {
            let label = match s {
                Skin::System => t("跟随系统", "System"),
                Skin::Black => t("黑", "Black"),
                Skin::White => t("白", "White"),
                Skin::Blue => t("蓝", "Blue"),
                Skin::Green => t("绿", "Green"),
                Skin::Purple => t("紫", "Purple"),
                Skin::Custom => t("自定义", "Custom"),
            };
            opt(s.to_index(), label)
        })
        .collect()
}

/// Native language names stay untranslated on purpose.
fn lang_opts() -> Vec<Opt> {
    vec![opt(0, "中文"), opt(1, "English")]
}

/// Text-size presets shown in Settings → Appearance. The multiplier applies
/// to every text size and is persisted as `AppConfig::font_scale`.
const FONT_CHOICES: [(f32, &str, &str); 4] = [
    (0.9, "小", "Small"),
    (1.0, "标准", "Default"),
    (1.15, "大", "Large"),
    (1.3, "特大", "Extra large"),
];

fn font_opts() -> Vec<Opt> {
    FONT_CHOICES
        .iter()
        .enumerate()
        .map(|(index, (_, zh, en))| opt(index as i32, t(zh, en)))
        .collect()
}

/// Nearest preset for a persisted scale (hand-edited configs may hold any value).
fn font_index(scale: f32) -> i32 {
    let mut best = 1i32;
    let mut distance = f32::MAX;
    for (index, (value, _, _)) in FONT_CHOICES.iter().enumerate() {
        let delta = (value - scale).abs();
        if delta < distance {
            distance = delta;
            best = index as i32;
        }
    }
    best
}

/// Concrete skins only: these seed and back-fill the custom palette.
fn base_skin_opts() -> Vec<Opt> {
    skin_opts()
        .into_iter()
        .filter(|o| o.value != Skin::System.to_index() && o.value != Skin::Custom.to_index())
        .collect()
}

/// Preset chips for the custom palette: neutrals first, then warm-to-cool
/// hues, ending with the soft bubble fills used by the default skins.
const SWATCHES: [Rgb; 48] = [
    (0x16, 0x18, 0x1d),
    (0x1e, 0x22, 0x29),
    (0x24, 0x28, 0x30),
    (0x2b, 0x32, 0x40),
    (0x3a, 0x3f, 0x49),
    (0x52, 0x58, 0x5f),
    (0x6d, 0x73, 0x7c),
    (0x8b, 0x91, 0x9a),
    (0xb8, 0xc0, 0xcc),
    (0xd7, 0xe6, 0xf8),
    (0xe7, 0xec, 0xf3),
    (0xff, 0xff, 0xff),
    (0xe8, 0x5d, 0x5d),
    (0xc6, 0x28, 0x28),
    (0x7a, 0x1f, 0x1f),
    (0xff, 0x8a, 0x80),
    (0xff, 0x9f, 0x43),
    (0xe6, 0x7e, 0x22),
    (0xf4, 0xd3, 0x5e),
    (0xff, 0xf3, 0xb0),
    (0x1f, 0x8a, 0x4c),
    (0x4e, 0xcf, 0x7a),
    (0x0c, 0x6b, 0x45),
    (0x13, 0x4e, 0x40),
    (0x58, 0xef, 0xc8),
    (0x35, 0xd4, 0xae),
    (0x22, 0xb8, 0xcf),
    (0x72, 0xe4, 0xf6),
    (0x1d, 0x5f, 0xbf),
    (0x2f, 0x6f, 0xd0),
    (0x5a, 0xa7, 0xff),
    (0x7a, 0xb8, 0xff),
    (0x12, 0x3a, 0x68),
    (0x0d, 0x2d, 0x5c),
    (0x07, 0x20, 0x48),
    (0xd5, 0xe4, 0xf7),
    (0x46, 0x25, 0x73),
    (0x75, 0x47, 0xb5),
    (0xb0, 0x96, 0xff),
    (0xd9, 0xc2, 0xf8),
    (0xf0, 0x65, 0x95),
    (0xff, 0x8f, 0xab),
    (0x32, 0x19, 0x56),
    (0xf8, 0xf2, 0xff),
    (0xe4, 0xf6, 0xea),
    (0xe4, 0xee, 0xfc),
    (0xfd, 0xe8, 0xe8),
    (0xf3, 0xf6, 0xfa),
];

fn kind_opts() -> Vec<Opt> {
    vec![
        opt(0, t("全部", "All")),
        opt(1, t("收", "Recv")),
        opt(2, t("发", "Send")),
        opt(3, t("系统", "System")),
        opt(4, t("错误", "Errors")),
    ]
}

fn echo_opts() -> Vec<Opt> {
    vec![
        opt(0, t("显示", "Show")),
        opt(1, t("隐藏", "Hide")),
        opt(2, t("跳过", "Skip")),
    ]
}

fn log_limit_opts() -> Vec<Opt> {
    vec![opt(200, "200"), opt(500, "500"), opt(1000, "1000"), opt(2000, "2000")]
}

fn pick<'a, F>(c: &Colors, options: &[Opt], current: i32, on: F) -> El<'a>
where
    F: Fn(i32) -> Msg + 'a,
{
    let selected = options
        .iter()
        .find(|o| o.value == current)
        .cloned();
    pick_list(options.to_vec(), selected, move |o: Opt| on(o.value))
        .text_size(fs(13.0))
        .padding([6, 8])
        .style(c.pick_list_style())
        .into()
}

// ---------------------------------------------------------------- app

struct Bench {
    options: BenchOptions,
    quit_at: Option<Instant>,
    timed_started: bool,
    finished: bool,
    announced: bool,
}

/// Which right-click copy a toolbar notice is confirming.
#[derive(Clone, Copy, PartialEq)]
enum CopyFlash {
    Topic,
    Payload,
    Message,
}

/// What the payload editor's last Enter edit changed, so the Ctrl+Enter
/// shortcut can undo it before publishing. The focused editor sees the
/// keystroke first, so its newline must be rolled back.
struct EnterUndo {
    at: Instant,
    text: String,
    /// Enter replaced a selection; a plain Backspace cannot restore it.
    replaced_selection: bool,
}

pub struct App {
    rt: AppRuntime,
    page: i32,
    session_tab: SessionTab,
    editor: text_editor::Content,
    started: Instant,
    first_frame_ms: Cell<Option<f64>>,
    bench: Option<Bench>,
    /// UI-side debounce; set on every edit so we only tick briefly after typing.
    dirty_at: Option<Instant>,
    /// Inline previews keyed by `LogRecord::seq` (format tag, rendered text).
    /// Cleared on connection switch / clear / format change.
    preview_cache: RefCell<HashMap<u64, (i32, String)>>,
    /// Undo record for the payload editor's last Enter edit; see [`EnterUndo`].
    editor_enter: Option<EnterUndo>,
    /// Expanded row shown instead of the log list. Cleared on tab / connection change.
    detail_seq: Option<u64>,
    /// Format index for the open detail page; reseeded from the global log
    /// format each time a row is opened.
    detail_format: i32,
    /// Number of newest visible rows currently rendered (see [`LOG_RENDER_ROWS`]).
    log_window: usize,
    /// Rows pinned on screen while auto-scroll is off. Copied out of the store
    /// (payloads are shared `Arc`s, so only the small row structs are cloned)
    /// because the ring buffer drops old rows while the stream keeps running;
    /// anchoring by seq alone made the view snap back to the newest row once
    /// the anchor had been evicted. New rows wait below until "回到最新".
    log_freeze: Option<Vec<LogRecord>>,
    /// Last vertical offset of the log list. With `anchor_bottom()` the view
    /// sits at 0 when it follows the stream, so a growing offset means the
    /// user scrolled up (wheel or scrollbar) and auto-scroll must stop.
    log_scroll_y: Option<f32>,
    /// Transient toolbar confirmation for a right-click copy, with its expiry.
    copy_flash: Option<(CopyFlash, Instant)>,
    /// Connection id waiting for a delete confirmation.
    pending_delete: Option<String>,
    /// “更多颜色” folded open in the custom-palette editor.
    more_colors: bool,
    /// Slot whose preset chip panel is open.
    swatch_field: Option<PaletteField>,
    /// “More presets” folded open below the publish form.
    presets_open: bool,
    /// App mark shown top-left; built once so the renderer can cache the decode.
    icon: Option<Handle>,
    /// Connection list / page links column in the content panel.
    sidebar_visible: bool,
    /// Custom maximize state (Windows only); native toggle_maximize overshoots
    /// the monitor by the DPI factor on frameless windows.
    #[cfg_attr(not(windows), allow(dead_code))]
    maximized: bool,
    /// Outer window rect in physical pixels captured before maximizing.
    #[cfg_attr(not(windows), allow(dead_code))]
    restore_rect: Option<[i32; 4]>,
}

impl App {
    pub fn new() -> Self {
        let options = mqttc_core::cli::parse(std::env::args().skip(1));
        let rt = AppRuntime::load(&options, None);
        Self::with_runtime(rt, options)
    }

    /// Boot hook: iced runs this task only after the window has opened, which
    /// lets us apply native chrome that has no iced setting (Windows 11 rounds
    /// the frameless window like any other app).
    pub fn boot() -> (Self, Task<Msg>) {
        let app = Self::new();
        let level = if app.rt.prefs.always_on_top {
            iced::window::Level::AlwaysOnTop
        } else {
            iced::window::Level::Normal
        };
        (
            app,
            Task::batch([
                window_task(|id| {
                    iced::window::run(id, |window| {
                        #[cfg(windows)]
                        win32::round_corners(window);
                        Msg::WindowReady
                    })
                }),
                window_task(move |id| iced::window::set_level::<Msg>(id, level)),
            ]),
        )
    }

    /// Tests run from `target/debug/deps`, where the portable `mqttc.json` may
    /// hold filters or settings left by an earlier run. Build from defaults so
    /// assertions never depend on that file.
    #[cfg(test)]
    fn for_test() -> Self {
        // The i18n switch is process-wide and runtime construction applies the
        // config language; serialize against tests that pin a language globally.
        let _lang = tests::LANG_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        Self::with_runtime(AppRuntime::new(None, None, None), BenchOptions::default())
    }

    fn with_runtime(rt: AppRuntime, options: BenchOptions) -> Self {
        let editor = text_editor::Content::with_text(&rt.form.pub_payload);
        let started = Instant::now();
        let bench = options.bench.then(|| Bench {
            quit_at: options
                .quit_after
                .map(|secs| started + Duration::from_secs(secs)),
            options,
            timed_started: false,
            finished: false,
            announced: false,
        });
        Self {
            rt,
            page: 0,
            session_tab: SessionTab::Logs,
            editor,
            started,
            first_frame_ms: Cell::new(None),
            bench,
            dirty_at: None,
            preview_cache: RefCell::new(HashMap::new()),
            editor_enter: None,
            detail_seq: None,
            detail_format: 0,
            log_window: LOG_RENDER_ROWS,
            log_freeze: None,
            log_scroll_y: None,
            copy_flash: None,
            pending_delete: None,
            more_colors: false,
            swatch_field: None,
            presets_open: false,
            icon: crate::icon_rgba()
                .map(|(width, height, pixels)| Handle::from_rgba(width, height, pixels)),
            sidebar_visible: true,
            maximized: false,
            restore_rect: None,
        }
    }

    pub fn theme(&self) -> iced::Theme {
        Colors::from_palette(self.rt.palette()).theme()
    }

    pub fn subscription(&self) -> iced::Subscription<Msg> {
        let busy = self.bench.is_some()
            || self.rt.timed_running()
            || self.rt.reconnect_pending()
            || self
                .rt
                .active_id()
                .map(|id| self.rt.status_of(&id) != ConnStatus::Disconnected)
                .unwrap_or(false);
        let every = if busy || self.dirty_at.is_some() || self.copy_flash.is_some() {
            Some(Duration::from_millis(100))
        } else if self.rt.prefs.skin_choice == 0 {
            Some(Duration::from_secs(1))
        } else {
            None
        };
        let timer = match every {
            Some(period) => iced::time::every(period).map(|_| Msg::Tick),
            None => iced::Subscription::none(),
        };
        iced::Subscription::batch([timer, iced::event::listen_with(event_message)])
    }

    fn touched(&mut self) -> Task<Msg> {
        self.dirty_at = Some(Instant::now() + Duration::from_millis(420));
        self.rt.mark_dirty();
        Task::none()
    }

    /// Flush a preset reorder from the draft form into the saved connection.
    fn persist_presets(&mut self) {
        self.rt.apply_form();
        self.rt.persist();
    }

    fn reload_form(&mut self) {
        self.rt.apply_form();
        self.editor = text_editor::Content::with_text(&self.rt.form.pub_payload);
    }

    /// Back to one page of rows. With auto-scroll off, pin the newest page so
    /// a filter or session switch does not slide with the incoming stream.
    fn reset_log_window(&mut self) {
        self.log_window = LOG_RENDER_ROWS;
        if self.rt.prefs.auto_scroll {
            self.log_freeze = None;
        } else {
            self.freeze_logs(true);
        }
    }

    /// Copy a page of rows out of the store so later ring-buffer eviction
    /// cannot move or drop what the user is reading. `tail` picks the newest
    /// rows (explicitly turning auto-scroll off) instead of the first page
    /// (a session that starts with auto-scroll already off).
    fn freeze_logs(&mut self, tail: bool) {
        let logs = self.rt.visible_logs();
        let rows: Vec<LogRecord> = if tail {
            let skip = logs.len().saturating_sub(self.log_window);
            logs[skip..].iter().map(|log| (*log).clone()).collect()
        } else {
            logs.iter()
                .take(LOG_RENDER_ROWS)
                .map(|log| (*log).clone())
                .collect()
        };
        self.log_freeze = (!rows.is_empty()).then_some(rows);
    }

    /// Rows shown in the frozen window are no longer guaranteed to be in the
    /// store, so detail and copy look them up here as a fallback.
    fn row_by_seq(&self, seq: u64) -> Option<&LogRecord> {
        self.rt.active_log(seq).or_else(|| {
            self.log_freeze
                .as_ref()?
                .iter()
                .find(|log| log.seq == seq)
        })
    }

    /// One row in the same shape as the exported log, for the clipboard.
    fn row_text(&self, log: &LogRecord) -> String {
        self.row_text_fmt(log, PayloadFormat::from_index(self.rt.prefs.log_format))
    }

    fn row_text_fmt(&self, log: &LogRecord, fmt: PayloadFormat) -> String {
        mqttc_core::export::log_line(log, fmt)
    }

    /// Ctrl+Shift+C: the open detail row, else the newest visible row.
    fn current_row_text(&self) -> Option<String> {
        if let Some(seq) = self.detail_seq {
            if let Some(log) = self.row_by_seq(seq) {
                return Some(
                    self.row_text_fmt(log, PayloadFormat::from_index(self.detail_format)),
                );
            }
        }
        self.rt
            .visible_logs()
            .last()
            .map(|log| self.row_text(log))
            .or_else(|| self.rt.last_log().map(|log| self.row_text(&log)))
    }

    /// Dropped certificate file → first empty TLS slot (then CA), enabling SSL.
    fn fill_cert_path(&mut self, path: &std::path::Path) -> bool {
        let value = path.to_string_lossy().into_owned();
        if value.trim().is_empty() {
            return false;
        }
        let lower = value.to_ascii_lowercase();
        let f = &mut self.rt.form;
        if lower.ends_with(".key") {
            f.ssl_key = value;
        } else if f.ssl_ca.trim().is_empty() {
            f.ssl_ca = value;
        } else if f.ssl_cert.trim().is_empty() {
            f.ssl_cert = value;
        } else if f.ssl_key.trim().is_empty() {
            f.ssl_key = value;
        } else {
            f.ssl_ca = value;
        }
        f.ssl = true;
        if let Some(id) = self.rt.active_id() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.rt.push_log(
                &id,
                "SYS",
                "",
                t(
                    &format!("已填入证书文件: {name}"),
                    &format!("Certificate file added: {name}"),
                )
                .into_bytes(),
            );
        }
        true
    }

    pub fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Tick => return self.on_tick(),
            Msg::Select(id) => {
                // Clicking the already-active row still opens its session:
                // the sidebar is now the only entry point.
                let changed = self.rt.select_connection(&id);
                if changed || self.rt.active_id().as_deref() == Some(id.as_str()) {
                    self.page = 1;
                    self.session_tab = SessionTab::Logs;
                    self.detail_seq = None;
                    self.reset_log_window();
                    self.presets_open = false;
                    self.preview_cache.borrow_mut().clear();
                    if changed {
                        self.reload_form();
                    }
                }
            }
            Msg::New => {
                self.rt.new_connection();
                self.page = 0;
                self.detail_seq = None;
                self.presets_open = false;
                self.preview_cache.borrow_mut().clear();
                self.rt.persist();
                self.reload_form();
            }
            Msg::Delete(id) => {
                self.pending_delete = Some(id);
            }
            Msg::ConfirmDelete => {
                if let Some(id) = self.pending_delete.take() {
                    self.rt.delete_connection(&id);
                    self.page = 0;
                    self.detail_seq = None;
                    self.presets_open = false;
                    self.preview_cache.borrow_mut().clear();
                    self.rt.persist();
                    self.reload_form();
                }
            }
            Msg::CancelDelete => self.pending_delete = None,
            Msg::CopyLog(seq, fmt) => {
                let fmt = PayloadFormat::from_index(fmt);
                if let Some(text) = self
                    .row_by_seq(seq)
                    .map(|log| self.log_body_text_fmt(log, fmt))
                {
                    self.copy_flash = Some((CopyFlash::Payload, Instant::now() + COPY_FLASH));
                    return iced::clipboard::write(text);
                }
            }
            Msg::CopyTopic(seq) => {
                if let Some(log) = self.row_by_seq(seq) {
                    let topic = payload::sanitize_display_text(&log.topic).into_owned();
                    if !topic.is_empty() {
                        self.copy_flash = Some((CopyFlash::Topic, Instant::now() + COPY_FLASH));
                        return iced::clipboard::write(topic);
                    }
                }
            }
            Msg::CopyBody(seq) => {
                if let Some(text) = self.row_by_seq(seq).map(|log| self.log_body_text(log)) {
                    self.copy_flash = Some((CopyFlash::Payload, Instant::now() + COPY_FLASH));
                    return iced::clipboard::write(text);
                }
            }
            Msg::CopyRow => {
                if let Some(text) = self.current_row_text() {
                    return iced::clipboard::write(text);
                }
            }
            Msg::CopyDetailRow => {
                let text = self.detail_seq.and_then(|seq| {
                    let log = self.row_by_seq(seq)?;
                    Some(self.row_text_fmt(log, PayloadFormat::from_index(self.detail_format)))
                });
                if let Some(text) = text {
                    self.copy_flash = Some((CopyFlash::Message, Instant::now() + COPY_FLASH));
                    return iced::clipboard::write(text);
                }
            }
            Msg::FocusSearch => {
                self.page = 1;
                self.session_tab = SessionTab::Logs;
                self.detail_seq = None;
                return iced::widget::operation::focus(iced::widget::Id::new("log-search"));
            }
            Msg::DroppedFile(path) => {
                if self.fill_cert_path(&path) {
                    return self.touched();
                }
            }
            Msg::Page(p) => {
                if p == 1 {
                    self.reload_form();
                    self.rt.persist();
                }
                self.page = p;
            }
            Msg::SessionTab(tab) => {
                self.session_tab = tab;
                self.detail_seq = None;
            }
            Msg::Connect => {
                self.rt.apply_form();
                if self.rt.connect() {
                    self.page = 1;
                    self.session_tab = SessionTab::Logs;
                    self.detail_seq = None;
                }
            }
            Msg::Disconnect => self.rt.disconnect(),
            Msg::Subscribe => {
                self.rt.apply_form();
                self.rt.subscribe();
            }
            Msg::Unsubscribe(index) => self.rt.unsubscribe(index),
            Msg::FlipSubOption(index, no_local) => {
                self.rt.flip_sub_option(index, no_local);
            }
            Msg::ToggleSub(topic) => self.rt.toggle_sub_filter(&topic),
            Msg::ClearSubFilter => self.rt.clear_sub_filter(),
            Msg::Publish => {
                // The form is hidden while the preset palette is open; a
                // publish must not send an invisible topic/payload.
                if !self.presets_open {
                    self.rt.apply_form();
                    self.rt.publish_form();
                }
            }
            Msg::PublishShortcut => {
                if !self.presets_open {
                    // The focused editor handles the same keystroke first and
                    // turns it into a newline; undo that edit before publishing.
                    if let Some(undo) = self.editor_enter.take() {
                        if undo.at.elapsed() < Duration::from_millis(200) {
                            if undo.replaced_selection {
                                self.editor =
                                    text_editor::Content::with_text(&undo.text);
                                self.rt.form.pub_payload = undo.text;
                            } else {
                                self.editor.perform(text_editor::Action::Edit(
                                    text_editor::Edit::Backspace,
                                ));
                                self.rt.form.pub_payload = self.editor.text();
                            }
                        }
                    }
                    self.rt.apply_form();
                    self.rt.publish_form();
                }
            }
            Msg::PublishPreset(index) => {
                self.rt.apply_form();
                self.rt.publish_preset(index);
            }
            Msg::TogglePresetPalette => {
                if !self.rt.form.presets.is_empty() {
                    self.presets_open = !self.presets_open;
                }
            }
            Msg::PresetMoveTop(index) => {
                if self.rt.form.move_preset_to(index, 0) {
                    self.persist_presets();
                }
            }
            Msg::PresetMoveUp(index) => {
                if self.rt.form.move_preset(index, -1) {
                    self.persist_presets();
                }
            }
            Msg::PresetMoveDown(index) => {
                if self.rt.form.move_preset(index, 1) {
                    self.persist_presets();
                }
            }
            Msg::PresetMoveBottom(index) => {
                let last = self.rt.form.presets.len().saturating_sub(1);
                if self.rt.form.move_preset_to(index, last) {
                    self.persist_presets();
                }
            }
            Msg::Format => {
                let _ = self.rt.format_payload();
                self.editor = text_editor::Content::with_text(&self.rt.form.pub_payload);
            }
            Msg::Timed => self.rt.toggle_timed(),
            Msg::GenId => {
                self.rt.form.client_id = default_client_id();
                return self.touched();
            }
            Msg::SaveLogs => {
                let _ = self.rt.save_logs();
            }
            Msg::SaveVisibleLogs => {
                let _ = self.rt.save_visible_logs();
            }
            Msg::TogglePause => {
                self.rt.toggle_logs_pause();
            }
            Msg::LogLimit(value) => {
                if value > 0 {
                    self.rt.set_log_limit(value as usize);
                    self.preview_cache.borrow_mut().clear();
                }
            }
            Msg::OpenLog(seq) => {
                self.detail_seq = Some(seq);
                self.detail_format = self.rt.prefs.log_format;
            }
            Msg::CloseLog => self.detail_seq = None,
            Msg::DetailFormat(value) => self.detail_format = value,
            Msg::ShowOlderLogs => {
                if let Some(rows) = self.log_freeze.as_mut() {
                    if let Some(first) = rows.first().map(|log| log.seq) {
                        let mut older: Vec<LogRecord> = self
                            .rt
                            .visible_logs()
                            .iter()
                            .filter(|log| log.seq < first)
                            .map(|log| (*log).clone())
                            .collect();
                        let drop_count = older.len().saturating_sub(LOG_RENDER_ROWS);
                        older.drain(..drop_count);
                        if !older.is_empty() {
                            older.extend(rows.drain(..));
                            *rows = older;
                        }
                    }
                } else {
                    self.log_window = self.log_window.saturating_add(LOG_RENDER_ROWS);
                }
            }
            Msg::ClearLogs => {
                self.detail_seq = None;
                self.log_window = LOG_RENDER_ROWS;
                self.log_freeze = None;
                self.preview_cache.borrow_mut().clear();
                self.rt.clear_logs();
            }
            Msg::Skin(choice) => {
                self.rt.set_skin_choice(choice);
                self.swatch_field = None;
            }
            Msg::CustomColor(field, value) => {
                self.rt.set_custom_color(field, value);
                return self.touched();
            }
            Msg::ResetCustomColor(field) => {
                self.rt.reset_custom_color(field);
                return self.touched();
            }
            Msg::CustomBase(index) => {
                self.rt.set_custom_base(index);
                return self.touched();
            }
            Msg::RefillCustom => {
                self.rt.refill_custom_palette();
                self.swatch_field = None;
            }
            Msg::ToggleMoreColors => self.more_colors = !self.more_colors,
            Msg::SwatchPanel(field) => {
                self.swatch_field = if self.swatch_field == field {
                    None
                } else {
                    field
                };
            }
            Msg::PickSwatch(color) => {
                if let Some(field) = self.swatch_field.take() {
                    self.rt.set_custom_color(field, to_hex(color));
                    return self.touched();
                }
            }
            Msg::Language(index) => self.rt.set_language(index),
            Msg::FontScale(scale) => self.rt.set_font_scale(scale),
            Msg::DragWindow => return window_task(iced::window::drag),
            Msg::ToggleAlwaysOnTop => {
                let on = !self.rt.prefs.always_on_top;
                self.rt.set_always_on_top(on);
                return window_task(move |id| {
                    iced::window::set_level::<Msg>(
                        id,
                        if on {
                            iced::window::Level::AlwaysOnTop
                        } else {
                            iced::window::Level::Normal
                        },
                    )
                });
            }
            Msg::MinimizeWindow => return window_task(|id| iced::window::minimize(id, true)),
            Msg::ToggleMaximize => {
                #[cfg(windows)]
                {
                    if self.maximized {
                        self.maximized = false;
                        let rect = self.restore_rect.take();
                        if let Some(rect) = rect {
                            return window_task(move |id| {
                                iced::window::run(id, move |window| {
                                    win32::restore(window, rect);
                                    Msg::Maximized(None)
                                })
                            });
                        }
                        return Task::none();
                    }
                    self.maximized = true;
                    return window_task(|id| {
                        iced::window::run(id, |window| {
                            Msg::Maximized(win32::maximize(window))
                        })
                    });
                }
                #[cfg(not(windows))]
                return window_task(|id| iced::window::toggle_maximize(id));
            }
            Msg::Maximized(rect) => self.restore_rect = rect,
            Msg::CloseWindow => return window_task(iced::window::close),
            Msg::WindowReady => {}
            Msg::OpenRepo => open_url("https://github.com/GordonWen/mqttc"),
            Msg::ToggleSidebar => {
                self.sidebar_visible = !self.sidebar_visible;
                let show = self.sidebar_visible;
                // Maximized windows cannot shrink; the page takes the space.
                if !self.maximized {
                    return window_task(move |id| {
                        iced::window::is_maximized(id).then(move |maximized| {
                            if maximized {
                                return Task::none();
                            }
                            let min = if show {
                                MIN_WINDOW_WIDTH
                            } else {
                                MIN_WINDOW_WIDTH - SIDEBAR_WIDTH
                            };
                            iced::window::size(id).then(move |size: Size| {
                                let width = if show {
                                    size.width + SIDEBAR_WIDTH
                                } else {
                                    size.width - SIDEBAR_WIDTH
                                }
                                .max(min);
                                // Window effects never produce a message, so
                                // they cannot be chained with `.then` — the
                                // `set_min_size` step would silently never
                                // run. Batch both instead; the resize and the
                                // new minimum are then applied back to back,
                                // which keeps dragging continuous afterwards.
                                Task::batch([
                                    iced::window::resize::<Msg>(
                                        id,
                                        Size::new(width, size.height),
                                    ),
                                    iced::window::set_min_size::<Msg>(
                                        id,
                                        Some(Size::new(min, MIN_WINDOW_HEIGHT)),
                                    ),
                                ])
                            })
                        })
                    });
                }
            }
            Msg::ResizeWindow(direction) => {
                return window_task(move |id| iced::window::drag_resize(id, direction));
            }
            Msg::AddRule => {
                self.rt.form.add_rule();
                self.rt.apply_form();
                self.rt.persist();
            }
            Msg::DelRule(index) => {
                self.rt.form.remove_rule(index);
                self.rt.apply_form();
                self.rt.persist();
            }
            Msg::MoveRule(index, delta) => {
                self.rt.move_rule(index, delta);
            }
            Msg::ResetRuleStats => self.rt.reset_rule_stats(),
            Msg::RuleInterval(index, value) => {
                if let Some(rule) = self.rt.form.rules.get_mut(index) {
                    rule.min_interval_secs = value;
                }
                self.rt.apply_form();
            }
            Msg::RuleEnabled(index, on) => {
                if let Some(rule) = self.rt.form.rules.get_mut(index) {
                    rule.enabled = on;
                }
                self.rt.apply_form();
            }
            Msg::RuleRetain(index, on) => {
                if let Some(rule) = self.rt.form.rules.get_mut(index) {
                    rule.retain = on;
                }
                self.rt.apply_form();
            }
            Msg::RuleNum(index, value) => {
                if let Some(rule) = self.rt.form.rules.get_mut(index) {
                    rule.qos = value;
                }
                self.rt.apply_form();
            }
            Msg::RuleFormat(index, value) => {
                if let Some(rule) = self.rt.form.rules.get_mut(index) {
                    rule.format = value;
                }
                self.rt.apply_form();
            }
            Msg::RuleText(index, field, value) => {
                if let Some(rule) = self.rt.form.rules.get_mut(index) {
                    match field {
                        RuleTextField::MatchTopic => rule.match_topic = value,
                        RuleTextField::Contains => rule.payload_contains = value,
                        RuleTextField::ReplyTopic => rule.reply_topic = value,
                        RuleTextField::ReplyPayload => rule.reply_payload = value,
                    }
                }
                self.rt.apply_form();
            }
            Msg::AddPreset => {
                self.rt.form.add_preset();
                self.rt.apply_form();
                self.rt.persist();
            }
            Msg::DelPreset(index) => {
                self.rt.form.remove_preset(index);
                if self.rt.form.presets.is_empty() {
                    self.presets_open = false;
                }
                self.rt.apply_form();
                self.rt.persist();
            }
            Msg::PresetRetain(index, on) => {
                if let Some(preset) = self.rt.form.presets.get_mut(index) {
                    preset.retain = on;
                }
                self.rt.apply_form();
            }
            Msg::PresetNum(index, value) => {
                if let Some(preset) = self.rt.form.presets.get_mut(index) {
                    preset.qos = value;
                }
                self.rt.apply_form();
            }
            Msg::PresetFormat(index, value) => {
                if let Some(preset) = self.rt.form.presets.get_mut(index) {
                    preset.format = value;
                }
                self.rt.apply_form();
            }
            Msg::PresetText(index, field, value) => {
                if let Some(preset) = self.rt.form.presets.get_mut(index) {
                    match field {
                        PresetTextField::Name => preset.name = value,
                        PresetTextField::Topic => preset.topic = value,
                        PresetTextField::Payload => preset.payload = value,
                    }
                }
                self.rt.apply_form();
            }
            Msg::LogFilter(value) => {
                self.rt.set_log_filter(value);
                self.reset_log_window();
            }
            Msg::LogKind(value) => {
                self.rt.set_log_kind_filter(value);
                self.reset_log_window();
            }
            Msg::LogFormat(value) => {
                self.rt.set_log_format(value);
                self.preview_cache.borrow_mut().clear();
            }
            Msg::AutoScroll(on) => {
                if on {
                    self.log_freeze = None;
                    self.log_scroll_y = Some(0.0);
                    self.rt.set_auto_scroll(true);
                    self.rt.maybe_persist();
                    return iced::widget::operation::scroll_to(
                        iced::widget::Id::new(LOG_SCROLL_ID),
                        iced::widget::operation::AbsoluteOffset {
                            x: None,
                            y: Some(0.0),
                        },
                    );
                }
                if self.log_freeze.is_none() {
                    self.freeze_logs(true);
                }
                self.rt.set_auto_scroll(false);
            }
            Msg::LogScrolled(y) => {
                let previous = self.log_scroll_y.replace(y);
                // The list is anchored to its end, so offset 0 is the newest
                // row and a growing offset means the user moved up by wheel or
                // scrollbar. Freeze the page they are reading and stop following.
                if self.rt.prefs.auto_scroll
                    && previous.is_some_and(|previous| y > previous + 4.0)
                {
                    if self.log_freeze.is_none() {
                        self.freeze_logs(true);
                    }
                    self.rt.set_auto_scroll(false);
                }
            }
            Msg::ResetCounts => self.rt.reset_active_counts(),
            Msg::Field(field, value) => {
                field.set(&mut self.rt.form, value);
                return self.touched();
            }
            Msg::Num(field, value) => {
                field.set(&mut self.rt.form, value);
                return self.touched();
            }
            Msg::Flag(flag, on) => {
                flag.set(&mut self.rt.form, on);
                return self.touched();
            }
            Msg::PubEdit(action) => {
                if matches!(
                    &action,
                    text_editor::Action::Edit(text_editor::Edit::Enter)
                ) {
                    self.editor_enter = Some(EnterUndo {
                        at: Instant::now(),
                        text: self.editor.text(),
                        replaced_selection: self.editor.selection().is_some(),
                    });
                } else if self.editor_enter.is_some() {
                    self.editor_enter = None;
                }
                let is_edit = action.is_edit();
                self.editor.perform(action);
                if is_edit {
                    self.rt.form.pub_payload = self.editor.text();
                    return self.touched();
                }
            }
        }
        self.rt.maybe_persist();
        Task::none()
    }

    fn on_tick(&mut self) -> Task<Msg> {
        let summary = self.rt.poll();
        if let Some(page) = summary.page_request {
            self.page = page;
            self.reload_form();
        }

        if let Some(at) = self.dirty_at {
            if Instant::now() >= at {
                self.dirty_at = None;
                self.rt.apply_form();
                self.rt.persist();
            }
        }
        self.rt.maybe_persist();

        if let Some((_, until)) = self.copy_flash {
            if Instant::now() >= until {
                self.copy_flash = None;
            }
        }

        if self.rt.prefs.skin_choice == 0 && self.rt.refresh_system_skin() {
            // Palette changed; nothing else to do until the next frame.
        }

        // A session that starts with auto-scroll off keeps its first page: pin
        // a copy as soon as the page is full so eviction cannot slide it.
        if !self.rt.prefs.auto_scroll
            && self.log_freeze.is_none()
            && self.rt.visible_logs().len() >= LOG_RENDER_ROWS
        {
            self.freeze_logs(false);
        }

        let Some(bench) = self.bench.as_mut() else {
            return Task::none();
        };

        if !bench.announced {
            bench.announced = true;
            if bench.options.auto_connect() {
                self.rt.apply_form();
                self.rt.connect();
            }
            println!(
                "BENCH scenario={} connect={} timed={}",
                bench.options.scenario.as_deref().unwrap_or("s1"),
                bench.options.auto_connect(),
                bench.options.wants_timed()
            );
        }

        if bench.options.wants_timed() && !bench.timed_started {
            let connected = self
                .rt
                .active_id()
                .map(|id| self.rt.is_connected(&id))
                .unwrap_or(false);
            if connected {
                self.rt.toggle_timed();
                bench.timed_started = true;
            }
        }

        if !bench.finished {
            if let Some(quit_at) = bench.quit_at {
                if Instant::now() >= quit_at {
                    bench.finished = true;
                    let logs = self.rt.visible_logs();
                    let recv = logs.iter().filter(|log| log.kind == "RECV").count();
                    let send = logs.iter().filter(|log| log.kind == "SEND").count();
                    println!(
                        "BENCH_SUMMARY logs={} recv={} send={} status={:?}",
                        logs.len(),
                        recv,
                        send,
                        self.rt.status()
                    );
                    println!(
                        "BENCH_QUIT {:.2}",
                        self.started.elapsed().as_secs_f64()
                    );
                    return iced::exit();
                }
            }
        }
        Task::none()
    }

    // ------------------------------------------------------------ view

    pub fn view(&self) -> El<'_> {
        if self.first_frame_ms.get().is_none() {
            let ms = self.started.elapsed().as_secs_f64() * 1000.0;
            self.first_frame_ms.set(Some(ms));
            println!("FIRST_FRAME_MS {ms:.1}");
        }
        let c = Colors::from_palette(self.rt.palette());
        set_font_scale(self.rt.prefs.font_scale);
        let divider = container(Space::new().width(1).height(Length::Fill)).style(move |_| {
            container::Style {
                background: Some(iced::Background::Color(c.hover_bg)),
                ..container::Style::default()
            }
        });
        let content: El<'_> = if self.sidebar_visible {
            row![self.sidebar(&c), divider, self.main_area(&c)]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            row![self.main_area(&c)]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        };
        let page: El<'_> = column![
            self.title_bar(&c),
            container(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(c.card()),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
        let base: El<'_> = Stack::with_children([page, self.resize_grips()])
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        let Some(pending) = self.pending_delete.as_deref() else {
            return base;
        };
        let name = self
            .rt
            .config
            .connections
            .iter()
            .find(|conn| conn.id == pending)
            .map(|conn| conn.name.clone())
            .unwrap_or_default();
        let dialog = container(
            column![
                text(t("删除连接？", "Delete connection?")).size(fs(16.0)).color(c.text_title),
                text(t(
                    &format!("“{name}”及其订阅、规则和预设将从 mqttc.json 中移除。"),
                    &format!("“{name}” and its subscriptions, rules and presets will be removed from mqttc.json."),
                ))
                .size(fs(12.0))
                .color(c.text_body)
                .wrapping(Wrapping::WordOrGlyph),
                row![
                    button(text(t("取消", "Cancel")).size(fs(12.0)))
                        .padding([6, 14])
                        .style(c.ghost_button())
                        .on_press(Msg::CancelDelete),
                    button(text(t("删除", "Delete")).size(fs(12.0)))
                        .padding([6, 14])
                        .style(c.danger_outline_button())
                        .on_press(Msg::ConfirmDelete),
                ]
                .spacing(8),
            ]
            .spacing(12),
        )
        .padding(18)
        .width(380)
        .style(c.card());
        let backdrop = container(dialog)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(iced::Color {
                    a: 0.55,
                    ..iced::Color::BLACK
                })),
                ..container::Style::default()
            });
        Stack::with_children([base, backdrop.into()])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// Custom title strip: brand on the left (doubles as the drag handle),
    /// window buttons on the right. Replaces the OS caption bar.
    fn title_bar(&self, c: &Colors) -> El<'_> {
        /// One caption cell: identical box for every glyph, so the three
        /// controls read as a set (VSCode/Qoder style) instead of tracking the
        /// glyph's own width.
        fn control<'a, S>(
            icon: Icon,
            color: iced::Color,
            style: S,
            msg: Msg,
        ) -> El<'a>
        where
            S: Fn(&iced::Theme, button::Status) -> button::Style + 'static,
        {
            button(
                container(IconCanvas::new(icon, color).widget(15.0))
                    .width(Length::Fixed(46.0))
                    .height(Length::Fixed(34.0))
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            )
            .padding(0)
            .style(style)
            .on_press(msg)
            .into()
        }

        let mark: El<'_> = match &self.icon {
            Some(handle) => Image::new(handle.clone())
                .width(18)
                .height(18)
                .content_fit(iced::ContentFit::Contain)
                .into(),
            None => Space::new().width(0).into(),
        };

        let brand = mouse_area(
            row![
                mark,
                text("Mqtt Debug Client")
                    .size(fs(13.0))
                    .color(c.text_title),
                Space::new().width(Length::Fill),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .width(Length::Fill),
        )
        .on_press(Msg::DragWindow)
        .on_double_click(Msg::ToggleMaximize);
        let toggle = tooltip(
            button(
                container(IconCanvas::new(Icon::Panel, c.text_muted).widget(16.0))
                    .width(Length::Fixed(28.0))
                    .height(Length::Fixed(26.0))
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            )
            .padding(0)
            .style(c.button_kind(ButtonKind::Ghost))
            .on_press(Msg::ToggleSidebar),
            container(
                text(t(
                    if self.sidebar_visible {
                        "隐藏侧栏"
                    } else {
                        "显示侧栏"
                    },
                    if self.sidebar_visible {
                        "Hide sidebar"
                    } else {
                        "Show sidebar"
                    },
                ))
                .size(fs(11.0))
                .color(c.text_primary),
            )
            .padding([5, 8])
            .style(c.inset()),
            tooltip::Position::Bottom,
        )
        .delay(Duration::from_millis(300));
        let pinned = self.rt.prefs.always_on_top;
        let pin = tooltip(
            control(
                if pinned { Icon::PinDown } else { Icon::Pin },
                if pinned { c.accent } else { c.text_muted },
                c.caption_button(),
                Msg::ToggleAlwaysOnTop,
            ),
            container(
                text(t(
                    if pinned { "取消置顶" } else { "窗口置顶" },
                    if pinned {
                        "Stop staying on top"
                    } else {
                        "Always on top"
                    },
                ))
                .size(fs(11.0))
                .color(c.text_primary),
            )
            .padding([5, 8])
            .style(c.inset()),
            tooltip::Position::Bottom,
        )
        .delay(Duration::from_millis(300));
        let controls = row![
            pin,
            Space::new().width(4),
            control(
                Icon::Minus,
                c.text_muted,
                c.caption_button(),
                Msg::MinimizeWindow
            ),
            control(
                Icon::Square,
                c.text_muted,
                c.caption_button(),
                Msg::ToggleMaximize
            ),
            control(
                Icon::Close,
                c.text_muted,
                c.caption_button(),
                Msg::CloseWindow
            ),
        ];
        container(
            row![toggle, brand, controls]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .padding(iced::Padding {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 10.0,
        })
        .width(Length::Fill)
        .into()
    }

    /// Thin invisible grips along every edge and corner, since a frameless
    /// window has no OS resize border.
    fn resize_grips(&self) -> El<'_> {
        use iced::mouse::Interaction;
        use iced::window::Direction;
        fn grip<'a>(
            direction: Direction,
            width: Length,
            height: Length,
            cursor: Interaction,
        ) -> El<'a> {
            mouse_area(Space::new().width(width).height(height))
                .interaction(cursor)
                .on_press(Msg::ResizeWindow(direction))
                .into()
        }
        let corner = |direction, cursor| {
            grip(direction, Length::Fixed(5.0), Length::Fixed(5.0), cursor)
        };
        let edge_h = Length::Fixed(5.0);
        let edge_v = Length::Fixed(5.0);
        let top = row![
            corner(Direction::NorthWest, Interaction::ResizingDiagonallyDown),
            grip(
                Direction::North,
                Length::Fill,
                edge_h,
                Interaction::ResizingVertically
            ),
            corner(Direction::NorthEast, Interaction::ResizingDiagonallyUp),
        ]
        .height(edge_h);
        let middle = row![
            grip(
                Direction::West,
                edge_v,
                Length::Fill,
                Interaction::ResizingHorizontally
            ),
            Space::new().width(Length::Fill).height(Length::Fill),
            grip(
                Direction::East,
                edge_v,
                Length::Fill,
                Interaction::ResizingHorizontally
            ),
        ]
        .height(Length::Fill);
        let bottom = row![
            corner(Direction::SouthWest, Interaction::ResizingDiagonallyUp),
            grip(
                Direction::South,
                Length::Fill,
                edge_h,
                Interaction::ResizingVertically
            ),
            corner(Direction::SouthEast, Interaction::ResizingDiagonallyDown),
        ]
        .height(edge_h);
        column![top, middle, bottom]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn sidebar(&self, c: &Colors) -> El<'_> {
        let mut list = column![].spacing(3);
        let active = self.rt.active_id();
        for conn in &self.rt.config.connections {
            let status = self.rt.status_of(&conn.id);
            let is_active = active.as_deref() == Some(conn.id.as_str());
            let dot = text("●").size(fs(9.0)).color(rgb(status.rgb()));
            let name = text(conn.name.as_str())
                .size(fs(13.0))
                .color(if is_active { c.text_primary } else { c.text_body });
            let label = row![dot, name]
                .spacing(6)
                .align_y(Alignment::Center)
                .width(Length::Fill);
            let pick = button(label)
                .width(Length::Fill)
                .padding([6, 8])
                .style(c.button_kind(if is_active {
                    ButtonKind::Selected
                } else {
                    ButtonKind::Ghost
                }))
                .on_press(Msg::Select(conn.id.clone()));
            let del = button(text("×").size(fs(14.0)))
                .padding([3, 5])
                .style(c.danger_button())
                .on_press(Msg::Delete(conn.id.clone()));
            list = list.push(row![pick, del].spacing(2));
        }

        let nav = [
            (2, t("设置", "Settings"), Icon::Gear),
            (5, t("关于", "About"), Icon::Info),
        ];
        let mut links = column![].spacing(2);
        // New connection heads the same block as the page links, so the three
        // actions line up on one baseline grid at the bottom of the sidebar.
        links = links.push(
            button(
                row![
                    IconCanvas::new(Icon::Plus, c.text_muted).widget(fs(15.0)),
                    text(t("新建连接", "New connection"))
                        .size(fs(13.0))
                        .color(c.text_body),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([5, 8])
            .style(c.ghost_button())
            .on_press(Msg::New),
        );
        for (page, label, icon) in nav {
            let selected = self.page == page;
            let tint = if selected {
                c.text_primary
            } else {
                c.text_muted
            };
            links = links.push(
                button(
                    row![
                        IconCanvas::new(icon, tint).widget(fs(15.0)),
                        text(label)
                            .size(fs(13.0))
                            .color(if selected { c.text_primary } else { c.text_body }),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding([5, 8])
                .style(c.button_kind(if selected {
                    ButtonKind::Selected
                } else {
                    ButtonKind::Ghost
                }))
                .on_press(Msg::Page(page)),
            );
        }

        container(
            column![
                scrollable(list).height(Length::Fill),
                Space::new().height(6),
                links,
            ]
            .spacing(5)
            .padding(8)
            .height(Length::Fill),
        )
        .width(SIDEBAR_WIDTH)
        .height(Length::Fill)
        .into()
    }

    fn main_area(&self, c: &Colors) -> El<'_> {
        let body: El<'_> = match self.page {
            0 => self.page_editor(c),
            1 => self.page_session(c),
            2 => self.page_settings(c),
            _ => self.page_about(c),
        };
        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn section(&self, c: &Colors, title: &str) -> El<'static> {
        text(title.to_string()).size(fs(12.0)).color(c.text_muted).into()
    }

    fn hint(&self, c: &Colors, text_value: String) -> El<'static> {
        text(text_value).size(fs(11.0)).color(c.text_faint).into()
    }

    fn input(&self, c: &Colors, field: Field, placeholder: &'static str) -> El<'_> {
        text_input(placeholder, field.get(&self.rt.form))
            .size(fs(13.0))
            .padding([7, 9])
            .width(Length::Fill)
            .style(c.input_style())
            .on_input(move |v| Msg::Field(field, v))
            .into()
    }

    fn labelled<'a>(&'a self, c: &Colors, label: &str, input: El<'a>) -> El<'a> {
        column![text(label.to_string()).size(fs(11.0)).color(c.text_faint), input]
            .spacing(3)
            .into()
    }

    fn page_editor(&self, c: &Colors) -> El<'_> {
        let f = &self.rt.form;
        let connected = self
            .rt
            .active_id()
            .map(|id| self.rt.status_of(&id) != ConnStatus::Disconnected)
            .unwrap_or(false);

        let general = column![
            self.section(c, &t("连接设置", "Connection settings")),
            row![
                self.labelled(c, &t("名称", "Name"), self.input(c, Field::Name, "Local bench")),
                self.labelled(c, &t("协议", "Protocol"), pick(c, &proto_opts(), f.protocol, |v| Msg::Num(Num::Protocol, v))),
            ]
            .spacing(10),
            row![
                self.labelled(c, &t("主机", "Host"), self.input(c, Field::Host, "127.0.0.1")),
                column![
                    text(t("端口", "Port")).size(fs(11.0)).color(c.text_faint),
                    text_input("1883", &f.port)
                        .size(fs(13.0))
                        .padding([7, 9])
                        .width(120)
                        .style(c.input_style())
                        .on_input(|v| Msg::Field(Field::Port, v)),
                ]
                .spacing(3)
                .width(120),
            ]
            .spacing(10),
            row![
                self.labelled(c, &t("客户端 ID", "Client ID"), self.input(c, Field::ClientId, "auto")),
                button(text(t("生成", "Generate")).size(fs(12.0)))
                    .padding([7, 10])
                    .style(c.ghost_button())
                    .on_press(Msg::GenId),
            ]
            .spacing(8)
            .align_y(Alignment::End),
            row![
                self.labelled(c, &t("用户名", "Username"), self.input(c, Field::Username, "")),
                self.labelled(c, &t("密码", "Password"), {
                    text_input("", &f.password)
                        .secure(true)
                        .size(fs(13.0))
                        .padding([7, 9])
                        .width(Length::Fill)
                        .style(c.input_style())
                        .on_input(|v| Msg::Field(Field::Password, v))
                        .into()
                }),
            ]
            .spacing(10),
        ]
        .spacing(10);

        let advanced = column![
            self.section(c, &t("高级", "Advanced")),
            row![
                self.labelled(c, &t("Keep Alive (s)", "Keep alive (s)"), self.input(c, Field::KeepAlive, "60")),
                self.labelled(c, &t("连接超时 (s)", "Connect timeout (s)"), self.input(c, Field::Timeout, "10")),
                column![
                    text(t("Clean session", "Clean session")).size(fs(11.0)).color(c.text_faint),
                    checkbox(f.clean_session)
                        .size(fs(15.0))
                        .style(c.checkbox_style())
                        .on_toggle(|v| Msg::Flag(Flag::CleanSession, v)),
                ]
                .spacing(3),
                column![
                    text(t("自动重连", "Auto reconnect")).size(fs(11.0)).color(c.text_faint),
                    checkbox(f.auto_reconnect)
                        .size(fs(15.0))
                        .style(c.checkbox_style())
                        .on_toggle(|v| Msg::Flag(Flag::AutoReconnect, v)),
                ]
                .spacing(3),
            ]
            .spacing(10),
        ]
        .spacing(10);

        let tls_note = if !f.ssl {
            t(
                "证书设置仅在启用 SSL 后生效",
                "Certificate settings apply only when SSL is enabled",
            )
        } else if !f.ssl_verify {
            t(
                "已关闭校验：接受任意服务器证书，CA 不生效，存在中间人风险",
                "Verification off: any certificate is accepted and the CA is ignored — MITM risk",
            )
        } else if f.ssl_ca.trim().is_empty() {
            t(
                "使用系统根证书校验服务器证书",
                "Server certificates are verified against the system root store",
            )
        } else {
            t(
                "仅信任所填 CA 证书，不再加载系统根证书",
                "Only the listed CA is trusted; system roots are not loaded",
            )
        };

        let ssl = column![
            self.section(c, &t("SSL / TLS", "SSL / TLS")),
            row![
                checkbox(f.ssl)
                    .label(t("启用 SSL", "Enable SSL"))
                    .size(fs(15.0))
                    .text_size(fs(12.0))
                    .style(c.checkbox_style())
                    .on_toggle(|v| Msg::Flag(Flag::Ssl, v)),
                checkbox(f.ssl_verify)
                    .label(t("校验服务器证书", "Verify certificate"))
                    .size(fs(15.0))
                    .text_size(fs(12.0))
                    .style(c.checkbox_style())
                    .on_toggle(|v| Msg::Flag(Flag::SslVerify, v)),
                // Keep the current trust behaviour in sight, right next to the
                // checkbox that controls it.
                text(tls_note)
                    .size(fs(11.0))
                    .color(c.text_muted)
                    .width(Length::Fill)
                    .wrapping(Wrapping::WordOrGlyph),
            ]
            .spacing(14)
            .align_y(Alignment::Center)
            .width(Length::Fill),
            self.labelled(c, &t("CA 证书", "CA certificate"), self.input(c, Field::Ca, "path/to/ca.pem")),
            row![
                self.labelled(c, &t("客户端证书", "Client certificate"), self.input(c, Field::Cert, "path/to/cert.pem")),
                self.labelled(c, &t("客户端私钥", "Client key"), self.input(c, Field::Key, "path/to/key.pem")),
            ]
            .spacing(10),
            self.hint(
                c,
                t(
                    "可把 .pem / .crt / .key 文件拖到窗口自动填入；连接前会检查文件是否存在",
                    "Drop a .pem / .crt / .key file anywhere to fill a slot; paths are checked before connecting",
                )
                .to_string(),
            ),
        ]
        .spacing(10);

        let will = column![
            self.section(c, &t("遗嘱消息", "Last will")),
            row![
                self.labelled(c, &t("Will Topic", "Will topic"), self.input(c, Field::WillTopic, "mqttc/status")),
                self.labelled(c, &t("Will Payload", "Will payload"), self.input(c, Field::WillPayload, "offline")),
                column![
                    text(t("Will QoS", "Will QoS")).size(fs(11.0)).color(c.text_faint),
                    pick(c, &qos_opts(), f.will_qos, |v| Msg::Num(Num::WillQos, v)),
                ]
                .spacing(3)
                .width(100),
            ]
            .spacing(10),
            checkbox(f.will_retain)
                .label(t("Retain", "Retain"))
                .size(fs(15.0))
                .text_size(fs(12.0))
                .style(c.checkbox_style())
                .on_toggle(|v| Msg::Flag(Flag::WillRetain, v)),
        ]
        .spacing(10);

        let actions = row![
            if connected {
                button(text(t("断开", "Disconnect")).size(fs(13.0)))
                    .padding([8, 18])
                    .style(c.danger_outline_button())
                    .on_press(Msg::Disconnect)
            } else {
                button(text(t("连接", "Connect")).size(fs(13.0)))
                    .padding([8, 18])
                    .style(c.primary_button())
                    .on_press(Msg::Connect)
            },
            button(text(t("会话页", "Session")).size(fs(13.0)))
                .padding([8, 14])
                .style(c.ghost_button())
                .on_press(Msg::Page(1)),
            self.hint(
                c,
                if connected {
                    t("连接中编辑不会生效，请先断开", "Editing while connected has no effect; disconnect first")
                } else {
                    t("修改会保存到 exe 同目录的 mqttc.json", "Changes are saved to mqttc.json next to the exe")
                },
            ),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        container(
            scrollable(
                column![general, advanced, ssl, will, actions]
                    .spacing(12)
                    .padding(8),
            )
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn page_session(&self, c: &Colors) -> El<'_> {
        let Some(conn) = self.rt.active() else {
            return container(self.hint(
                c,
                t("还没有连接，请先新建", "No connection yet — create one first"),
            ))
            .padding(8)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        };
        let status = self.rt.status();
        let connected = status == ConnStatus::Connected;
        let connecting = status == ConnStatus::Connecting;
        let retry_note: El<'_> = match self.rt.reconnect_state() {
            Some((attempt, left)) => text(t(
                &format!("重连 {attempt}/8 · {}s 后", left.as_secs().max(1)),
                &format!("Retry {attempt}/8 · in {}s", left.as_secs().max(1)),
            ))
            .size(fs(11.0))
            .color(c.text_muted)
            .into(),
            None => Space::new().width(0).into(),
        };

        let header = container(
            row![
                text(conn.name.as_str())
                    .size(fs(18.0))
                    .color(c.text_title)
                    .width(Length::Fill)
                    .wrapping(Wrapping::WordOrGlyph),
                text("●").size(fs(10.0)).color(rgb(status.rgb())),
                text(status.text()).size(fs(12.0)).color(c.text_muted),
                text(format!("{}:{}", conn.host, conn.port))
                    .size(fs(12.0))
                    .color(c.text_faint),
                retry_note,
                button(text(t("编辑", "Edit")).size(fs(12.0)))
                    .padding([6, 12])
                    .style(c.ghost_button())
                    .on_press(Msg::Page(0)),
                if connected || connecting {
                    button(text(t("断开", "Disconnect")).size(fs(12.0)))
                        .padding([6, 12])
                        .style(c.danger_outline_button())
                        .on_press(Msg::Disconnect)
                } else {
                    button(text(t("连接", "Connect")).size(fs(12.0)))
                        .padding([6, 12])
                        .style(c.primary_button())
                        .on_press(Msg::Connect)
                },
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([2, 2]);

        let (recv_count, send_count) = self.rt.active_counts();
        // Counters and reset share the tab buttons' frame and type size so the
        // whole strip reads as one control row; only the value colour differs.
        let recv_chip = container(
            text(t(
                &format!("收 · {recv_count}"),
                &format!("In · {recv_count}")
            ))
            .size(fs(12.0))
            .color(c.recv_topic),
        )
        .padding([6, 12])
        .style(c.chip());
        let send_chip = container(
            text(t(
                &format!("发 · {send_count}"),
                &format!("Out · {send_count}")
            ))
            .size(fs(12.0))
            .color(c.send_topic),
        )
        .padding([6, 12])
        .style(c.chip());
        let tabs = row![
            self.tab_button(c, SessionTab::Logs, t("日志", "Logs"), None),
            self.tab_button(
                c,
                SessionTab::Subs,
                t("订阅", "Subscriptions"),
                Some(self.rt.subscriptions().len())
            ),
            self.tab_button(
                c,
                SessionTab::Rules,
                t("自动回复", "Auto reply"),
                Some(self.rt.form.rules.len())
            ),
            self.tab_button(
                c,
                SessionTab::Presets,
                t("预设发布", "Publish presets"),
                Some(self.rt.form.presets.len())
            ),
            Space::new().width(Length::Fill),
            recv_chip,
            send_chip,
            button(text(t("计数清零", "Reset counts")).size(fs(12.0)))
                .padding([6, 12])
                .style(c.ghost_button())
                .on_press(Msg::ResetCounts),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .width(Length::Fill);

        let body: El<'_> = match self.session_tab {
            SessionTab::Logs => {
                if self.detail_seq.is_some() {
                    self.log_detail(c)
                } else {
                    column![self.logs_card(c), self.publish_card(c)]
                        .spacing(6)
                        .height(Length::Fill)
                        .into()
                }
            }
            SessionTab::Subs => self.subs_panel(c),
            SessionTab::Rules => self.page_rules(c),
            SessionTab::Presets => self.page_presets(c),
        };

        container(column![header, tabs, body].spacing(8).height(Length::Fill))
            .padding(8)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn tab_button(
        &self,
        c: &Colors,
        tab: SessionTab,
        label: String,
        count: Option<usize>,
    ) -> El<'_> {
        let caption = match count {
            Some(n) if n > 0 => format!("{label} · {n}"),
            _ => label,
        };
        button(text(caption).size(fs(12.0)))
            .padding([6, 12])
            .style(c.button_kind(if self.session_tab == tab {
                ButtonKind::Selected
            } else {
                ButtonKind::Ghost
            }))
            .on_press(Msg::SessionTab(tab))
            .into()
    }

    fn subs_panel(&self, c: &Colors) -> El<'_> {
        let connected = self.rt.status() == ConnStatus::Connected;
        let active = self.rt.active_sub_topic();
        let subs = self.rt.subscriptions();

        let mut list = column![].spacing(3);
        for (index, sub) in subs.iter().enumerate() {
            let selected = !active.is_empty() && active == sub.topic;
            let label = text(sub.topic.as_str())
                .size(fs(13.0))
                .color(if selected { c.text_primary } else { c.text_body });
            let pick = button(label)
                .width(Length::Fill)
                .padding([7, 8])
                .style(c.button_kind(if selected {
                    ButtonKind::Selected
                } else {
                    ButtonKind::Ghost
                }))
                .on_press(Msg::ToggleSub(sub.topic.clone()));
            let qos = text(format!("QoS {}", sub.qos))
                .size(fs(11.0))
                .color(c.text_faint);
            let nl = button(text("NL").size(fs(10.0)))
                .padding([2, 6])
                .style(c.button_kind(if sub.no_local {
                    ButtonKind::Selected
                } else {
                    ButtonKind::Ghost
                }))
                .on_press(Msg::FlipSubOption(index, true));
            let rap = button(text("RAP").size(fs(10.0)))
                .padding([2, 6])
                .style(c.button_kind(if sub.retain_as_published {
                    ButtonKind::Selected
                } else {
                    ButtonKind::Ghost
                }))
                .on_press(Msg::FlipSubOption(index, false));
            let del = button(text("×").size(fs(14.0)))
                .padding([4, 8])
                .style(c.danger_button())
                .on_press(Msg::Unsubscribe(index));
            list = list.push(
                row![pick, qos, nl, rap, del]
                    .spacing(6)
                    .align_y(Alignment::Center),
            );
        }
        if subs.is_empty() {
            list = list.push(self.hint(c, t("尚未订阅", "No subscriptions yet")));
        }

        let subscribe = button(text(t("订阅", "Subscribe")).size(fs(12.0)))
            .padding([6, 16])
            .style(c.primary_button())
            .on_press_maybe(connected.then_some(Msg::Subscribe));

        let status_hint = if connected {
            t("点击订阅条目按 Topic 筛选日志", "Click a subscription to filter logs by topic")
        } else {
            t("未连接，连接后即可订阅", "Connect first to subscribe")
        };

        container(
            column![
                row![
                    self.section(c, &t("订阅", "Subscriptions")),
                    text(subs.len().to_string()).size(fs(11.0)).color(c.text_faint),
                    Space::new().width(Length::Fill),
                    self.hint(c, status_hint),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                row![
                    text_input("mqttc/mvp/#", &self.rt.form.sub_topic)
                        .size(fs(13.0))
                        .padding([7, 9])
                        .width(Length::Fill)
                        .style(c.input_style())
                        .on_input(|v| Msg::Field(Field::SubTopic, v)),
                    pick(c, &qos_opts(), self.rt.form.sub_qos, |v| Msg::Num(Num::SubQos, v)),
                    subscribe,
                ]
                .spacing(6)
                .align_y(Alignment::Center),
                row![
                    checkbox(self.rt.form.sub_no_local)
                        .label("No Local")
                        .size(fs(14.0))
                        .text_size(fs(11.0))
                        .style(c.checkbox_style())
                        .on_toggle(|v| Msg::Flag(Flag::SubNoLocal, v)),
                    checkbox(self.rt.form.sub_retain_as_published)
                        .label(t("保留发布标志", "Retain as published"))
                        .size(fs(14.0))
                        .text_size(fs(11.0))
                        .style(c.checkbox_style())
                        .on_toggle(|v| Msg::Flag(Flag::SubRap, v)),
                    self.hint(
                        c,
                        t(
                            "仅 MQTT 5 生效；3.1.1 会忽略。列表中 NL / RAP 可随时切换",
                            "MQTT 5 only (ignored on 3.1.1); toggle NL / RAP on any saved row"
                        ),
                    ),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
                scrollable(list).height(Length::Fill),
                self.hint(
                    c,
                    t(
                        "点击条目会筛选日志，× 取消订阅",
                        "Click a row to filter logs; × unsubscribes"
                    )
                ),
            ]
            .spacing(10)
            .height(Length::Fill),
        )
        .padding(12)
        .height(Length::Fill)
        .style(c.inset())
        .into()
    }

    fn logs_card(&self, c: &Colors) -> El<'_> {
        let prefs = &self.rt.prefs;
        let paused = self.rt.logs_paused();
        let pause_label: El<'_> = if paused {
            text(t(
                &format!("已暂停 · 丢弃 {} 条", self.rt.paused_dropped()),
                &format!("Paused · {} dropped", self.rt.paused_dropped()),
            ))
            .size(fs(11.0))
            .color(c.err_text)
            .into()
        } else {
            Space::new().width(0).into()
        };
        let copy_note = self.copy_note(c);
        // Active subscription-row filter: click the chip to clear it.
        let sub_filter = self.rt.active_sub_topic();
        let filter_chip: El<'_> = if sub_filter.is_empty() {
            Space::new().width(0).into()
        } else {
            button(
                text(t(
                    &format!("筛选：{sub_filter}  ×"),
                    &format!("Filter: {sub_filter}  ×"),
                ))
                .size(fs(11.0)),
            )
            .padding([3, 8])
            .style(c.button_kind(ButtonKind::Selected))
            .on_press(Msg::ClearSubFilter)
            .into()
        };

        let toolbar = column![
            row![
                text_input(
                    l("搜索 Topic / Payload  (Ctrl+F)", "Search topic / payload  (Ctrl+F)"),
                    &prefs.log_filter
                )
                .id(iced::widget::Id::new("log-search"))
                .size(fs(13.0))
                .padding([5, 7])
                .width(Length::Fill)
                .style(c.input_style())
                .on_input(Msg::LogFilter),
                pick(c, &kind_opts(), prefs.log_kind_filter, Msg::LogKind),
                pick(c, &format_opts(), prefs.log_format, Msg::LogFormat),
                checkbox(prefs.auto_scroll)
                    .label(t("自动滚动", "Auto-scroll"))
                    .size(fs(15.0))
                    .text_size(fs(12.0))
                    .style(c.checkbox_style())
                    .on_toggle(Msg::AutoScroll),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
            row![
                filter_chip,
                pause_label,
                Space::new().width(Length::Fill),
                copy_note,
                button(text(t("复制最新记录", "Copy latest record")).size(fs(11.0)))
                    .padding([4, 9])
                    .style(c.ghost_button())
                    .on_press(Msg::CopyRow),
                button(text(if paused {
                    t("继续", "Resume")
                } else {
                    t("暂停", "Pause")
                })
                .size(fs(12.0)))
                .padding([5, 10])
                .style(c.button_kind(if paused {
                    ButtonKind::Selected
                } else {
                    ButtonKind::Ghost
                }))
                .on_press(Msg::TogglePause),
                button(text(t("保存", "Save")).size(fs(12.0)))
                    .padding([5, 10])
                    .style(c.ghost_button())
                    .on_press(Msg::SaveLogs),
                button(text(t("存筛选", "Save view")).size(fs(12.0)))
                    .padding([5, 10])
                    .style(c.ghost_button())
                    .on_press(Msg::SaveVisibleLogs),
                button(text(t("清空", "Clear")).size(fs(12.0)))
                    .padding([5, 10])
                    .style(c.ghost_button())
                    .on_press(Msg::ClearLogs),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        ]
        .spacing(5);

        let logs = self.rt.visible_logs();
        let total = logs.len();
        // Frozen: render the copy taken when auto-scroll went off, so the ring
        // buffer filling up cannot drag the view along. Following: the newest
        // `log_window` rows straight from the store.
        let rows: Vec<&LogRecord> = if let Some(frozen) = &self.log_freeze {
            frozen.iter().collect()
        } else {
            logs[total.saturating_sub(self.log_window)..].to_vec()
        };
        let shown = rows.len();
        let (hidden_before, hidden_after) = if self.log_freeze.is_some() {
            let first = rows.first().map_or(0, |log| log.seq);
            let last = rows.last().map_or(0, |log| log.seq);
            (
                logs.iter().filter(|log| log.seq < first).count(),
                logs.iter().filter(|log| log.seq > last).count(),
            )
        } else {
            (total.saturating_sub(shown), 0)
        };
        let range_total = hidden_before + shown + hidden_after;
        let mut list = column![].spacing(5);
        if rows.is_empty() {
            list = list.push(
                container(self.hint(c, t("还没有日志", "No logs yet")))
                    .padding(12)
                    .center_x(Length::Fill),
            );
        } else {
            if hidden_before > 0 {
                list = list.push(
                    row![
                        self.hint(
                            c,
                            t(
                                &format!("仅显示 {shown} 条，共 {range_total} 条"),
                                &format!("Showing {shown} of {range_total}"),
                            ),
                        ),
                        button(text(t("显示更早", "Show older")).size(fs(11.0)))
                            .padding([3, 8])
                            .style(c.ghost_button())
                            .on_press(Msg::ShowOlderLogs),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
            for log in rows.iter().copied() {
                list = list.push(self.log_bubble(c, log));
            }
            if hidden_after > 0 {
                list = list.push(
                    row![
                        self.hint(
                            c,
                            t(
                                &format!(
                                    "下方还有 {hidden_after} 条新日志（仅保留最近 {} 条，更早的自动丢弃）",
                                    self.rt.config.log_limit
                                ),
                                &format!(
                                    "{hidden_after} newer entries below (only the latest {} are kept; older ones are dropped)",
                                    self.rt.config.log_limit
                                ),
                            ),
                        ),
                        button(text(t("回到最新", "Jump to latest")).size(fs(11.0)))
                            .padding([3, 8])
                            .style(c.ghost_button())
                            .on_press(Msg::AutoScroll(true)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
        }

        // The scrollbar lane hugs the card border (2 px). The toolbar and the
        // list are inset instead, so the bar keeps clear of SEND bubbles,
        // which sit on their row's right edge. The list needs a little more
        // than the bar is wide (10 px) so there is a visible gap.
        let toolbar = container(toolbar)
            .width(Length::Fill)
            .padding(Padding {
                right: 8.0,
                ..Padding::default()
            });
        let list = container(list)
            .width(Length::Fill)
            .padding(Padding {
                right: 14.0,
                ..Padding::default()
            });
        container(
            column![
                toolbar,
                scrollable(list)
                    .id(iced::widget::Id::new(LOG_SCROLL_ID))
                    .height(Length::Fill)
                    .anchor_bottom()
                    .auto_scroll(prefs.auto_scroll && !paused)
                    .on_scroll(|viewport| Msg::LogScrolled(viewport.absolute_offset().y)),
            ]
            .spacing(6)
            .height(Length::Fill),
        )
        .padding(Padding {
            top: 10.0,
            right: 2.0,
            bottom: 10.0,
            left: 10.0,
        })
        .height(Length::FillPortion(3))
        .style(c.inset())
        .into()
    }

    /// Inline preview with a per-row cache; JSON parsing and hex encoding are
    /// far too expensive to repeat for 200 bubbles on every 100 ms tick.
    fn preview_body(&self, log: &LogRecord, fmt: PayloadFormat) -> (String, bool) {
        let fmt_index = fmt.to_index();
        if let Some((cached_fmt, body)) = self.preview_cache.borrow().get(&log.seq) {
            if *cached_fmt == fmt_index {
                let truncated = log.is_payload_kind()
                    && log.payload.len() > payload::preview_limit(fmt);
                return (body.clone(), truncated);
            }
        }
        let (body, truncated) = if log.is_payload_kind() {
            payload::preview_payload(&log.payload, fmt, payload::preview_limit(fmt))
        } else {
            (String::from_utf8_lossy(&log.payload).into_owned(), false)
        };
        // Untrusted broker text must not reach the shaper verbatim, otherwise
        // RTL content plus a paragraph separator aborts cosmic-text.
        let body = payload::sanitize_display_text(&body).into_owned();
        let mut cache = self.preview_cache.borrow_mut();
        if cache.len() >= 1024 {
            cache.clear();
        }
        cache.insert(log.seq, (fmt_index, body.clone()));
        (body, truncated)
    }

    /// Full display text of one row's payload (no preview cap).
    fn log_body_text(&self, log: &LogRecord) -> String {
        self.log_body_text_fmt(log, PayloadFormat::from_index(self.rt.prefs.log_format))
    }

    fn log_body_text_fmt(&self, log: &LogRecord, fmt: PayloadFormat) -> String {
        if log.is_payload_kind() {
            payload::render_payload(&log.payload, fmt)
        } else {
            String::from_utf8_lossy(&log.payload).into_owned()
        }
    }

    fn log_bubble(&self, c: &Colors, log: &LogRecord) -> El<'_> {
        let c = *c;
        let fmt = PayloadFormat::from_index(self.rt.prefs.log_format);
        let (bg, topic_color, body_color, right) = match log.kind.as_str() {
            "RECV" => (c.recv_bg, c.recv_topic, c.recv_payload, false),
            "SEND" => (c.send_bg, c.send_topic, c.send_payload, true),
            "ERR" => (c.err_bg, c.err_label, c.err_text, false),
            _ => (c.sys_bg, c.sys_text, c.sys_text, false),
        };
        let (body, truncated) = self.preview_body(log, fmt);
        let topic_line = if log.topic.is_empty() {
            format!("{} · {}", log.time, log.kind)
        } else {
            format!(
                "{} · {} · {}",
                log.time,
                log.kind,
                payload::sanitize_display_text(&log.topic)
            )
        };
        let bytes = human_bytes(log.payload.len());
        let detail_label = t("详情", "Detail");
        let seq = log.seq;

        let flags = log.flags();
        let props = log.props_text();
        let mut meta = flags;
        if !props.is_empty() {
            if !meta.is_empty() {
                meta.push_str(" · ");
            }
            meta.push_str(&props);
        }
        if truncated {
            if !meta.is_empty() {
                meta.push_str(" · ");
            }
            meta.push_str(&t("预览已截断", "preview truncated"));
        }
        let meta_line = (!meta.is_empty())
            .then(|| payload::sanitize_display_text(&meta).into_owned());

        // The bubble follows its content: `max_width` reads the width the row
        // offers during layout and caps the line at `LOG_BUBBLE_MAX` of it.
        // Short messages stay narrow instead of stretching into a wide block.
        // Right-click copies just the topic; the payload body below copies the
        // message itself. Left-click behaviour and wheel scrolling stay as-is.
        let topic_text = text(topic_line)
            .size(fs(10.5))
            .color(topic_color)
            .wrapping(Wrapping::WordOrGlyph);
        let topic_el: El<'_> = if log.topic.is_empty() {
            topic_text.into()
        } else {
            mouse_area(topic_text)
                .on_right_press(Msg::CopyTopic(seq))
                .interaction(iced::mouse::Interaction::Pointer)
                .into()
        };
        let mut header = row![topic_el].spacing(6).align_y(Alignment::Center);
        header = header
            .push(Space::new().width(10.0))
            .push(text(bytes).size(fs(10.0)).color(c.text_faint))
            .push(
                button(text(detail_label).size(fs(10.0)))
                    .padding([1, 6])
                    .style(c.ghost_button())
                    .on_press(Msg::OpenLog(seq)),
            );
        let mut content = column![header].spacing(2);
        if let Some(meta) = meta_line {
            content = content.push(text(meta).size(fs(10.0)).color(c.text_faint));
        }
        content = content.push(
            mouse_area(
                text(body)
                    .size(fs(12.5))
                    .font(mono_font())
                    .color(body_color)
                    .wrapping(Wrapping::WordOrGlyph),
            )
            .on_right_press(Msg::CopyBody(seq))
            .interaction(iced::mouse::Interaction::Pointer),
        );
        let bubble = max_width::max_width(
            container(content).padding([7, 9]).style(c.bubble(bg)),
            LOG_BUBBLE_MAX,
            180.0,
        );

        let mut row_children: Vec<El> = Vec::new();
        if right {
            row_children.push(Space::new().width(Length::Fill).into());
            row_children.push(bubble.into());
        } else {
            row_children.push(bubble.into());
            row_children.push(Space::new().width(Length::Fill).into());
        }
        row(row_children).width(Length::Fill).into()
    }

    /// Transient confirmation after a copy action; empty when none is pending.
    fn copy_note(&self, c: &Colors) -> El<'static> {
        match self.copy_flash {
            Some((kind, until)) if Instant::now() < until => {
                let label = match kind {
                    CopyFlash::Topic => t("已复制 Topic", "Topic copied"),
                    CopyFlash::Payload => t("已复制 Payload", "Payload copied"),
                    CopyFlash::Message => t("已复制整条", "Message copied"),
                };
                text(label).size(fs(11.0)).color(c.accent).into()
            }
            _ => Space::new().width(0).into(),
        }
    }

    fn detail_back(&self, c: &Colors) -> El<'static> {
        button(text(t("← 返回日志", "← Back to logs")).size(fs(12.0)))
            .padding([4, 8])
            .style(c.ghost_button())
            .on_press(Msg::CloseLog)
            .into()
    }

    /// Full-ish payload view. Huge payloads are capped here too; `Save` exports
    /// the complete bytes, so the cap only affects on-screen text shaping.
    fn log_detail(&self, c: &Colors) -> El<'_> {
        let seq = self.detail_seq.unwrap_or(0);
        let Some(log) = self.row_by_seq(seq) else {
            return container(
                column![
                    self.detail_back(c),
                    self.hint(
                        c,
                        t("该条日志已滚出缓冲区", "This entry is no longer buffered").to_string()
                    ),
                ]
                .spacing(10),
            )
            .padding(12)
            .height(Length::Fill)
            .style(c.inset())
            .into();
        };

        let fmt = PayloadFormat::from_index(self.detail_format);
        let (body, cut) = if log.is_payload_kind() {
            payload::preview_payload(&log.payload, fmt, payload::LOG_DETAIL_BYTES)
        } else {
            (String::from_utf8_lossy(&log.payload).into_owned(), false)
        };
        let body = payload::sanitize_display_text(&body).into_owned();
        let topic = payload::sanitize_display_text(&log.topic).into_owned();
        let header = row![
            self.detail_back(c),
            text(format!("{} · {}", log.time, log.kind))
                .size(fs(11.0))
                .color(c.text_faint),
            text(topic)
                .size(fs(12.0))
                .color(c.text_body)
                .width(Length::Fill)
                .wrapping(Wrapping::WordOrGlyph),
            text(human_bytes(log.payload.len()))
                .size(fs(11.0))
                .color(c.text_faint),
            text(t("格式", "Format"))
                .size(fs(11.0))
                .color(c.text_faint),
            pick(c, &format_opts(), self.detail_format, Msg::DetailFormat),
            button(text(t("复制Topic", "Copy topic")).size(fs(11.0)))
                .padding([3, 8])
                .style(c.ghost_button())
                .on_press(Msg::CopyTopic(log.seq)),
            button(text(t("复制Payload", "Copy payload")).size(fs(11.0)))
                .padding([3, 8])
                .style(c.ghost_button())
                .on_press(Msg::CopyLog(log.seq, self.detail_format)),
            button(text(t("复制整条Msg", "Copy message")).size(fs(11.0)))
                .padding([3, 8])
                .style(c.ghost_button())
                .on_press(Msg::CopyDetailRow),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let flags = log.flags();
        let props = log.props_text();
        let meta_line: El<'_> = if flags.is_empty() && props.is_empty() {
            Space::new().width(0).into()
        } else {
            let mut meta = flags;
            if !props.is_empty() {
                if !meta.is_empty() {
                    meta.push_str(" · ");
                }
                meta.push_str(&props);
            }
            text(payload::sanitize_display_text(&meta).into_owned())
                .size(fs(11.0))
                .color(c.text_faint)
                .into()
        };
        let meta_row = row![
            meta_line,
            Space::new().width(Length::Fill),
            self.copy_note(c)
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mut panel = column![
            header,
            meta_row,
            scrollable(
                container(
                    text(body)
                        .size(fs(12.5))
                        .font(mono_font())
                        .color(c.text_body)
                        .wrapping(Wrapping::WordOrGlyph),
                )
                .padding([2, 4])
            )
            .height(Length::Fill),
        ]
        .spacing(8)
        .height(Length::Fill);
        if cut {
            panel = panel.push(self.hint(
                c,
                t(
                    "内容较大，仅显示前 16 KB；完整内容可用“保存”导出",
                    "Large payload: showing the first 16 KB; use Save to export the full log",
                )
                .to_string(),
            ));
        }
        container(panel)
            .padding(12)
            .height(Length::Fill)
            .style(c.inset())
            .into()
    }

    /// One quick-publish button, sized to its name and capped at about nine
    /// CJK glyphs. `max_units` is the display budget; truncated names expose
    /// the full text through a tooltip.
    fn preset_button(&self, c: &Colors, index: usize, max_units: usize) -> El<'_> {
        let preset = &self.rt.form.presets[index];
        let (caption, full) = preset_caption(preset, index, max_units);
        let sized = button(text(caption).size(fs(12.0)))
            .padding([5, 8])
            .width(Length::Shrink)
            .style(c.ghost_button())
            .on_press(Msg::PublishPreset(index));
        match full {
            Some(full) => tooltip(
                sized,
                container(text(full).size(fs(11.0)).color(c.text_primary))
                    .padding([5, 8])
                    .style(c.inset()),
                tooltip::Position::Top,
            )
            .delay(Duration::from_millis(300))
            .into(),
            None => sized.into(),
        }
    }

    /// Header strip: as many presets as fit at their natural widths, with the
    /// fold toggle kept at the far right.
    fn quick_presets(&self, c: &Colors, available: f32) -> El<'_> {
        let order: Vec<usize> = (0..self.rt.form.presets.len()).collect();
        let widths: Vec<f32> = order
            .iter()
            .map(|&index| {
                let (caption, _) = preset_caption(&self.rt.form.presets[index], index, 18);
                caption_button_width(&caption)
            })
            .collect();
        let all_width = widths.iter().sum::<f32>()
            + widths.len().saturating_sub(1) as f32 * PRESET_BUTTON_GAP;
        let shown = if !self.presets_open && all_width <= available {
            widths.len()
        } else {
            pack_button_count(&widths, available - fold_button_width() - PRESET_BUTTON_GAP)
        };
        let mut strip = row![].spacing(PRESET_BUTTON_GAP).align_y(Alignment::Center);
        for &index in order.iter().take(shown) {
            strip = strip.push(self.preset_button(c, index, 18));
        }
        if order.len() > shown || self.presets_open {
            let label = if self.presets_open {
                t("收起预设 ▾", "Hide presets ▾")
            } else {
                t("更多预设 ▸", "More presets ▸")
            };
            strip = strip.push(
                button(text(label).size(fs(12.0)))
                    .padding([4, 10])
                    .style(c.ghost_button())
                    .on_press(Msg::TogglePresetPalette),
            );
        }
        strip.into()
    }

    /// Folded-open preset grid, shown in place of the publish form so long
    /// lists get the whole card body instead of a horizontal scroll.
    fn preset_palette(&self, c: &Colors) -> El<'_> {
        let order: Vec<usize> = (0..self.rt.form.presets.len()).collect();
        let widths: Vec<(usize, f32)> = order
            .iter()
            .map(|&index| {
                let (caption, _) = preset_caption(&self.rt.form.presets[index], index, 18);
                (index, caption_button_width(&caption))
            })
            .collect();
        let colors = *c;
        let grid = responsive(move |size| {
            // Pack whole buttons greedily, wrapping once the next natural
            // width no longer fits the current row.
            let mut rows = column![].spacing(6);
            let mut current = row![].spacing(PRESET_BUTTON_GAP);
            let mut used = 0.0f32;
            for &(index, width) in &widths {
                let add = if used > 0.0 {
                    PRESET_BUTTON_GAP + width
                } else {
                    width
                };
                if used > 0.0 && used + add > size.width {
                    rows = rows.push(current);
                    current = row![].spacing(PRESET_BUTTON_GAP);
                    used = width;
                } else {
                    used += add;
                }
                current = current.push(self.preset_button(&colors, index, 18));
            }
            if used > 0.0 {
                rows = rows.push(current);
            }
            rows.into()
        })
        .height(Length::Shrink);
        column![
            self.hint(
                c,
                t(
                    "点击预设立即发布；可在“预设发布”页用行首箭头调整顺序",
                    "Click a preset to publish; reorder them with the arrows in the Publish presets tab"
                )
                .to_string(),
            ),
            container(scrollable(grid).height(Length::Fixed(168.0)))
                .padding(8)
                .style(c.inset()),
        ]
        .spacing(6)
        .into()
    }

    fn publish_card(&self, c: &Colors) -> El<'_> {
        let f = &self.rt.form;
        let timed = self.rt.timed_running();
        let palette_open = self.presets_open && !f.presets.is_empty();

        let topic_row = row![
            text_input("mqttc/mvp/hello", &f.pub_topic)
                .size(fs(13.0))
                .padding([5, 7])
                .width(Length::Fill)
                .style(c.input_style())
                .on_input(|v| Msg::Field(Field::PubTopic, v)),
            pick(c, &qos_opts(), f.pub_qos, |v| Msg::Num(Num::PubQos, v)),
            checkbox(f.pub_retain)
                .label("Retain")
                .size(fs(15.0))
                .text_size(fs(12.0))
                .style(c.checkbox_style())
                .on_toggle(|v| Msg::Flag(Flag::PubRetain, v)),
            pick(c, &format_opts(), f.pub_format, |v| Msg::Num(Num::PubFormat, v)),
            button(text(t("美化", "Pretty")).size(fs(12.0)))
                .padding([4, 8])
                .style(c.ghost_button())
                .on_press(Msg::Format),
        ]
        .spacing(5)
        .align_y(Alignment::Center);

        let props_row = row![
            text(t("用户属性", "User props")).size(fs(11.0)).color(c.text_faint),
            text_input("k=v;k2=v2", &f.pub_props)
                .size(fs(12.0))
                .padding([4, 7])
                .width(Length::Fill)
                .style(c.input_style())
                .on_input(|v| Msg::Field(Field::PubProps, v)),
            text(t("过期 (s)", "Expiry (s)")).size(fs(11.0)).color(c.text_faint),
            text_input("0", &f.pub_expiry)
                .size(fs(12.0))
                .padding([4, 7])
                .width(70)
                .style(c.input_style())
                .on_input(|v| Msg::Field(Field::PubExpiry, v)),
            self.hint(c, t("仅 MQTT 5", "MQTT 5 only")),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let timed_row = row![
            text_input("5", &f.timed_interval)
                .size(fs(13.0))
                .padding([5, 7])
                .width(70)
                .style(c.input_style())
                .on_input(|v| Msg::Field(Field::TimedInterval, v)),
            text(t("秒一次", "sec interval")).size(fs(11.0)).color(c.text_faint),
            button(
                text(if timed {
                    t("停止定时", "Stop timed")
                } else {
                    t("开始定时", "Start timed")
                })
                .size(fs(12.0))
            )
            .padding([5, 12])
            .style(c.button_kind(if timed {
                ButtonKind::Danger
            } else {
                ButtonKind::Ghost
            }))
            .on_press(Msg::Timed),
            Space::new().width(Length::Fill),
            // The shortcut hint sits beside Publish instead of on its own
            // row, leaving the log area that much taller.
            self.hint(
                c,
                t("Ctrl+Enter 发布", "Ctrl+Enter publishes").to_string(),
            ),
            button(text(t("发布", "Publish")).size(fs(12.0)))
                .padding([5, 14])
                .style(c.primary_button())
                .on_press(Msg::Publish),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .width(Length::Fill);

        let body: El<'_> = if palette_open {
            self.preset_palette(c)
        } else {
            column![
                topic_row,
                props_row,
                // 81 = 2 * 8 padding + 4 * 16.25 line height (12.5 * 1.3):
                // the text area must hold a whole number of lines, otherwise
                // tiny-skia draws the partial edge lines outside the field and
                // they overlap the widgets above/below.
                text_editor(&self.editor)
                    .height(81)
                    .padding(8)
                    .size(fs(12.5))
                    .font(mono_font())
                    .style(c.editor_style())
                    .on_action(Msg::PubEdit),
                timed_row,
            ]
            .spacing(6)
            .into()
        };

        let colors = *c;
        let quick = responsive(move |size| {
            row![
                Space::new().width(Length::Fill),
                self.quick_presets(&colors, size.width),
            ]
            .align_y(Alignment::Center)
            .into()
        })
        .height(Length::Shrink);

        container(
            column![
                row![
                    self.section(c, &t("发布", "Publish")),
                    quick,
                ]
                .align_y(Alignment::Center),
                body,
            ]
            .spacing(6),
        )
        .padding(10)
        .style(c.inset())
        .into()
    }

    /// One row of the custom-palette editor: chip, name, hex box, reset.
    fn color_row(&self, c: &Colors, field: PaletteField) -> El<'_> {
        let (zh, en) = field.label();
        let active = field.get(&self.rt.palette());
        let value = self.rt.custom_color_text(field);
        row![
            button(Space::new())
                .width(22)
                .height(22)
                .padding(0)
                .style(c.swatch_button(active, self.swatch_field == Some(field)))
                .on_press(Msg::SwatchPanel(Some(field))),
            text(t(zh, en))
                .size(fs(12.0))
                .color(c.text_body)
                .width(Length::Fill),
            text_input("", &value)
                .size(fs(12.0))
                .font(mono_font())
                .padding([4, 7])
                .width(110)
                .style(c.input_style())
                .on_input(move |v| Msg::CustomColor(field, v)),
            button(text("↺").size(fs(12.0)))
                .padding([3, 7])
                .style(c.ghost_button())
                .on_press(Msg::ResetCustomColor(field)),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }

    /// Inline chip grid rather than a popup, so it works across all three
    /// shells and never clips the row it was opened from.
    fn swatch_panel(&self, c: &Colors, field: PaletteField) -> El<'_> {
        let (zh, en) = field.label();
        let active = field.get(&self.rt.palette());
        let mut chips = row![].spacing(5);
        for color in SWATCHES {
            chips = chips.push(
                button(Space::new())
                    .width(20)
                    .height(20)
                    .padding(0)
                    .style(c.swatch_button(color, color == active))
                    .on_press(Msg::PickSwatch(color)),
            );
        }
        let chips = chips.wrap();
        container(
            column![
                row![
                    text(t("选择颜色", "Pick a colour"))
                        .size(fs(12.0))
                        .color(c.text_title),
                    text(t(zh, en)).size(fs(11.0)).color(c.text_faint),
                    Space::new().width(Length::Fill),
                    button(text("×").size(fs(12.0)))
                        .padding([1, 6])
                        .style(c.ghost_button())
                        .on_press(Msg::SwatchPanel(None)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                chips,
            ]
            .spacing(8),
        )
        .padding(10)
        .style(c.inset())
        .into()
    }

    fn color_rows(&self, c: &Colors, fields: &[PaletteField]) -> El<'_> {
        let mut rows = column![].spacing(4);
        for &field in fields {
            rows = rows.push(self.color_row(c, field));
            // Keep the chip panel next to the row it belongs to; otherwise a
            // click in the folded-open list would push it out of view.
            if self.swatch_field == Some(field) {
                rows = rows.push(self.swatch_panel(c, field));
            }
        }
        rows.into()
    }

    fn custom_palette_section(&self, c: &Colors) -> El<'_> {
        let base = self.rt.custom_palette().base_skin().to_index();
        let hidden = PaletteField::MORE.len();
        let (glyph, more_label) = if self.more_colors {
            ("▾", t("收起更多颜色", "Show fewer colours"))
        } else {
            (
                "▸",
                t(
                    &format!("展开更多颜色（还有 {hidden} 项）"),
                    &format!("Show more colours ({hidden} more)"),
                ),
            )
        };
        let toggle = button(
            row![
                text(glyph).size(fs(11.0)).color(c.text_muted),
                text(more_label).size(fs(12.0)),
                Space::new().width(Length::Fill),
            ]
            .spacing(6)
            .width(Length::Fill)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([6, 9])
        .style(c.disclosure_button(self.more_colors))
        .on_press(Msg::ToggleMoreColors);

        let mut body = column![
            row![
                text(t("自定义配色", "Custom palette"))
                    .size(fs(13.0))
                    .color(c.text_title),
                Space::new().width(Length::Fill),
                text(t("基础皮肤", "Base skin")).size(fs(11.0)).color(c.text_faint),
                pick(c, &base_skin_opts(), base, Msg::CustomBase),
                button(text(t("按基础重置", "Reset to base")).size(fs(11.0)))
                    .padding([5, 9])
                    .style(c.ghost_button())
                    .on_press(Msg::RefillCustom),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            self.hint(
                c,
                t(
                    "点色块选预设，或直接输入 #rrggbb；无效值自动回退到基础皮肤",
                    "Tap a chip for a preset or type #rrggbb; invalid values fall back to the base skin"
                ),
            ),
            self.color_rows(c, &PaletteField::COMMON),
            toggle,
        ]
        .spacing(8);
        if self.more_colors {
            body = body.push(self.color_rows(c, &PaletteField::MORE));
        }
        container(body).padding(10).style(c.inset()).into()
    }

    fn page_settings(&self, c: &Colors) -> El<'_> {
        let mut body = column![
            self.section(c, &t("外观", "Appearance")),
            row![
                text(t("皮肤", "Skin")).size(fs(13.0)).color(c.text_body),
                pick(c, &skin_opts(), self.rt.prefs.skin_choice, Msg::Skin),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            self.hint(
                c,
                t(
                    "跟随系统会按 Windows/macOS/Linux 的深浅色自动切换",
                    "System follows the OS light/dark setting on Windows/macOS/Linux"
                )
            ),
            row![
                text(t("语言", "Language")).size(fs(13.0)).color(c.text_body),
                pick(c, &lang_opts(), self.rt.prefs.language, Msg::Language),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            row![
                text(t("字号", "Text size")).size(fs(13.0)).color(c.text_body),
                pick(
                    c,
                    &font_opts(),
                    font_index(self.rt.prefs.font_scale),
                    |index| Msg::FontScale(FONT_CHOICES[index as usize].0),
                ),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            self.hint(
                c,
                t(
                    "字号按比例缩放整个界面，适合高分屏或视力需要放大时使用",
                    "Text size scales the whole interface — handy on high-DPI or hard-to-read displays"
                )
            ),
        ]
        .spacing(12);

        if self.rt.skin() == Skin::Custom {
            body = body.push(self.custom_palette_section(c));
        }

        body = body
            .push(Space::new().height(8))
            .push(self.section(c, &t("日志", "Logs")))
            .push(
                row![
                    text(t("历史上限", "History limit"))
                        .size(fs(13.0))
                        .color(c.text_body),
                    pick(
                        c,
                        &log_limit_opts(),
                        self.rt.config.log_limit as i32,
                        Msg::LogLimit
                    ),
                    self.hint(
                        c,
                        t(
                            &format!(
                                "每条连接单独保留最近 {} 条；超出后丢弃最旧的",
                                self.rt.config.log_limit
                            ),
                            &format!(
                                "Keep the latest {} per connection; the oldest entries are dropped when full",
                                self.rt.config.log_limit
                            )
                        )
                    ),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .push(self.hint(
                c,
                t(
                    "大报文在气泡内自动截断，点“展开”查看；单连接原始数据另有 16 MB 内存上限",
                    "Large payloads are truncated inline — use Open to expand; raw data is capped at 16 MB per connection"
                ),
            ))
            .push(Space::new().height(8))
            .push(self.section(c, &t("配置文件", "Configuration")))
            .push(self.hint(
                c,
                t(
                    "所有设置写入 exe 同目录的 mqttc.json，复制整个目录即可迁移",
                    "Everything is stored in mqttc.json next to the exe; copy the folder to migrate"
                ),
            ));

        container(scrollable(body.padding(8)).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn page_rules(&self, c: &Colors) -> El<'_> {
        let mut list = column![].spacing(8);
        let hits = self.rt.active_rule_hits();
        let total = self.rt.form.rules.len();
        for (index, rule) in self.rt.form.rules.iter().enumerate() {
            let hit_count = hits.get(index).copied().unwrap_or(0);
            let card = container(
                column![
                    row![
                        checkbox(rule.enabled)
                            .label(t("启用", "Enabled"))
                            .size(fs(15.0))
                            .text_size(fs(12.0))
                            .style(c.checkbox_style())
                            .on_toggle(move |v| Msg::RuleEnabled(index, v)),
                        text(t("匹配 Topic", "Match topic")).size(fs(11.0)).color(c.text_faint),
                        text_input("mqttc/in/#", &rule.match_topic)
                            .size(fs(13.0))
                            .padding([5, 7])
                            .width(Length::Fill)
                            .style(c.input_style())
                            .on_input(move |v| Msg::RuleText(index, RuleTextField::MatchTopic, v)),
                        text(t("间隔 s", "Every s")).size(fs(10.0)).color(c.text_faint),
                        text_input("0", &rule.min_interval_secs)
                            .size(fs(12.0))
                            .padding([4, 6])
                            .width(56)
                            .style(c.input_style())
                            .on_input(move |v| Msg::RuleInterval(index, v)),
                        text(t(
                            &format!("命中 {hit_count}"),
                            &format!("{hit_count} hits"),
                        ))
                        .size(fs(10.0))
                        .color(c.text_faint),
                        button(text("↑").size(fs(12.0)))
                            .padding([2, 5])
                            .style(c.ghost_button())
                            .on_press_maybe((index > 0).then_some(Msg::MoveRule(index, -1))),
                        button(text("↓").size(fs(12.0)))
                            .padding([2, 5])
                            .style(c.ghost_button())
                            .on_press_maybe(
                                (index + 1 < total).then_some(Msg::MoveRule(index, 1)),
                            ),
                        button(text("×").size(fs(13.0)))
                            .padding([3, 7])
                            .style(c.danger_button())
                            .on_press(Msg::DelRule(index)),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                    row![
                        text(t("包含文本", "Contains")).size(fs(11.0)).color(c.text_faint),
                        text_input("", &rule.payload_contains)
                            .size(fs(13.0))
                            .padding([5, 7])
                            .width(Length::Fill)
                            .style(c.input_style())
                            .on_input(move |v| Msg::RuleText(index, RuleTextField::Contains, v)),
                        text(t("回复 Topic", "Reply topic")).size(fs(11.0)).color(c.text_faint),
                        text_input("", &rule.reply_topic)
                            .size(fs(13.0))
                            .padding([5, 7])
                            .width(Length::Fill)
                            .style(c.input_style())
                            .on_input(move |v| Msg::RuleText(index, RuleTextField::ReplyTopic, v)),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                    row![
                        text(t("回复内容", "Reply payload")).size(fs(11.0)).color(c.text_faint),
                        text_input("", &rule.reply_payload)
                            .size(fs(13.0))
                            .padding([5, 7])
                            .width(Length::Fill)
                            .style(c.input_style())
                            .on_input(move |v| Msg::RuleText(index, RuleTextField::ReplyPayload, v)),
                        pick(c, &qos_opts(), rule.qos, move |v| Msg::RuleNum(index, v)),
                        checkbox(rule.retain)
                            .label("Retain")
                            .size(fs(15.0))
                            .text_size(fs(12.0))
                            .style(c.checkbox_style())
                            .on_toggle(move |v| Msg::RuleRetain(index, v)),
                        pick(c, &format_opts(), rule.format, move |v| Msg::RuleFormat(index, v)),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                ]
                .spacing(6),
            )
            .padding(10)
            .style(c.inset());
            list = list.push(card);
        }
        if self.rt.form.rules.is_empty() {
            list = list.push(self.hint(
                c,
                t("尚无规则，点击右上角新增", "No rules yet — use Add rule"),
            ));
        }

        scrollable(
            column![
                row![
                    self.section(c, &t("自动回复", "Auto reply")),
                    Space::new().width(Length::Fill),
                    text(t("自身回投", "Own echo")).size(fs(11.0)).color(c.text_faint),
                    pick(c, &echo_opts(), self.rt.form.auto_reply_echo, |v| {
                        Msg::Num(Num::Echo, v)
                    }),
                    button(text(t("重置计数", "Reset hits")).size(fs(11.0)))
                        .padding([5, 9])
                        .style(c.ghost_button())
                        .on_press(Msg::ResetRuleStats),
                    button(text(t("新增规则", "Add rule")).size(fs(12.0)))
                        .padding([5, 10])
                        .style(c.primary_button())
                        .on_press(Msg::AddRule),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                list,
                self.hint(
                    c,
                    t(
                        "按顺序匹配，首条可回复的规则生效；间隔秒数内同一规则不会重复回复，↑↓ 调整优先级",
                        "Rules run in order; a rule inside its interval is skipped so later rules can answer. ↑↓ reorders",
                    )
                ),
            ]
            .spacing(10)
            .padding(0),
        )
        .height(Length::Fill)
        .into()
    }

    fn page_presets(&self, c: &Colors) -> El<'_> {
        let mut list = column![].spacing(8);
        let count = self.rt.form.presets.len();
        for (index, preset) in self.rt.form.presets.iter().enumerate() {
            let card = container(
                column![
                    row![
                        row![
                            reorder_button(
                                c,
                                Icon::MoveTop,
                                index > 0,
                                ("移动到最顶", "Move to top"),
                                Msg::PresetMoveTop(index),
                            ),
                            reorder_button(
                                c,
                                Icon::ArrowUp,
                                index > 0,
                                ("上移一位", "Move up"),
                                Msg::PresetMoveUp(index),
                            ),
                            reorder_button(
                                c,
                                Icon::ArrowDown,
                                index + 1 < count,
                                ("下移一位", "Move down"),
                                Msg::PresetMoveDown(index),
                            ),
                            reorder_button(
                                c,
                                Icon::MoveBottom,
                                index + 1 < count,
                                ("移动到最低", "Move to bottom"),
                                Msg::PresetMoveBottom(index),
                            ),
                        ]
                        .spacing(1)
                        .align_y(Alignment::Center),
                        text_input(l("名称", "Name"), &preset.name)
                            .size(fs(13.0))
                            .padding([5, 7])
                            .width(140)
                            .style(c.input_style())
                            .on_input(move |v| Msg::PresetText(index, PresetTextField::Name, v)),
                        text_input("mqttc/mvp/hello", &preset.topic)
                            .size(fs(13.0))
                            .padding([5, 7])
                            .width(Length::Fill)
                            .style(c.input_style())
                            .on_input(move |v| Msg::PresetText(index, PresetTextField::Topic, v)),
                        pick(c, &qos_opts(), preset.qos, move |v| Msg::PresetNum(index, v)),
                        checkbox(preset.retain)
                            .label("Retain")
                            .size(fs(15.0))
                            .text_size(fs(12.0))
                            .style(c.checkbox_style())
                            .on_toggle(move |v| Msg::PresetRetain(index, v)),
                        pick(c, &format_opts(), preset.format, move |v| Msg::PresetFormat(index, v)),
                        tooltip(
                            button(text(t("发送", "Send")).size(fs(12.0)))
                                .padding([4, 8])
                                .style(c.primary_button())
                                .on_press(Msg::PublishPreset(index)),
                            container(
                                text(t(
                                    "立即发布该条的 Topic / 内容 / QoS / Retain",
                                    "Publish this row's topic, payload, QoS and retain now"
                                ))
                                .size(fs(11.0))
                                .color(c.text_primary),
                            )
                            .padding([4, 7])
                            .style(c.inset()),
                            tooltip::Position::Top,
                        )
                        .delay(Duration::from_millis(300)),
                        button(text("×").size(fs(13.0)))
                            .padding([3, 7])
                            .style(c.danger_button())
                            .on_press(Msg::DelPreset(index)),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                    text_input(l("内容", "Payload"), &preset.payload)
                        .size(fs(13.0))
                        .padding([5, 7])
                        .width(Length::Fill)
                        .style(c.input_style())
                        .on_input(move |v| Msg::PresetText(index, PresetTextField::Payload, v)),
                ]
                .spacing(5),
            )
            .padding(10)
            .style(c.inset());
            list = list.push(card);
        }
        if self.rt.form.presets.is_empty() {
            list = list.push(self.hint(
                c,
                t("尚无预设，点击右上角新增", "No presets yet — use Add preset"),
            ));
        }

        scrollable(
            column![
                row![
                    self.section(c, &t("预设列表", "Preset list")),
                    Space::new().width(Length::Fill),
                    button(text(t("新增预设", "Add preset")).size(fs(12.0)))
                        .padding([5, 10])
                        .style(c.primary_button())
                        .on_press(Msg::AddPreset),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                list,
                self.hint(
                    c,
                    t(
                        "预设按列表顺序显示在发布框；行首箭头可调整位置",
                        "Presets appear in list order in the publish strip; use the row arrows to reorder"
                    )
                ),
            ]
            .spacing(10)
            .padding(0),
        )
        .height(Length::Fill)
        .into()
    }

    fn page_about(&self, c: &Colors) -> El<'_> {
        let link = button(text("github.com/GordonWen/mqttc").size(fs(12.0)))
            .padding([2, 4])
            .style(c.link_button())
            .on_press(Msg::OpenRepo);

        container(
            column![
                text("MQTT Debug Client").size(fs(22.0)).color(c.text_title),
                text(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .size(fs(12.0))
                    .color(c.text_faint),
                Space::new().height(8),
                text(t(
                    "轻量级跨平台 MQTT 调试客户端，支持 Windows、Linux 和 macOS。",
                    "A lightweight, cross-platform MQTT debug client for Windows, Linux and macOS."
                ))
                .size(fs(13.0))
                .color(c.text_body)
                .width(Length::Fill)
                .wrapping(Wrapping::WordOrGlyph),
                text(t(
                    "支持 MQTT 3.1.1 / 5.0、TLS、订阅与发布、遗嘱消息、自动回复、定时发布，以及日志筛选和导出。",
                    "MQTT 3.1.1 / 5.0, TLS, subscribe and publish, last will, auto reply, timed publish, plus log filtering and export."
                ))
                .size(fs(12.0))
                .color(c.text_muted)
                .width(Length::Fill)
                .wrapping(Wrapping::WordOrGlyph),
                text(t(
                    "设计以轻量、易用为主：单个可执行文件，无需安装，配置和日志都保存在 exe 同目录。",
                    "Designed to stay light and simple: one executable, no installer, with config and logs stored next to it."
                ))
                .size(fs(12.0))
                .color(c.text_muted)
                .width(Length::Fill)
                .wrapping(Wrapping::WordOrGlyph),
                Space::new().height(8),
                row![
                    text("GitHub").size(fs(12.0)).color(c.text_faint),
                    link,
                ]
                .spacing(6)
                .align_y(Alignment::Center),
                self.hint(
                    c,
                    t(
                        "遇到问题或有建议，欢迎到 GitHub Issues 反馈。",
                        "Found an issue or have an idea? Open a GitHub Issue."
                    )
                    .to_string(),
                ),
                Space::new().height(4),
                self.hint(c, "License: MIT".to_string()),
            ]
            .spacing(8)
            .padding(8),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

/// Opens a link in the platform browser. Only compile-time constants are
/// passed in, never user or broker supplied text.
fn open_url(url: &'static str) {
    #[cfg(windows)]
    win32::open_url(url);
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// Global shortcuts + window file drops. Ctrl-modified keys are handled even
/// while an input has focus.
fn event_message(
    event: iced::Event,
    _status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Msg> {
    use iced::keyboard::{key::Named, Event as KeyEvent, Key};

    if let iced::Event::Window(iced::window::Event::FileDropped(path)) = event {
        return Some(Msg::DroppedFile(path));
    }
    let iced::Event::Keyboard(KeyEvent::KeyPressed { key, modifiers, .. }) = event else {
        return None;
    };

    if key == Key::Named(Named::Escape) {
        return Some(Msg::CancelDelete);
    }
    if modifiers.command() {
        if modifiers.shift()
            && matches!(&key, Key::Character(c) if c.eq_ignore_ascii_case("c"))
        {
            return Some(Msg::CopyRow);
        }
        if matches!(&key, Key::Character(c) if c.eq_ignore_ascii_case("f")) {
            return Some(Msg::FocusSearch);
        }
        if matches!(&key, Key::Character(c) if c.eq_ignore_ascii_case("l")) {
            return Some(Msg::ClearLogs);
        }
        if key == Key::Named(Named::Enter) {
            return Some(Msg::PublishShortcut);
        }
    }
    None
}

fn human_bytes(size: usize) -> String {
    if size < 1024 {
        format!("{size} B")
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{:.2} MB", size as f64 / (1024.0 * 1024.0))
    }
}

/// `t()` returns an owned `String`; `text_input` placeholders must borrow a
/// `&'static str`, so the small helper keeps both strings in the binary.
/// One inline preset button: a little over nine CJK glyphs including padding.
const PRESET_BUTTON_GAP: f32 = 4.0;

/// Estimated rendered width of a preset button for a caption: glyph budget
/// plus horizontal padding. Overestimating CJK keeps the row from overflowing.
fn caption_button_width(caption: &str) -> f32 {
    let units = caption.chars().map(char_units).sum::<usize>();
    fs(units as f32 * 6.5 + 20.0)
}

/// Header room for the “More / Hide presets” toggle, estimated from the
/// localized label so the wider English text cannot push it off the row.
fn fold_button_width() -> f32 {
    let label = t("更多预设 ▸", "More presets ▸");
    fs(20.0 + label.chars().map(char_units).sum::<usize>() as f32 * 6.5)
}

/// Greedily count how many natural-width buttons fit into `budget`, including
/// the gaps between them.
fn pack_button_count(widths: &[f32], budget: f32) -> usize {
    let mut used = 0.0f32;
    let mut count = 0usize;
    for &width in widths {
        let add = if count == 0 {
            width
        } else {
            PRESET_BUTTON_GAP + width
        };
        if used + add > budget {
            break;
        }
        used += add;
        count += 1;
    }
    count
}

/// Rough width in half-em units, so CJK names truncate sooner than ASCII ones.
fn char_units(ch: char) -> usize {
    if ch.is_ascii() {
        1
    } else {
        2
    }
}

fn ellipsize(input: &str, max_units: usize) -> (String, bool) {
    if input.chars().map(char_units).sum::<usize>() <= max_units {
        return (input.to_string(), false);
    }
    let mut budget = max_units.saturating_sub(1); // reserve room for the ellipsis
    let mut out = String::new();
    for ch in input.chars() {
        let units = char_units(ch);
        if units > budget {
            break;
        }
        out.push(ch);
        budget -= units;
    }
    out.push('…');
    (out, true)
}

/// Compact icon button for the four reorder controls at the head of a preset
/// row. Disabled at the list boundary so nothing moves in vain.
fn reorder_button<'a>(
    c: &Colors,
    icon: Icon,
    enabled: bool,
    tip: (&'static str, &'static str),
    msg: Msg,
) -> El<'a> {
    let color = if enabled { c.text_primary } else { c.text_faint };
    let button = button(
        container(IconCanvas::new(icon, color).widget(12.0))
            .width(Length::Fixed(20.0))
            .height(Length::Fixed(22.0))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .padding(0)
    .style(c.button_kind(ButtonKind::Ghost));
    let button = if enabled { button.on_press(msg) } else { button };
    tooltip(
        button,
        container(text(t(tip.0, tip.1)).size(fs(11.0)).color(c.text_primary))
            .padding([4, 7])
            .style(c.inset()),
        tooltip::Position::Top,
    )
    .delay(Duration::from_millis(300))
    .into()
}

/// Display caption plus the full text when the caption had to be shortened.
fn preset_caption(
    preset: &PresetRow,
    index: usize,
    max_units: usize,
) -> (String, Option<String>) {
    let base = if preset.name.trim().is_empty() {
        format!("{} {}", t("预设", "Preset"), index + 1)
    } else {
        preset.name.trim().to_string()
    };
    let (shown, truncated) = ellipsize(&base, max_units);
    (shown, truncated.then_some(base))
}

fn l(zh: &'static str, en: &'static str) -> &'static str {
    if is_en() {
        en
    } else {
        zh
    }
}

pub fn mono_font() -> iced::Font {
    iced::Font {
        family: iced::font::Family::Monospace,
        ..iced::Font::DEFAULT
    }
}

pub fn ui_font() -> iced::Font {
    #[cfg(target_os = "windows")]
    const FAMILY: &str = "Microsoft YaHei UI";
    #[cfg(target_os = "macos")]
    const FAMILY: &str = "PingFang SC";
    #[cfg(all(unix, not(target_os = "macos")))]
    const FAMILY: &str = "Noto Sans CJK SC";
    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    const FAMILY: &str = "sans-serif";

    iced::Font {
        family: iced::font::Family::Name(FAMILY),
        ..iced::Font::DEFAULT
    }
}

/// Window geometry helpers for the frameless shell.
#[cfg(windows)]
mod win32 {
    use raw_window_handle::RawWindowHandle;
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    struct MonitorInfo {
        size: u32,
        _monitor: Rect,
        work: Rect,
        _flags: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn MonitorFromWindow(hwnd: *mut c_void, flags: u32) -> *mut c_void;
        fn GetMonitorInfoW(monitor: *mut c_void, info: *mut MonitorInfo) -> i32;
        fn GetWindowRect(hwnd: *mut c_void, rect: *mut Rect) -> i32;
        fn SetWindowPos(
            hwnd: *mut c_void,
            after: *mut c_void,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
    }

    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: *mut c_void,
            attribute: u32,
            value: *const c_void,
            size: u32,
        ) -> i32;
    }

    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: *mut c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> *mut c_void;
    }

    const MONITOR_DEFAULTTONEAREST: u32 = 2;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    /// `DWMWA_WINDOW_CORNER_PREFERENCE` (Windows 11 22000+).
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWCP_ROUND: i32 = 2;

    fn hwnd(window: &dyn iced::window::Window) -> Option<*mut c_void> {
        match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as *mut c_void),
            _ => None,
        }
    }

    /// Opens an http(s) URL in the default browser. ShellExecute keeps the
    /// launch silent, unlike spawning a console process.
    pub fn open_url(url: &str) {
        fn wide(text: &str) -> Vec<u16> {
            text.encode_utf16().chain(std::iter::once(0)).collect()
        }
        let operation = wide("open");
        let file = wide(url);
        // SAFETY: both UTF-16 buffers are NUL-terminated for the call.
        unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            );
        }
    }

    /// Windows 11 rounds every top-level window; a frameless one is otherwise
    /// square. Windows 10 rejects the attribute, which is fine.
    pub fn round_corners(window: &dyn iced::window::Window) {
        if let Some(hwnd) = hwnd(window) {
            let preference = DWMWCP_ROUND;
            // SAFETY: hwnd belongs to the live iced window.
            unsafe {
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    &preference as *const i32 as *const c_void,
                    std::mem::size_of::<i32>() as u32,
                );
            }
        }
    }

    /// Fits the window to the monitor work area and returns the old rect so it
    /// can be restored. winit's own maximize doubles the size on high-DPI
    /// frameless windows, so the title bar drives this instead.
    pub fn maximize(window: &dyn iced::window::Window) -> Option<[i32; 4]> {
        let hwnd = hwnd(window)?;
        let mut current = Rect::default();
        // SAFETY: hwnd belongs to the live iced window.
        unsafe {
            if GetWindowRect(hwnd, &mut current) == 0 {
                return None;
            }
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if monitor.is_null() {
                return None;
            }
            let mut info = MonitorInfo {
                size: std::mem::size_of::<MonitorInfo>() as u32,
                _monitor: Rect::default(),
                work: Rect::default(),
                _flags: 0,
            };
            if GetMonitorInfoW(monitor, &mut info) == 0 {
                return None;
            }
            let work = info.work;
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                work.left,
                work.top,
                work.right - work.left,
                work.bottom - work.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        Some([current.left, current.top, current.right, current.bottom])
    }

    pub fn restore(window: &dyn iced::window::Window, rect: [i32; 4]) {
        if let Some(hwnd) = hwnd(window) {
            // SAFETY: hwnd belongs to the live iced window.
            unsafe {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    rect[0],
                    rect[1],
                    rect[2] - rect[0],
                    rect[3] - rect[1],
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The i18n switch is process-wide, so tests that pin a language and tests
    /// that construct an app (which applies the config language) take turns.
    pub(super) static LANG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Widget construction has no renderer dependency; building every page
    /// catches layout/borrow panics that the bench (always on page 0) misses.
    #[test]
    fn every_page_builds() {
        let mut app = App::for_test();
        for page in [0, 1, 2, 5] {
            app.page = page;
            let _page = app.view();
        }
    }

    /// The four presets must map back to their own index even after a
    /// hand-edited config lands between two of them.
    #[test]
    fn font_choices_round_trip() {
        for (index, (scale, _, _)) in FONT_CHOICES.iter().enumerate() {
            assert_eq!(font_index(*scale), index as i32);
        }
        assert_eq!(font_index(0.5), 0);
        assert_eq!(font_index(2.0), 3);
    }

    /// Every page must build at the largest preset without overflowing or
    /// tripping the scaled-size helpers.
    #[test]
    fn every_page_builds_at_large_text() {
        let mut app = App::for_test();
        app.rt.prefs.font_scale = FONT_SCALE_MAX;
        for page in [0, 1, 2, 5] {
            app.page = page;
            let _page = app.view();
        }
        assert_eq!(fs(13.0), 13.0 * FONT_SCALE_MAX);
        app.rt.prefs.font_scale = 1.0;
        let _ = app.view();
        assert_eq!(fs(13.0), 13.0);
    }

    /// Each session tab has its own body builder; exercise all of them with
    /// non-empty connection-level lists so the list branches are covered.
    #[test]
    fn every_session_tab_builds() {
        use mqttc_core::config::{AutoReplyRule, ConnectionConfig, SubscriptionConfig};
        use mqttc_core::form::FormState;

        let mut app = App::for_test();
        let mut conn = ConnectionConfig::new("test".into());
        conn.subscriptions.push(SubscriptionConfig {
            topic: "mqttc/test/#".into(),
            qos: 1,
            ..Default::default()
        });
        conn.publish_presets.push(Default::default());
        conn.auto_reply_rules.push(AutoReplyRule::default());
        app.rt.config.active_id = conn.id.clone();
        app.rt.config.connections.push(conn);
        app.rt.form = FormState::from_config(app.rt.active().unwrap());
        app.page = 1;
        for tab in [
            SessionTab::Logs,
            SessionTab::Subs,
            SessionTab::Rules,
            SessionTab::Presets,
        ] {
            app.session_tab = tab;
            let _page = app.view();
        }
    }

    /// Preset captions shorten by display width and keep the full text for the
    /// hover tooltip.
    #[test]
    fn preset_caption_ellipsizes_long_names() {
        let mut preset = PresetRow {
            name: "battery-voltage-report".into(),
            ..Default::default()
        };
        let (shown, full) = preset_caption(&preset, 0, 12);
        assert!(shown.ends_with('…'));
        assert!(shown.chars().count() < preset.name.chars().count());
        assert_eq!(full.as_deref(), Some("battery-voltage-report"));

        preset.name = "OTA".into();
        let (shown, full) = preset_caption(&preset, 0, 12);
        assert_eq!(shown, "OTA");
        assert!(full.is_none());
    }

    /// With the fold open, every preset (including the empty-name fallback)
    /// must build without touching the runtime.
    #[test]
    fn preset_palette_builds() {
        let mut app = App::for_test();
        app.rt.form.presets = (0..8)
            .map(|index| PresetRow {
                name: if index == 3 {
                    String::new()
                } else {
                    format!("preset-{index}")
                },
                topic: format!("demo/{index}"),
                payload: "x".into(),
                ..Default::default()
            })
            .collect();
        app.presets_open = true;
        app.page = 1;
        app.session_tab = SessionTab::Logs;
        let _page = app.view();
    }

    /// Folding open/closed is independent of how many presets fit the header.
    #[test]
    fn toggle_palette_with_three_presets() {
        let mut app = App::for_test();
        app.rt.form.presets = (0..3).map(|_| PresetRow::default()).collect();
        let _ = app.update(Msg::TogglePresetPalette);
        assert!(app.presets_open);
        let _ = app.update(Msg::TogglePresetPalette);
        assert!(!app.presets_open);
    }

    /// The four row arrows act on the shared form state and keep plain list
    /// order (no special pinned-first pass).
    #[test]
    fn preset_move_messages_reorder_the_list() {
        let mut app = App::for_test();
        app.rt.form.presets = ["a", "b", "c"]
            .iter()
            .map(|name| PresetRow {
                name: (*name).into(),
                ..Default::default()
            })
            .collect();
        let names = |app: &App| -> Vec<String> {
            app.rt.form.presets.iter().map(|p| p.name.clone()).collect()
        };

        let _ = app.update(Msg::PresetMoveTop(2));
        assert_eq!(names(&app), ["c", "a", "b"]);
        let _ = app.update(Msg::PresetMoveDown(0));
        assert_eq!(names(&app), ["a", "c", "b"]);
        let _ = app.update(Msg::PresetMoveUp(2));
        assert_eq!(names(&app), ["a", "b", "c"]);
        let _ = app.update(Msg::PresetMoveBottom(0));
        assert_eq!(names(&app), ["b", "c", "a"]);
    }

    /// Buttons follow their names: two-character names stay narrow, long ones
    /// stop at the capped budget instead of widening the strip.
    #[test]
    fn preset_buttons_size_to_their_names() {
        let short = PresetRow {
            name: "报警".into(),
            ..Default::default()
        };
        let long = PresetRow {
            name: "这是一条特别长的预设名称用来验证八到九个中文字符之后会被省略".into(),
            ..Default::default()
        };
        let (short_caption, short_full) = preset_caption(&short, 0, 18);
        let (long_caption, long_full) = preset_caption(&long, 0, 18);
        assert!(short_full.is_none());
        assert!(long_full.is_some());
        let short_width = caption_button_width(&short_caption);
        let long_width = caption_button_width(&long_caption);
        assert!(short_width < 70.0);
        assert!(long_width > short_width);
        assert!(long_width <= 140.0);
    }

    /// Header packing uses each button's own width, so short names leave room
    /// for more buttons than the old fixed slot did.
    #[test]
    fn pack_buttons_uses_each_natural_width() {
        let widths = [44.0, 44.0, 70.0, 122.0, 44.0];
        assert_eq!(pack_button_count(&widths, 100.0), 2);
        assert_eq!(pack_button_count(&widths, 92.0), 2);
        assert_eq!(pack_button_count(&widths, 91.0), 1);
        assert_eq!(pack_button_count(&widths, 0.0), 0);
        assert_eq!(pack_button_count(&[44.0, 44.0, 44.0], 140.0), 3);
        assert_eq!(pack_button_count(&[140.0], 120.0), 0);
    }

    /// Deleting the last preset while folded open must return to the form.
    #[test]
    fn deleting_last_open_preset_closes_palette() {
        let mut app = App::for_test();
        app.rt.form.presets = vec![PresetRow::default()];
        app.presets_open = true;
        let _ = app.update(Msg::DelPreset(0));
        assert!(!app.presets_open);
        assert!(app.rt.form.presets.is_empty());
    }

    /// Pick-list labels are rebuilt from the active language, so switching
    /// language must not leave "全部 / 黑" frozen in the dropdowns.
    #[test]
    fn option_labels_follow_language() {
        use mqttc_core::i18n::{set_lang, Lang};

        let _lang = LANG_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        set_lang(Lang::Zh);
        assert_eq!(skin_opts()[1].label, "黑");
        assert_eq!(kind_opts()[0].label, "全部");
        assert_eq!(echo_opts()[0].label, "显示");
        assert_eq!(font_opts()[3].label, "特大");

        set_lang(Lang::En);
        assert_eq!(skin_opts()[1].label, "Black");
        assert_eq!(kind_opts()[0].label, "All");
        assert_eq!(kind_opts()[4].label, "Errors");
        assert_eq!(echo_opts()[0].label, "Show");
        assert_eq!(font_opts()[3].label, "Extra large");

        set_lang(Lang::Zh);
    }

    /// Exercise the truncation path, the expanded detail panel, and the paused
    /// toolbar state without needing a broker.
    #[test]
    fn big_payload_detail_and_pause_build() {
        let mut app = App::for_test();
        let id = app.rt.active_id().unwrap();
        app.rt
            .push_log(&id, "RECV", "demo/large", vec![b'x'; 4096]);
        app.rt.push_log(&id, "SYS", "", b"system".to_vec());
        app.page = 1;
        app.session_tab = SessionTab::Logs;

        {
            let _page = app.view();
        }
        let seq = app.rt.visible_logs().last().unwrap().seq;

        app.detail_seq = Some(seq);
        {
            let _page = app.view();
        }
        // Detail-page format switching (Hex) must render without the global pref.
        app.detail_format = 2;
        {
            let _page = app.view();
        }

        // A row that was evicted / belongs to another connection.
        app.detail_seq = Some(u64::MAX);
        {
            let _page = app.view();
        }

        app.detail_seq = None;
        app.rt.toggle_logs_pause();
        {
            let _page = app.view();
        }
        app.rt.toggle_logs_pause();
        {
            let _page = app.view();
        }
    }

    /// Ctrl+Enter publishes; the focused editor handles the same keystroke
    /// first (inserting a newline), so the shortcut must undo that edit.
    #[test]
    fn publish_shortcut_undoes_the_editor_enter() {
        let mut app = App::for_test();
        let before = app.editor.text();
        let _ = app.update(Msg::PubEdit(text_editor::Action::Edit(
            text_editor::Edit::Enter,
        )));
        assert_ne!(app.editor.text(), before);
        let _ = app.update(Msg::PublishShortcut);
        assert_eq!(app.editor.text(), before);

        // A stale Enter is a real line break and must stay.
        let _ = app.update(Msg::PubEdit(text_editor::Action::Edit(
            text_editor::Edit::Enter,
        )));
        app.editor_enter = Some(EnterUndo {
            at: Instant::now() - Duration::from_secs(1),
            text: before.clone(),
            replaced_selection: false,
        });
        let _ = app.update(Msg::PublishShortcut);
        assert_ne!(app.editor.text(), before);

        // A selection replaced by the editor's Enter is restored as well.
        app.editor = text_editor::Content::with_text("hello");
        let _ = app.update(Msg::PubEdit(text_editor::Action::SelectAll));
        let _ = app.update(Msg::PubEdit(text_editor::Action::Edit(
            text_editor::Edit::Enter,
        )));
        assert_eq!(app.editor.text(), "\n");
        let _ = app.update(Msg::PublishShortcut);
        assert_eq!(app.editor.text(), "hello");
    }

    /// Auto-scroll off pins the visible page by value, not by index: once the
    /// ring buffer evicts those rows the page must stay put instead of sliding
    /// to the newest row, and the detail panel must still find them.
    #[test]
    fn frozen_log_page_survives_eviction() {
        let mut app = App::for_test();
        let id = app.rt.active_id().unwrap();
        app.rt.set_auto_scroll(false);
        for n in 0..LOG_RENDER_ROWS {
            app.rt
                .push_log(&id, "RECV", "demo", format!("row {n}").into_bytes());
        }
        app.freeze_logs(true);
        let frozen: Vec<u64> = app
            .log_freeze
            .as_ref()
            .expect("page should be pinned")
            .iter()
            .map(|log| log.seq)
            .collect();
        assert_eq!(frozen.len(), LOG_RENDER_ROWS);

        // Push past the ring-buffer limit so every pinned seq gets evicted.
        for n in 0..600 {
            app.rt
                .push_log(&id, "RECV", "demo", format!("later {n}").into_bytes());
        }
        let after: Vec<u64> = app
            .log_freeze
            .as_ref()
            .expect("page should stay pinned")
            .iter()
            .map(|log| log.seq)
            .collect();
        assert_eq!(after, frozen, "pinned page must not slide with the stream");

        let first = frozen[0];
        assert!(app.rt.active_log(first).is_none(), "row should be evicted");
        assert!(app.row_by_seq(first).is_some(), "freeze keeps it reachable");
        {
            let _page = app.view();
        }
    }

    /// Scrolling up by wheel or scrollbar while following the stream must
    /// turn auto-scroll off and pin the page the user moved to.
    #[test]
    fn scrolling_up_stops_auto_scroll() {
        let mut app = App::for_test();
        let id = app.rt.active_id().unwrap();
        app.rt.set_auto_scroll(true);
        for n in 0..10 {
            app.rt
                .push_log(&id, "RECV", "demo", format!("row {n}").into_bytes());
        }
        app.log_freeze = None;

        // Scrolling down to the end (offset 0 with anchor_bottom) keeps following.
        let _ = app.update(Msg::LogScrolled(0.0));
        assert!(app.rt.prefs.auto_scroll);
        assert!(app.log_freeze.is_none());

        let _ = app.update(Msg::LogScrolled(40.0));
        assert!(!app.rt.prefs.auto_scroll);
        assert_eq!(app.log_freeze.as_ref().map(Vec::len), Some(10));
        {
            let _page = app.view();
        }
    }

    /// Deleting a connection must go through a confirmation dialog; the modal
    /// must build for both an existing and an already-removed id.
    #[test]
    fn delete_confirmation_builds() {
        let mut app = App::for_test();
        let id = app.rt.active_id().unwrap();

        app.pending_delete = Some(id);
        {
            let _page = app.view();
        }

        app.pending_delete = Some("does-not-exist".into());
        {
            let _page = app.view();
        }

        let _ = app.update(Msg::CancelDelete);
        assert!(app.pending_delete.is_none());
    }
}
