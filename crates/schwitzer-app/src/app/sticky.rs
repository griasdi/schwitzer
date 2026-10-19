//! A head that pins to the top of a scrolled viewport while the element it heads is in view.

use std::cell::Cell;

use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::widget::container;
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

/// Lays out `head` above `rest`. When the viewport's top passes `head`, it draws `head` at the
/// viewport's top on a `style` plate, at most until `rest` ends.
pub struct Sticky<'a, Message> {
    /// `[head, rest]`.
    children: [Element<'a, Message>; 2],
    /// The plate behind the head while pinned.
    style: fn(&Theme) -> container::Style,
}

/// `head` above `rest`, `head` pinned on a `style` plate while scrolled past.
pub fn sticky<'a, Message>(
    head: impl Into<Element<'a, Message>>,
    rest: impl Into<Element<'a, Message>>,
    style: fn(&Theme) -> container::Style,
) -> Sticky<'a, Message> {
    Sticky {
        children: [head.into(), rest.into()],
        style,
    }
}

#[derive(Default)]
struct State {
    /// The head's layout, at the origin; the widget's own node holds only the rest.
    head: layout::Node,
    /// How far the head was last drawn below its place, for `operate`.
    lift: Cell<f32>,
}

/// How far the head moves down from its place: to the viewport's top, at most until it meets
/// the bottom of `bounds`.
fn lift(bounds: Rectangle, head_height: f32, viewport: &Rectangle) -> f32 {
    (viewport.y - bounds.y).clamp(0.0, (bounds.height - head_height).max(0.0))
}

/// The head's layout, `lift` below its place.
fn head_layout<'b>(layout: Layout<'_>, state: &'b State, lift: f32) -> Layout<'b> {
    let p = layout.position();
    Layout::with_offset(Vector::new(p.x, p.y + lift), &state.head)
}

/// The cursor the rest sees: none while it lies over the pinned head.
fn rest_cursor(cursor: mouse::Cursor, head: Rectangle, lift: f32) -> mouse::Cursor {
    if lift > 0.0 && cursor.is_over(head) {
        mouse::Cursor::Unavailable
    } else {
        cursor
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for Sticky<'_, Message> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let Tree {
            state, children, ..
        } = tree;
        let width = limits.max().width;
        let child = layout::Limits::new(Size::ZERO, Size::new(width, limits.max().height));
        let [head, rest] = &mut self.children;
        let head = head
            .as_widget_mut()
            .layout(&mut children[0], renderer, &child);
        let head_h = head.size().height;
        let rest = rest
            .as_widget_mut()
            .layout(&mut children[1], renderer, &child)
            .move_to(Point::new(0.0, head_h));
        let rest_h = rest.size().height;
        state.downcast_mut::<State>().head = head;
        layout::Node::with_children(Size::new(width, head_h + rest_h), vec![rest])
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;

        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_ref::<State>();
        let l = lift(layout.bounds(), state.head.size().height, viewport);
        state.lift.set(l);
        let head = head_layout(layout, state, l);
        self.children[1].as_widget().draw(
            &children[1],
            renderer,
            theme,
            style,
            layout.child(0),
            rest_cursor(cursor, head.bounds(), l),
            viewport,
        );
        if l == 0.0 {
            self.children[0].as_widget().draw(
                &children[0],
                renderer,
                theme,
                style,
                head,
                cursor,
                viewport,
            );
            return;
        }
        // Its own layer, so the rest's text, drawn after the quads of its layer, stays below.
        renderer.with_layer(*viewport, |renderer| {
            let plate = (self.style)(theme);
            if let Some(background) = plate.background {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: head.bounds(),
                        border: plate.border,
                        shadow: plate.shadow,
                        snap: true,
                    },
                    background,
                );
            }
            let style = renderer::Style {
                text_color: plate.text_color.unwrap_or(style.text_color),
            };
            self.children[0].as_widget().draw(
                &children[0],
                renderer,
                theme,
                &style,
                head,
                cursor,
                viewport,
            );
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_ref::<State>();
        let l = lift(layout.bounds(), state.head.size().height, viewport);
        state.lift.set(l);
        let head = head_layout(layout, state, l);
        let [head_el, rest_el] = &mut self.children;
        head_el.as_widget_mut().update(
            &mut children[0],
            event,
            head,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        // Always forwarded, so keys and blur-on-click reach the editors; only the cursor is hidden.
        rest_el.as_widget_mut().update(
            &mut children[1],
            event,
            layout.child(0),
            rest_cursor(cursor, head.bounds(), l),
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_ref::<State>();
        let l = lift(layout.bounds(), state.head.size().height, viewport);
        let head = head_layout(layout, state, l);
        // At rest the head and the rest do not overlap; pinned, the head covers the rest.
        if cursor.is_over(head.bounds()) {
            self.children[0].as_widget().mouse_interaction(
                &children[0],
                head,
                cursor,
                viewport,
                renderer,
            )
        } else {
            self.children[1].as_widget().mouse_interaction(
                &children[1],
                layout.child(0),
                cursor,
                viewport,
                renderer,
            )
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_ref::<State>();
        let head = head_layout(layout, state, state.lift.get());
        let [head_el, rest_el] = &mut self.children;
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            head_el
                .as_widget_mut()
                .operate(&mut children[0], head, renderer, operation);
            rest_el
                .as_widget_mut()
                .operate(&mut children[1], layout.child(0), renderer, operation);
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_ref::<State>();
        let l = lift(
            layout.bounds() + translation,
            state.head.size().height,
            viewport,
        );
        let head = head_layout(layout, state, l);
        let [head_el, rest_el] = &mut self.children;
        let [head_tree, rest_tree] = &mut children[..] else {
            return None;
        };
        let overlays: Vec<_> = [
            head_el
                .as_widget_mut()
                .overlay(head_tree, head, renderer, viewport, translation),
            rest_el.as_widget_mut().overlay(
                rest_tree,
                layout.child(0),
                renderer,
                viewport,
                translation,
            ),
        ]
        .into_iter()
        .flatten()
        .collect();
        (!overlays.is_empty()).then(|| overlay::Group::with_children(overlays).overlay())
    }
}

impl<'a, Message: 'a> From<Sticky<'a, Message>> for Element<'a, Message> {
    fn from(sticky: Sticky<'a, Message>) -> Self {
        Element::new(sticky)
    }
}
