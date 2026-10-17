//! Glue between the client library and iced: the event subscription and the async calls
//! whose results feed back into `Message`.

use std::hash::{Hash, Hasher};

use futures::{SinkExt, Stream};
use iced::Task;
use schwitzer_client::Client;
use tokio::sync::broadcast::error::RecvError;

use crate::app::Message;

/// Identity of the client subscription: one per process, so the constant hash keeps the
/// stream alive across view rebuilds.
#[derive(Clone)]
pub struct ClientKey(pub Client);

impl Hash for ClientKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        0x7377_6974_u32.hash(state);
    }
}

/// Stream of client events as application messages. A receiver that lags asks for a
/// [`Message::Resync`].
pub fn client_events(key: &ClientKey) -> impl Stream<Item = Message> + use<> {
    let client = key.0.clone();
    iced::stream::channel(256, async move |mut out| {
        let mut rx = client.subscribe();
        loop {
            let message = match rx.recv().await {
                Ok(event) => Message::Client(event),
                Err(RecvError::Lagged(_)) => Message::Resync,
                Err(RecvError::Closed) => break,
            };
            if out.send(message).await.is_err() {
                break;
            }
        }
    })
}

/// Runs a client call and maps its result.
pub fn call<T, E, F>(
    fut: F,
    map: impl Fn(Result<T, String>) -> Message + Send + 'static,
) -> Task<Message>
where
    T: Send + 'static,
    E: std::fmt::Display,
    F: Future<Output = Result<T, E>> + Send + 'static,
{
    Task::perform(async move { fut.await.map_err(|e| e.to_string()) }, map)
}

/// Runs a client call whose value is ignored; failures raise the banner.
pub fn fire<T, E, F>(fut: F) -> Task<Message>
where
    T: Send + 'static,
    E: std::fmt::Display,
    F: Future<Output = Result<T, E>> + Send + 'static,
{
    call(fut, |r| match r {
        Ok(_) => Message::Noop,
        Err(e) => Message::Failed(e),
    })
}
