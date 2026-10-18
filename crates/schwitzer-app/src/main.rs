//! schwitzer: the desktop app of the task rooms for coding agents and their humans.
// A release build is a GUI program on Windows, so starting it opens no console. Debug builds
// keep the console for the log, and `remote` builds need it: `ctl` and `--headless` report on
// stdout and stderr.
#![cfg_attr(
    all(windows, not(debug_assertions), not(feature = "remote")),
    windows_subsystem = "windows"
)]

mod about;
mod app;
mod brand;
mod bridge;
mod mcp;
mod omp_plugin;
mod sessions;
mod settings;
mod theme;
mod update;
mod wsl;

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

    // Before the window opens: after a restart for an update it opens where the old one was.
    let resumed = app::Resumed::take();
    // A narrow window beside the editor.
    let window = resumed.place(iced::window::Settings {
        size: iced::Size::new(400.0, 780.0),
        min_size: Some(iced::Size::new(340.0, 520.0)),
        icon: brand::window_icon(),
        // Matches `StartupWMClass` in deploy/schwitzer.desktop: desktops find the icon by it.
        #[cfg(target_os = "linux")]
        platform_specific: iced::window::settings::PlatformSpecific {
            application_id: "schwitzer".into(),
            ..Default::default()
        },
        ..iced::window::Settings::default()
    });
    let mut app = iced::application(move || App::boot(&resumed), App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .default_font(theme::REGULAR)
        .window(window);
    for font in theme::FONTS {
        app = app.font(font);
    }

    #[cfg(feature = "remote")]
    {
        iced_impeccable::Remote::new(app)
            // The clock, the session scan, the update check and the wait for a restored focus
            // mean "time passed", not activity, for `wait-idle`.
            .heartbeat(|m| {
                matches!(
                    m,
                    Message::Tick
                        | Message::LocalSessions(_)
                        | Message::CheckUpdate
                        | Message::ResumeFocus(false)
                )
            })
            .run()
    }
    #[cfg(not(feature = "remote"))]
    {
        app.run()
    }
}
