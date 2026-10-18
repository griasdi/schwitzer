//! Schwitzer's look: raised slabs in a recessed well. Two themes (Daylight and Night) follow the system
//! appearance; every colour of the app comes from the [`Tokens`] of the active theme, and the
//! style functions below are the only place that reads them for widgets.

use iced::border::Radius;
use iced::font::Weight;
use iced::theme::Palette;
use iced::theme::palette::{Extended, Pair};
use iced::widget::{
    button, container, markdown, overlay::menu, pick_list, scrollable, text_editor, text_input,
};
use iced::{Background, Border, Color, Font, Pixels, Shadow, Theme, Vector, color};

// ---- spacing and type ----------------------------------------------------------------------

pub const SP_1: f32 = 4.0;
pub const SP_2: f32 = 8.0;
pub const SP_3: f32 = 12.0;
pub const SP_4: f32 = 16.0;

/// Captions, tags, counters.
pub const TS_12: f32 = 12.0;
/// Body text and rows.
pub const TS_14: f32 = 14.0;
/// Page titles.
pub const TS_20: f32 = 20.0;

/// Corner radius of rows, inputs and buttons.
pub const RADIUS: f32 = 8.0;
/// Corner radius of the square stage lamps.
pub const RADIUS_LAMP: f32 = 3.0;

/// The bundled UI face, in the weights the app uses.
pub const FAMILY: &str = "Figtree";

pub const FONTS: [&[u8]; 4] = [
    include_bytes!("../assets/fonts/Figtree-Regular.ttf"),
    include_bytes!("../assets/fonts/Figtree-Medium.ttf"),
    include_bytes!("../assets/fonts/Figtree-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Figtree-Bold.ttf"),
];

pub const REGULAR: Font = Font::with_name(FAMILY);
pub const MEDIUM: Font = Font {
    weight: Weight::Medium,
    ..REGULAR
};
pub const SEMIBOLD: Font = Font {
    weight: Weight::Semibold,
    ..REGULAR
};

// ---- colours -------------------------------------------------------------------------------

/// A tag colour: fill and the text on it.
#[derive(Debug, Clone, Copy)]
pub struct Tag {
    pub fill: Color,
    pub text: Color,
}

/// The colours of one theme.
#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    /// The recessed well behind slabs: list, sessions, page bodies, inputs, pills.
    pub ground: Color,
    /// Slabs, bars, menus, tooltips.
    pub raised: Color,
    /// A slab or button under the pointer.
    pub hover: Color,
    /// A pressed slab or button; selection; active segment.
    pub selected: Color,
    /// Separators and input borders at rest.
    pub line: Color,
    /// Input borders under the pointer (3:1 on the ground).
    pub line_strong: Color,
    pub text: Color,
    /// Secondary text, 4.5:1 on every surface.
    pub muted: Color,
    /// Green: the action fill.
    pub primary: Color,
    pub primary_hover: Color,
    pub on_primary: Color,
    /// Green readable as text and focus ring.
    pub ink: Color,
    pub danger: Color,
    pub danger_soft: Color,
    pub on_danger: Color,
    pub shadow: Color,
    /// The soft under-edge shadow of raised slabs and buttons.
    pub edge: Color,
    /// Stage lamp colours.
    pub stage_new: Color,
    pub stage_idea: Color,
    pub stage_planning: Color,
    pub stage_pickable: Color,
    pub stage_running: Color,
    pub stage_closed: Color,
    pub stage_rejected: Color,
    /// Member tags, indexed by [`crate::app::logic::member_hue`].
    pub tags: [Tag; 6],
}

