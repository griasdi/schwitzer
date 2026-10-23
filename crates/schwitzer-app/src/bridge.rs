//! Glue between the client library and iced: the event subscription and the async calls
//! whose results feed back into `Message`.

use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, PoisonError};

use futures::{SinkExt, Stream};
use iced::Task;
use schwitzer_client::{Client, ClientEvent};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;

use crate::app::Message;

/// The client's events from start-up until the subscription first runs: subscribed
/// before any room is added, taken by [`client_events`], so nothing the client emits
/// while the window comes up is lost.
pub type Backlog = Arc<Mutex<Option<broadcast::Receiver<ClientEvent>>>>;

/// Identity of the client subscription: one per process, so the constant hash keeps the
/// stream alive across view rebuilds.
#[derive(Clone)]
pub struct ClientKey {
    pub client: Client,
    pub backlog: Backlog,
}

impl Hash for ClientKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        0x7377_6974_u32.hash(state);
    }
}

/// Stream of client events as application messages, starting with the events buffered
/// since start-up. A receiver that lags asks for a [`Message::Resync`]. Debug log bytes stay
/// out of the message loop: the debug worker reads them from its own receiver.
pub fn client_events(key: &ClientKey) -> impl Stream<Item = Message> + use<> {
    let client = key.client.clone();
    let backlog = key
        .backlog
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    iced::stream::channel(256, async move |mut out| {
        let mut rx = backlog.unwrap_or_else(|| client.subscribe());
        loop {
            let message = match rx.recv().await {
                Ok(ClientEvent::DebugLog { .. }) => continue,
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
