//! System notifications (notify-rust). They are shown off the UI thread; a failure is only
//! logged, since a missing notification daemon must not disturb the app.

/// Shows a system notification with `summary` and `body`. A headless instance shows none and
/// logs it instead.
pub async fn show(summary: String, body: String) {
    #[cfg(feature = "remote")]
    if iced_impeccable::is_headless() {
        tracing::info!("headless: notification not shown: {summary}: {body}");
        return;
    }
    let shown = tokio::task::spawn_blocking(move || {
        notify_rust::Notification::new()
            .appname("Schwitzer")
            .summary(&summary)
            .body(&body)
            .show()
            .map(drop)
    })
    .await;
    match shown {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::warn!("notification not shown: {e}"),
        Err(e) => tracing::warn!("notification task failed: {e}"),
    }
}