const DAYLIGHT: Tokens = Tokens {
    ground: color!(0xe9f0ea),
    raised: color!(0xffffff),
    hover: color!(0xf2f8f3),
    selected: color!(0xe2f1e5),
    line: color!(0xd9e5db),
    line_strong: color!(0x8fa394),
    text: color!(0x142019),
    muted: color!(0x56665a),
    primary: color!(0x2db34a),
    primary_hover: color!(0x27a142),
    on_primary: color!(0x04210c),
    ink: color!(0x1a7f35),
    danger: color!(0xc53a35),
    danger_soft: color!(0xfbe9e7),
    on_danger: color!(0xffffff),
    shadow: Color::from_rgba8(12, 28, 17, 0.18),
    edge: Color::from_rgba8(12, 28, 17, 0.14),
    stage_new: color!(0x7d8c80),
    stage_idea: color!(0x869589),
    stage_planning: color!(0xc98410),
    stage_pickable: color!(0x2f7fd6),
    stage_running: color!(0x22a340),
    stage_closed: color!(0x869589),
    stage_rejected: color!(0xd9473f),
    tags: [
        Tag {
            fill: color!(0xd8f3dc),
            text: color!(0x14532d),
        },
        Tag {
            fill: color!(0xdbeafe),
            text: color!(0x1e3a8a),
        },
        Tag {
            fill: color!(0xede4fd),
            text: color!(0x4c1d95),
        },
        Tag {
            fill: color!(0xffe8cc),
            text: color!(0x7c2d12),
        },
        Tag {
            fill: color!(0xfde2ef),
            text: color!(0x831843),
        },
        Tag {
            fill: color!(0xd5f2f0),
            text: color!(0x134e4a),
        },
    ],
};

const NIGHT: Tokens = Tokens {
    ground: color!(0x0d1310),
    raised: color!(0x1a231e),
    hover: color!(0x222d26),
    selected: color!(0x213629),
    line: color!(0x2a3a30),
    line_strong: color!(0x627568),
    text: color!(0xe4eee6),
    muted: color!(0x93a597),
    primary: color!(0x3dcb5c),
    primary_hover: color!(0x5ad876),
    on_primary: color!(0x03200b),
    ink: color!(0x5ed77a),
    danger: color!(0xf06a62),
    danger_soft: color!(0x3a1a19),
    on_danger: color!(0x2a0605),
    shadow: Color::from_rgba8(0, 0, 0, 0.45),
    edge: Color::from_rgba8(0, 0, 0, 0.55),
    stage_new: color!(0x7d9182),
    stage_idea: color!(0x6f8374),
    stage_planning: color!(0xf0a630),
    stage_pickable: color!(0x5aa6f5),
    stage_running: color!(0x3dcb5c),
    stage_closed: color!(0x738678),
    stage_rejected: color!(0xf06a62),
    tags: [
        Tag {
            fill: color!(0x1d4a2a),
            text: color!(0xc6f6d0),
        },
        Tag {
            fill: color!(0x1e3a6b),
            text: color!(0xd3e4ff),
        },
        Tag {
            fill: color!(0x3b2a6b),
            text: color!(0xe6dcff),
        },
        Tag {
            fill: color!(0x5a3416),
            text: color!(0xffe2c4),
        },
        Tag {
            fill: color!(0x5c1f3d),
            text: color!(0xffd9ea),
        },
        Tag {
            fill: color!(0x16494a),
            text: color!(0xcdf5f2),
        },
    ],
};

/// The stage lamp of a task: which state it is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lamp {
    Idea,
    New,
    Planning,
    Pickable,
    Running,
    Closed,
    Rejected,
}

/// The tokens of `theme`.
pub fn tokens(theme: &Theme) -> &'static Tokens {
    if theme.extended_palette().is_dark {
        &NIGHT
    } else {
        &DAYLIGHT
    }
}

/// The theme for the system appearance; no preference means dark.
pub fn for_mode(mode: iced::theme::Mode) -> Theme {
    match mode {
        iced::theme::Mode::Light => build("Schwitzer Daylight", &DAYLIGHT),
        _ => build("Schwitzer Night", &NIGHT),
    }
}

fn build(name: &'static str, t: &'static Tokens) -> Theme {
    Theme::custom_with_fn(
        name,
        Palette {
            background: t.ground,
            text: t.text,
            primary: t.primary,
            success: t.stage_running,
            warning: t.stage_planning,
            danger: t.danger,
        },
        move |p| {
            let mut e = Extended::generate(p);
            e.primary.base = Pair {
                color: t.primary,
                text: t.on_primary,
            };
            e.danger.base = Pair {
                color: t.danger,
                text: t.on_danger,
            };
            e
        },
    )
}

// ---- containers ----------------------------------------------------------------------------

fn surface(background: Color, text: Color) -> container::Style {
    container::Style {
        text_color: Some(text),
        background: Some(background.into()),
        ..container::Style::default()
    }
}

/// The recessed well behind the slabs.
pub fn ground(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    surface(t.ground, t.text)
}

/// The header bar, page bars and the status bar: raised panels.
pub fn bar(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    surface(t.raised, t.text)
}

