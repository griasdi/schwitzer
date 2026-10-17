//! schwitzer: the desktop app of the task rooms for coding agents and their humans.
// A release build is a GUI program on Windows, so starting it opens no console. Debug builds
// keep the console for the log, and `remote` builds need it: `ctl` and `--headless` report on
// stdout and stderr.
#![cfg_attr(
    all(windows, not(debug_assertions), not(feature = "remote")),
    windows_subsystem = "windows"
)]

mod app;
mod bridge;
mod mcp;
mod sessions;
mod settings;
mod theme;

use app::App;
#[cfg(feature = "remote")]
use app::Message;

fn main() -> iced::Result {
    // The log goes to stderr: `ctl` prints its replies on stdout. iced_winit logs every byte
    // of the window icon at info.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,iced_winit=warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let mut app = iced::application(App::boot, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .default_font(theme::REGULAR)
        // A narrow buddy-list window beside the editor.
        .window(iced::window::Settings {
            size: iced::Size::new(400.0, 780.0),
            min_size: Some(iced::Size::new(340.0, 520.0)),
            ..iced::window::Settings::default()
        });
    for font in theme::FONTS {
        app = app.font(font);
    }

    #[cfg(feature = "remote")]
    {
        iced_impeccable::Remote::new(app)
            // The clock and the session scan mean "time passed", not activity, for `wait-idle`.
            .heartbeat(|m| matches!(m, Message::Tick | Message::LocalSessions(_)))
            .run()
    }
    #[cfg(not(feature = "remote"))]
    {
        app.run()
    }
}
