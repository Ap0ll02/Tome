use std::ops::Range;

use gpui::{
    App, Bounds, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    ShapedLine, SharedString, Style, TextRun, UTF16Selection, Window, actions, div, fill, point,
    prelude::*, px, relative, rgb, size,
};
use unicode_segmentation::UnicodeSegmentation;

fn safe_byte_range(text: &str, range: Range<usize>) -> Range<usize> {
    let mut start = range.start.min(text.len());
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = range.end.min(text.len()).max(start);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    start..end
}

fn byte_from_utf16(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units + ch.len_utf16() > offset {
            return byte;
        }
        units += ch.len_utf16();
    }
    text.len()
}

actions!(
    field,
    [
        Left,
        Right,
        SelectLeft,
        SelectRight,
        Backspace,
        Delete,
        SelectAll,
        Commit,
        Cancel
    ]
);

pub enum FieldEvent {
    Commit,
    Cancel,
}

pub struct TextField {
    focus: FocusHandle,
    text: SharedString,
    placeholder: SharedString,
    selection: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    line: Option<ShapedLine>,
    bounds: Option<Bounds<Pixels>>,
    selecting: bool,
    compact: bool,
}

impl EventEmitter<FieldEvent> for TextField {}

impl TextField {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            text: "".into(),
            placeholder: "Campaign name".into(),
            selection: 0..0,
            reversed: false,
            marked: None,
            line: None,
            bounds: None,
            selecting: false,
            compact: false,
        }
    }

    pub fn character_name(cx: &mut Context<Self>) -> Self {
        let mut field = Self::new(cx);
        field.placeholder = "Character name".into();
        field
    }

    pub fn value(&self) -> String {
        self.text.to_string()
    }

    pub fn with_value(cx: &mut Context<Self>, value: String, placeholder: &'static str) -> Self {
        let mut field = Self::new(cx);
        field.selection = value.len()..value.len();
        field.text = value.into();
        field.placeholder = placeholder.into();
        field
    }

    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    pub fn focus(&self) -> &FocusHandle {
        &self.focus
    }

    fn cursor(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    fn move_to(&mut self, at: usize, cx: &mut Context<Self>) {
        self.selection = at..at;
        self.reversed = false;
        cx.notify();
    }

    fn select_to(&mut self, at: usize, cx: &mut Context<Self>) {
        let anchor = if self.reversed {
            self.selection.end
        } else {
            self.selection.start
        };
        self.selection = anchor.min(at)..anchor.max(at);
        self.reversed = at < anchor;
        cx.notify();
    }

    fn previous(&self) -> usize {
        self.text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .take_while(|&i| i < self.cursor())
            .last()
            .unwrap_or(0)
    }

    fn next(&self) -> usize {
        self.text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .find(|&i| i > self.cursor())
            .unwrap_or(self.text.len())
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        let at = if self.selection.is_empty() {
            self.previous()
        } else {
            self.selection.start
        };
        self.move_to(at, cx);
    }
    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        let at = if self.selection.is_empty() {
            self.next()
        } else {
            self.selection.end
        };
        self.move_to(at, cx);
    }
    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous(), cx);
    }
    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next(), cx);
    }
    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selection.is_empty() {
            self.select_to(self.previous(), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }
    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selection.is_empty() {
            self.select_to(self.next(), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }
    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = 0..self.text.len();
        self.reversed = false;
        cx.notify();
    }
    fn commit(&mut self, _: &Commit, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(FieldEvent::Commit);
    }
    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(FieldEvent::Cancel);
    }

    fn at_mouse(&self, position: Point<Pixels>) -> usize {
        match (&self.bounds, &self.line) {
            (Some(bounds), Some(line)) => line.closest_index_for_x(position.x - bounds.left()),
            _ => 0,
        }
    }
    fn mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.selecting = true;
        let at = self.at_mouse(event.position);
        if event.modifiers.shift {
            self.select_to(at, cx)
        } else {
            self.move_to(at, cx)
        }
    }
    fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            self.select_to(self.at_mouse(event.position), cx);
        }
    }
    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    fn to_utf16(&self, byte: usize) -> usize {
        self.text[..byte].encode_utf16().count()
    }
    fn from_utf16(&self, offset: usize) -> usize {
        byte_from_utf16(&self.text, offset)
    }
    fn from_range(&self, range: Range<usize>) -> Range<usize> {
        self.from_utf16(range.start)..self.from_utf16(range.end)
    }
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.from_range(range);
        *adjusted = Some(self.to_utf16(range.start)..self.to_utf16(range.end));
        Some(self.text[range].to_string())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.to_utf16(self.selection.start)..self.to_utf16(self.selection.end),
            reversed: self.reversed,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|r| self.to_utf16(r.start)..self.to_utf16(r.end))
    }
    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = safe_byte_range(
            &self.text,
            range
                .map(|r| self.from_range(r))
                .or(self.marked.take())
                .unwrap_or(self.selection.clone()),
        );
        let text = text.replace(['\n', '\r'], " ");
        self.text = format!(
            "{}{}{}",
            &self.text[..range.start],
            text,
            &self.text[range.end..]
        )
        .into();
        let end = range.start + text.len();
        self.selection = end..end;
        self.reversed = false;
        cx.notify();
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let start = range
            .as_ref()
            .map(|r| self.from_utf16(r.start))
            .or_else(|| self.marked.as_ref().map(|r| r.start))
            .unwrap_or(self.selection.start)
            .min(self.text.len());
        self.replace_text_in_range(range, text, window, cx);
        let inserted = text.replace(['\n', '\r'], " ");
        self.marked = (!inserted.is_empty()).then_some(start..start + inserted.len());
        if let Some(selected) = selected {
            let at = start + byte_from_utf16(&inserted, selected.start);
            let end = start + byte_from_utf16(&inserted, selected.end);
            self.selection = safe_byte_range(&self.text, at..end);
        }
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.line.as_ref()?;
        let range = self.from_range(range);
        Some(Bounds::from_corners(
            point(bounds.left() + line.x_for_index(range.start), bounds.top()),
            point(bounds.left() + line.x_for_index(range.end), bounds.bottom()),
        ))
    }
    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.to_utf16(self.at_mouse(position)))
    }
}

