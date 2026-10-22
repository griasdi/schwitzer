//! The About page: Schwitzer's own license, those of the bundled font and icons, and those of
//! the open source components, each unfolding in place.

use iced::widget::{column, container, row, text};
use iced::{Alignment, Element, Length};

use super::icons::{Icon, Tone, icon};
use super::pages::{frame, section};
use super::widgets::{heading, id_button, muted};
use super::{App, Message};
use crate::about::{self, Group};
use crate::theme::{self, SP_1, SP_2, SP_3, SP_4, TS_12, TS_14};
use crate::update;

pub fn view(app: &App) -> Element<'_, Message> {
    let body = column![
        section(group(
            app,
            Group::Schwitzer,
            "Schwitzer",
            format!(
                "Schwitzer {}. {}. You may use it under the MIT License or the Apache License \
                 2.0, at your option.",
                update::CURRENT,
                about::COPYRIGHT
            ),
        )),
        section(group(
            app,
            Group::Assets,
            "Fonts and icons",
            "The app bundles the Figtree typeface and Lucide icons.".to_owned(),
        )),
        section(group(
            app,
            Group::Components,
            "Open source components",
            format!(
                "Schwitzer is built from {} open source components. Each entry is one license \
                 text and the components under it.",
                about::component_count()
            ),
        )),
    ]
    .spacing(SP_4);
    frame("licenses-page", "About", body.into(), None)
}

/// The entries of `group`, each a row that unfolds its sources and text below it.
fn group<'a>(
    app: &'a App,
    group: Group,
    title: &'static str,
    intro: String,
) -> Element<'a, Message> {
    let mut rows = column![].spacing(2);
    for (i, entry) in about::entries().iter().enumerate() {
        if entry.group != group {
            continue;
        }
        let open = app.license_open == Some(i);
        let chevron = if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        let mut lines = column![text(entry.title.as_ref()).size(TS_14)]
            .spacing(2)
            .width(Length::Fill);
        if let Some(caption) = &entry.caption {
            lines = lines.push(muted(caption.as_str()));
        }
        rows = rows.push(id_button(
            format!("license-{i}"),
            container(
                row![icon(chevron, 14.0, Tone::Muted), lines]
                    .spacing(SP_2)
                    .align_y(Alignment::Center),
            )
            .padding([6.0, SP_2]),
            Some(Message::ToggleLicense(i)),
            theme::button_row,
            Length::Fill,
        ));
        if open {
            let mut unfolded = column![].spacing(SP_2);
            for source in &entry.sources {
                unfolded = unfolded.push(muted(source.as_str()));
            }
            unfolded = unfolded.push(text(entry.text.as_ref()).size(TS_12));
            rows = rows.push(
                container(unfolded)
                    .padding([SP_1, SP_2])
                    .width(Length::Fill),
            );
        }
    }
    column![heading(title), muted(intro), rows]
        .spacing(SP_3)
        .into()
}