/// A slab that is not a button: session rows, settings sections.
pub fn slab_static(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        text_color: Some(t.text),
        background: Some(t.raised.into()),
        border: Border {
            color: t.line,
            width: 1.0,
            radius: RADIUS.into(),
        },
        shadow: lift(t, button::Status::Active),
        snap: true,
    }
}

/// The open task or the draft of a new one: a resting slab with the Sage Edge border.
pub fn slab_open(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        text_color: Some(t.text),
        background: Some(t.raised.into()),
        border: Border {
            color: t.line_strong,
            width: 1.0,
            radius: RADIUS.into(),
        },
        shadow: lift(t, button::Status::Active),
        snap: true,
    }
}

/// A stage lamp: a small square, hollow while an idea or only proposed.
pub fn lamp(kind: Lamp) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let t = tokens(theme);
        let color = match kind {
            Lamp::Idea => t.stage_idea,
            Lamp::New => t.stage_new,
            Lamp::Planning => t.stage_planning,
            Lamp::Pickable => t.stage_pickable,
            Lamp::Running => t.stage_running,
            Lamp::Closed => t.stage_closed,
            Lamp::Rejected => t.stage_rejected,
        };
        match kind {
            Lamp::Idea | Lamp::New => container::Style {
                border: Border {
                    color,
                    width: 2.0,
                    radius: RADIUS_LAMP.into(),
                },
                ..container::Style::default()
            },
            _ => container::Style {
                background: Some(color.into()),
                border: Border {
                    radius: RADIUS_LAMP.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            },
        }
    }
}

/// A one-pixel separator.
pub fn rule(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(tokens(theme).line.into()),
        ..container::Style::default()
    }
}

/// A member's name tag.
pub fn tag(hue: usize) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let tag = tokens(theme).tags[hue % 6];
        container::Style {
            text_color: Some(tag.text),
            background: Some(tag.fill.into()),
            border: Border {
                radius: Radius::new(999.0),
                ..Border::default()
            },
            ..container::Style::default()
        }
    }
}

/// A quiet pill: counters, stage labels.
pub fn pill(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        text_color: Some(t.muted),
        background: Some(t.ground.into()),
        border: Border {
            color: t.line,
            width: 1.0,
            radius: Radius::new(999.0),
        },
        ..container::Style::default()
    }
}

/// The error strip.
pub fn alert(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        text_color: Some(t.text),
        background: Some(t.danger_soft.into()),
        border: Border {
            color: t.danger,
            width: 1.0,
            radius: RADIUS.into(),
        },
        ..container::Style::default()
    }
}

/// A tooltip bubble.
pub fn tooltip(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        text_color: Some(t.text),
        background: Some(t.raised.into()),
        border: Border {
            color: t.line,
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow {
            color: t.shadow,
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        snap: true,
    }
}

/// The online dot: green when up, red-ish when down, line colour when unknown.
pub fn dot(state: Option<bool>) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let t = tokens(theme);
        let fill = match state {
            Some(true) => t.stage_running,
            Some(false) => t.danger,
            None => t.line_strong,
        };
        container::Style {
            background: Some(fill.into()),
            border: Border {
                radius: Radius::new(999.0),
                ..Border::default()
            },
            ..container::Style::default()
        }
    }
}

// ---- text colours --------------------------------------------------------------------------

pub fn text_muted(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(tokens(theme).muted),
    }
}

pub fn text_ink(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(tokens(theme).ink),
    }
}

pub fn text_danger(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(tokens(theme).danger),
    }
}

// ---- buttons -------------------------------------------------------------------------------

fn face(background: Option<Color>, text: Color, border: Border, shadow: Shadow) -> button::Style {
    button::Style {
        background: background.map(Background::from),
        text_color: text,
        border,
        shadow,
        snap: true,
    }
}

/// The under-edge shadow of a raised slab or button: deeper on hover, gone when pressed.
fn lift(t: &Tokens, status: button::Status) -> Shadow {
    match status {
        button::Status::Active => Shadow {
            color: t.edge,
            offset: Vector::new(0.0, 1.0),
            blur_radius: 2.0,
        },
        button::Status::Hovered => Shadow {
            color: t.edge,
            offset: Vector::new(0.0, 2.0),
            blur_radius: 4.0,
        },
        button::Status::Pressed | button::Status::Disabled => Shadow::default(),
    }
}

