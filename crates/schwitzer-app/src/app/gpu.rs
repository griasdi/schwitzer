//! Notices when the graphics device is lost (a driver update or reset, a GPU that went away).
//! iced never recreates its wgpu device and wgpu drops the errors of a lost one, so the window
//! would stay empty; the app restarts instead (`App::handle`, `Message::Tick`).

use std::sync::atomic::{AtomicBool, Ordering};

use iced::widget::shader;
use iced::{Element, Length, Rectangle, mouse, wgpu};

/// Whether the device-lost callback is registered on the window's device.
static ARMED: AtomicBool = AtomicBool::new(false);
/// Whether the device was lost (not destroyed on purpose).
static LOST: AtomicBool = AtomicBool::new(false);

/// Whether a wgpu device took the probe and will report its loss.
fn armed() -> bool {
    ARMED.load(Ordering::Acquire)
}

/// Whether the window's graphics device was lost.
fn lost() -> bool {
    LOST.load(Ordering::Acquire)
}

/// How long after start the view carries the device-lost probe when no wgpu device took it
/// (tiny-skia).
const PROBE_MS: u64 = 5_000;
/// A process restarts for a lost graphics device only after running this long: at most one
/// such restart per 10 s if the new device is lost again right away.
const RESTART_UPTIME_MS: u64 = 10_000;

/// When to show the probe and when to restart for a lost device, by this process's age.
#[derive(Debug)]
pub struct Watch {
    started_ms: u64,
    /// Whether the restart is under way.
    restarting: bool,
}

impl Watch {
    /// A watch for a process that started at `now_ms`.
    pub fn new(now_ms: u64) -> Self {
        Self {
            started_ms: now_ms,
            restarting: false,
        }
    }

    /// Whether the view carries the probe at `now_ms`: no device took it yet and the process
    /// is young.
    pub fn probe_wanted(&self, now_ms: u64) -> bool {
        !armed() && now_ms < self.started_ms + PROBE_MS
    }

    /// Whether the app restarts now: the device was lost, no restart is under way and the
    /// process has run long enough. `true` at most once; the caller starts the restart.
    pub fn restart_due(&mut self, now_ms: u64) -> bool {
        if lost() && !self.restarting && now_ms >= self.started_ms + RESTART_UPTIME_MS {
            self.restarting = true;
            return true;
        }
        false
    }
}

/// An invisible 1×1 shader at the window's top left corner, which draws nothing: the first
/// frame that has it hands wgpu's device to `Pipeline::new`, which registers the callback.
/// A widget of zero size would never be drawn (iced skips what lies outside the viewport).
/// Not drawn on tiny-skia (no device there).
pub fn probe<'a, Message: 'a>() -> Element<'a, Message> {
    shader(Probe)
        .width(Length::Fixed(1.0))
        .height(Length::Fixed(1.0))
        .into()
}

struct Probe;

impl<Message> shader::Program<Message> for Probe {
    type State = ();
    type Primitive = Primitive;

    fn draw(&self, _state: &(), _cursor: mouse::Cursor, _bounds: Rectangle) -> Primitive {
        Primitive
    }
}

#[derive(Debug)]
struct Primitive;

impl shader::Primitive for Primitive {
    type Pipeline = Pipeline;

    fn prepare(
        &self,
        _pipeline: &mut Pipeline,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &shader::Viewport,
    ) {
    }
}

struct Pipeline;

impl shader::Pipeline for Pipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, _format: wgpu::TextureFormat) -> Self {
        device.set_device_lost_callback(|reason, message| {
            // `Destroyed` comes only from `Device::destroy`, which nothing here calls.
            if reason == wgpu::DeviceLostReason::Unknown {
                tracing::error!("graphics device lost: {message}");
                LOST.store(true, Ordering::Release);
            }
        });
        ARMED.store(true, Ordering::Release);
        tracing::debug!("graphics device-lost callback registered");
        Pipeline
    }
}
