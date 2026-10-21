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
pub fn armed() -> bool {
    ARMED.load(Ordering::Acquire)
}

/// Whether the window's graphics device was lost.
pub fn lost() -> bool {
    LOST.load(Ordering::Acquire)
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