fn rounded() -> Border {
    Border {
        radius: RADIUS.into(),
        ..Border::default()
    }
}

/// The one call to action of a view: green.
pub fn button_primary(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let shadow = lift(t, status);
    match status {
        button::Status::Active => face(Some(t.primary), t.on_primary, rounded(), shadow),
        button::Status::Hovered => face(Some(t.primary_hover), t.on_primary, rounded(), shadow),
        button::Status::Pressed => face(Some(t.ink), t.on_primary, rounded(), shadow),
        button::Status::Disabled => face(Some(t.line), t.muted, rounded(), shadow),
    }
}

/// A bordered secondary button.
pub fn button_secondary(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let shadow = lift(t, status);
    let border = |color| Border {
        color,
        width: 1.0,
        radius: RADIUS.into(),
    };
    match status {
        button::Status::Active => face(Some(t.raised), t.text, border(t.line), shadow),
        button::Status::Hovered => face(Some(t.hover), t.text, border(t.line_strong), shadow),
        button::Status::Pressed => face(Some(t.selected), t.text, border(t.line_strong), shadow),
        button::Status::Disabled => face(None, t.muted, border(t.line), shadow),
    }
}

/// A destructive button: red text, red fill on hover.
pub fn button_danger(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let shadow = lift(t, status);
    let border = Border {
        color: t.danger,
        width: 1.0,
        radius: RADIUS.into(),
    };
    match status {
        button::Status::Active => face(None, t.danger, border, shadow),
        button::Status::Hovered => face(Some(t.danger_soft), t.danger, border, shadow),
        button::Status::Pressed => face(Some(t.danger), t.on_danger, border, shadow),
        button::Status::Disabled => face(None, t.muted, rounded(), shadow),
    }
}

/// Borderless: icon buttons in the bars, text links.
pub fn button_ghost(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let none = Shadow::default();
    match status {
        button::Status::Active => face(None, t.text, rounded(), none),
        button::Status::Hovered => face(Some(t.hover), t.text, rounded(), none),
        button::Status::Pressed => face(Some(t.selected), t.text, rounded(), none),
        button::Status::Disabled => face(None, t.muted, rounded(), none),
    }
}

/// A flat row inside a slab, such as a room in the settings.
pub fn button_row(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let none = Shadow::default();
    match status {
        button::Status::Active | button::Status::Disabled => face(None, t.text, rounded(), none),
        button::Status::Hovered => face(Some(t.hover), t.text, rounded(), none),
        button::Status::Pressed => face(Some(t.selected), t.text, rounded(), none),
    }
}

/// A group header in the list: flat in the well until hovered.
pub fn button_header(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let none = Shadow::default();
    match status {
        button::Status::Active | button::Status::Disabled => face(None, t.text, rounded(), none),
        button::Status::Hovered => face(Some(t.hover), t.text, rounded(), none),
        button::Status::Pressed => face(Some(t.selected), t.text, rounded(), none),
    }
}

/// A task slab: a raised full-width plate that sinks in when pressed.
pub fn slab(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let (background, border) = match status {
        button::Status::Active | button::Status::Disabled => (t.raised, t.line),
        button::Status::Hovered => (t.hover, t.line_strong),
        button::Status::Pressed => (t.selected, t.line_strong),
    };
    face(
        Some(background),
        t.text,
        Border {
            color: border,
            width: 1.0,
            radius: RADIUS.into(),
        },
        lift(t, status),
    )
}

/// A segment of a segmented control; the active one sits pressed in.
pub fn button_segment(on: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let t = tokens(theme);
        let shadow = if on {
            Shadow::default()
        } else {
            lift(t, status)
        };
        let border = Border {
            color: if on { t.ink } else { t.line },
            width: 1.0,
            radius: RADIUS.into(),
        };
        match (on, status) {
            (true, _) => face(Some(t.selected), t.ink, border, shadow),
            (false, button::Status::Hovered) => face(Some(t.hover), t.text, border, shadow),
            (false, button::Status::Pressed) => face(Some(t.selected), t.text, border, shadow),
            (false, _) => face(None, t.text, border, shadow),
        }
    }
}

// ---- inputs --------------------------------------------------------------------------------

