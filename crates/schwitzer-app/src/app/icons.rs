//! Vendored Lucide icons (ISC, see `assets/icons/LICENSE`) and the app's status flower, drawn
//! with iced's `svg` widget and tinted from the theme.

use iced::widget::svg;
use iced::{Length, Theme};

use crate::theme::tokens;

macro_rules! icons {
    ($($variant:ident => $file:literal),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Icon {
            $($variant,)+
        }

        impl Icon {
            fn bytes(self) -> &'static [u8] {
                match self {
                    $(Self::$variant => include_bytes!(concat!("../../assets/icons/", $file, ".svg")),)+
                }
            }
        }
    };
}

icons! {
    Plus => "plus",
    Settings => "settings",
    Close => "x",
    ChevronDown => "chevron-down",
    ChevronRight => "chevron-right",
    Copy => "copy",
    Leave => "log-out",
    Folder => "folder",
    FolderPlus => "folder-plus",
    Trash => "trash-2",
    Check => "check",
    Pencil => "pencil",
    Users => "users",
    UserPlus => "user-plus",
    Plug => "plug",
    Refresh => "refresh-cw",
    Notes => "message-square",
    ArrowUp => "arrow-up",
    ArrowDown => "arrow-down",
    Pick => "hand-grab",
    Bot => "bot",
    Alert => "circle-alert",
    Plan => "file-text",
    Flower => "flower",
    FlowerOutline => "flower-outline",
}

/// The colour of a status flower: what state a task is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bloom {
    New,
    Idea,
    Planning,
    Pickable,
    Running,
    Closed,
    Rejected,
}

/// How an icon is coloured; resolved against the theme when drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Text,
    Muted,
    Ink,
    OnPrimary,
    Danger,
    Bloom(Bloom),
}

/// An icon of `size` logical pixels in `tone`.
pub fn icon<'a>(icon: Icon, size: f32, tone: Tone) -> svg::Svg<'a, Theme> {
    svg(svg::Handle::from_memory(icon.bytes()))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |theme: &Theme, _| {
            let t = tokens(theme);
            svg::Style {
                color: Some(match tone {
                    Tone::Text => t.text,
                    Tone::Muted => t.muted,
                    Tone::Ink => t.ink,
                    Tone::OnPrimary => t.on_primary,
                    Tone::Danger => t.danger,
                    Tone::Bloom(Bloom::New) => t.stage_new,
                    Tone::Bloom(Bloom::Idea) => t.stage_idea,
                    Tone::Bloom(Bloom::Planning) => t.stage_planning,
                    Tone::Bloom(Bloom::Pickable) => t.stage_pickable,
                    Tone::Bloom(Bloom::Running) => t.stage_running,
                    Tone::Bloom(Bloom::Closed) => t.stage_closed,
                    Tone::Bloom(Bloom::Rejected) => t.stage_rejected,
                }),
            }
        })
}