struct TextElement {
    input: Entity<TextField>,
}
struct Prepaint {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Prepaint {
        let input = self.input.read(cx);
        let style = window.text_style();
        let display: SharedString = if input.text.is_empty() {
            input.placeholder.clone()
        } else {
            input.text.clone()
        };
        let color = if input.text.is_empty() {
            rgb(0x9f9689).into()
        } else {
            style.color
        };
        let run = TextRun {
            len: display.len(),
            font: style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window.text_system().shape_line(
            display,
            style.font_size.to_pixels(window.rem_size()),
            &[run],
            None,
        );
        let cursor = input.selection.is_empty().then(|| {
            fill(
                Bounds::new(
                    point(
                        bounds.left() + line.x_for_index(input.cursor()),
                        bounds.top(),
                    ),
                    size(px(2.), bounds.bottom() - bounds.top()),
                ),
                rgb(0xc7a778),
            )
        });
        let selection = (!input.selection.is_empty()).then(|| {
            fill(
                Bounds::from_corners(
                    point(
                        bounds.left() + line.x_for_index(input.selection.start),
                        bounds.top(),
                    ),
                    point(
                        bounds.left() + line.x_for_index(input.selection.end),
                        bounds.bottom(),
                    ),
                ),
                gpui::rgba(0xc7a77855),
            )
        });
        Prepaint {
            line: Some(line),
            cursor,
            selection,
        }
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        pre: &mut Prepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.input.read(cx).focus.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = pre.selection.take() {
            window.paint_quad(selection);
        }
        let line = pre.line.take().unwrap();
        line.paint(bounds.origin, window.line_height(), window, cx)
            .unwrap();
        if focus.is_focused(window)
            && let Some(cursor) = pre.cursor.take()
        {
            window.paint_quad(cursor);
        }
        self.input.update(cx, |input, _| {
            input.line = Some(line);
            input.bounds = Some(bounds);
        });
    }
}

impl Render for TextField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("TextField")
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::commit))
            .on_action(cx.listener(Self::cancel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .w_full()
            .h(px(if self.compact { 26. } else { 38. }))
            .px_2()
            .py(px(if self.compact { 2. } else { 8. }))
            .bg(rgb(0x141414))
            .text_size(px(if self.compact { 15. } else { 18. }))
            .child(TextElement { input: cx.entity() })
    }
}
impl Focusable for TextField {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{byte_from_utf16, safe_byte_range};

    #[test]
    fn stale_selection_on_empty_text_is_safe() {
        assert_eq!(safe_byte_range("", 8..8), 0..0);
    }

    #[test]
    fn utf16_offsets_stop_at_character_boundaries() {
        assert_eq!(byte_from_utf16("A🙂B", 2), 1);
        assert_eq!(byte_from_utf16("A🙂B", 3), 5);
        assert_eq!(safe_byte_range("A🙂B", 2..9), 1..6);
    }
}