pub fn input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let t = tokens(theme);
    let (border, width) = match status {
        text_input::Status::Active => (t.line, 1.0),
        text_input::Status::Hovered => (t.line_strong, 1.0),
        text_input::Status::Focused { .. } => (t.ink, 2.0),
        text_input::Status::Disabled => (t.line, 1.0),
    };
    text_input::Style {
        background: if matches!(status, text_input::Status::Disabled) {
            t.ground.into()
        } else {
            t.raised.into()
        },
        border: Border {
            color: border,
            width,
            radius: RADIUS.into(),
        },
        icon: t.muted,
        placeholder: t.muted,
        value: t.text,
        selection: t.selected,
    }
}

pub fn editor(theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let t = tokens(theme);
    let (border, width) = match status {
        text_editor::Status::Active => (t.line, 1.0),
        text_editor::Status::Hovered => (t.line_strong, 1.0),
        text_editor::Status::Focused { .. } => (t.ink, 2.0),
        text_editor::Status::Disabled => (t.line, 1.0),
    };
    text_editor::Style {
        background: t.raised.into(),
        border: Border {
            color: border,
            width,
            radius: RADIUS.into(),
        },
        placeholder: t.muted,
        value: t.text,
        selection: t.selected,
    }
}

/// The room switcher in the header bar: flat until hovered.
pub fn picker(theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let t = tokens(theme);
    let (background, border) = match status {
        pick_list::Status::Active => (t.raised, t.raised),
        pick_list::Status::Hovered => (t.hover, t.line_strong),
        pick_list::Status::Opened { .. } => (t.selected, t.ink),
    };
    pick_list::Style {
        text_color: t.text,
        placeholder_color: t.muted,
        handle_color: t.muted,
        background: background.into(),
        border: Border {
            color: border,
            width: 1.0,
            radius: RADIUS.into(),
        },
    }
}

pub fn picker_menu(theme: &Theme) -> menu::Style {
    let t = tokens(theme);
    menu::Style {
        background: t.raised.into(),
        border: Border {
            color: t.line,
            width: 1.0,
            radius: RADIUS.into(),
        },
        text_color: t.text,
        selected_text_color: t.on_primary,
        selected_background: t.primary.into(),
        shadow: Shadow {
            color: t.shadow,
            offset: Vector::new(0.0, 4.0),
            blur_radius: 16.0,
        },
    }
}

/// Thin rails that only show the scroller, darker while used.
pub fn scroll(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let t = tokens(theme);
    let mut style = scrollable::default(theme, status);
    let (vertical, horizontal) = match status {
        scrollable::Status::Active { .. } => (t.line, t.line),
        scrollable::Status::Hovered {
            is_vertical_scrollbar_hovered: v,
            is_horizontal_scrollbar_hovered: h,
            ..
        } => (
            if v { t.line_strong } else { t.line },
            if h { t.line_strong } else { t.line },
        ),
        scrollable::Status::Dragged {
            is_vertical_scrollbar_dragged: v,
            is_horizontal_scrollbar_dragged: h,
            ..
        } => (
            if v { t.muted } else { t.line },
            if h { t.muted } else { t.line },
        ),
    };
    for (rail, color) in [
        (&mut style.vertical_rail, vertical),
        (&mut style.horizontal_rail, horizontal),
    ] {
        rail.background = None;
        rail.border = Border::default();
        rail.scroller.background = color.into();
        rail.scroller.border = Border {
            radius: Radius::new(999.0),
            ..Border::default()
        };
    }
    style
}

/// Plans in the open task: body size, the UI face, headings one step above the body.
pub fn plan_markdown(theme: &Theme) -> markdown::Settings {
    let t = tokens(theme);
    let mut style = markdown::Style::from_palette(theme.palette());
    style.font = REGULAR;
    style.link_color = t.ink;
    style.inline_code_color = t.text;
    style.inline_code_highlight = iced::advanced::text::Highlight {
        background: t.ground.into(),
        border: Border {
            color: t.line,
            width: 1.0,
            radius: 4.0.into(),
        },
    };
    let mut settings = markdown::Settings::with_text_size(TS_14, style);
    settings.h1_size = Pixels(TS_20);
    settings.h2_size = Pixels(17.0);
    settings.h3_size = Pixels(15.0);
    settings.h4_size = Pixels(TS_14);
    settings.h5_size = Pixels(TS_14);
    settings.h6_size = Pixels(TS_14);
    settings.spacing = Pixels(SP_2);
    settings
}
