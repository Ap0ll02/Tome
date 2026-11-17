use std::ops::Range;

use gpui::{
    App, Bounds, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    ShapedLine, SharedString, Style, TextAlign, TextRun, UTF16Selection, Window, WrappedLine,
    actions, div, fill, point, prelude::*, px, relative, rgb, size,
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

fn wrapped_position(lines: &[WrappedLine], index: usize, line_height: Pixels) -> Point<Pixels> {
    let mut start = 0;
    let mut y = px(0.);
    for line in lines {
        if index <= start + line.len() {
            let local = line
                .position_for_index(index - start, line_height)
                .unwrap_or_default();
            return point(local.x, y + local.y);
        }
        start += line.len() + 1;
        y += line.size(line_height).height;
    }
    point(px(0.), y)
}

actions!(
    field,
    [
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        Backspace,
        Delete,
        SelectAll,
        Commit,
        Cancel,
        CommitNotes
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
    multiline: bool,
    wrapped_lines: Vec<WrappedLine>,
    line_height: Pixels,
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
            multiline: false,
            wrapped_lines: Vec::new(),
            line_height: px(20.),
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

    pub fn multiline(mut self) -> Self {
        self.multiline = true;
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
    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(-1., cx);
    }
    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(1., cx);
    }
    fn move_vertical(&mut self, direction: f32, cx: &mut Context<Self>) {
        if !self.multiline {
            return;
        }
        let Some(bounds) = self.bounds else {
            return;
        };
        let position = wrapped_position(&self.wrapped_lines, self.cursor(), self.line_height);
        let y = position.y + self.line_height * direction;
        let target = if y < px(0.) {
            0
        } else {
            self.at_mouse(point(bounds.left() + position.x, bounds.top() + y + px(1.)))
        };
        self.move_to(target, cx);
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
    fn commit(&mut self, _: &Commit, window: &mut Window, cx: &mut Context<Self>) {
        if self.multiline {
            self.replace_text_in_range(None, "\n", window, cx);
        } else {
            cx.emit(FieldEvent::Commit);
        }
    }
    fn commit_notes(&mut self, _: &CommitNotes, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(FieldEvent::Commit);
    }
    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(FieldEvent::Cancel);
    }

    fn at_mouse(&self, position: Point<Pixels>) -> usize {
        if self.multiline {
            if let Some(bounds) = self.bounds {
                let mut y = position.y - bounds.top();
                let mut offset = 0;
                for line in &self.wrapped_lines {
                    let height = line.size(self.line_height).height;
                    if y < height {
                        let local = line
                            .closest_index_for_position(
                                point(position.x - bounds.left(), y),
                                self.line_height,
                            )
                            .unwrap_or_else(|index| index);
                        return offset + local;
                    }
                    y -= height;
                    offset += line.len() + 1;
                }
                return self.text.len();
            }
        }
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
        let text = if self.multiline {
            text.replace('\r', "\n")
        } else {
            text.replace(['\n', '\r'], " ")
        };
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
        let inserted = if self.multiline {
            text.replace('\r', "\n")
        } else {
            text.replace(['\n', '\r'], " ")
        };
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
        if self.multiline {
            let at = self.from_range(range).start;
            let position = wrapped_position(&self.wrapped_lines, at, self.line_height);
            return Some(Bounds::new(
                point(bounds.left() + position.x, bounds.top() + position.y),
                size(px(2.), self.line_height),
            ));
        }
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
    wrapped_lines: Vec<WrappedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    multiline_selection: Vec<PaintQuad>,
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
        let input = self.input.read(cx);
        if input.multiline {
            let text = input.text.clone();
            let font = window.text_style().font();
            let run = TextRun {
                len: text.len(),
                font,
                color: rgb(0xe9e2d5).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let width = input
                .bounds
                .map(|bounds| bounds.size.width)
                .unwrap_or(px(180.));
            let lines = window
                .text_system()
                .shape_text(text, px(15.), &[run], Some(width), None)
                .unwrap();
            let rows: usize = lines
                .iter()
                .map(|line| line.wrap_boundaries().len() + 1)
                .sum();
            style.size.height = (window.line_height() * rows.max(8) as f32).into();
        } else {
            style.size.height = window.line_height().into();
        }
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
        if input.multiline {
            let lines = window
                .text_system()
                .shape_text(
                    display,
                    style.font_size.to_pixels(window.rem_size()),
                    &[run],
                    Some(bounds.size.width),
                    None,
                )
                .unwrap();
            let position = wrapped_position(&lines, input.cursor(), window.line_height());
            let cursor = input.selection.is_empty().then(|| {
                fill(
                    Bounds::new(
                        point(bounds.left() + position.x, bounds.top() + position.y),
                        size(px(2.), window.line_height()),
                    ),
                    rgb(0xc7a778),
                )
            });
            let mut multiline_selection = Vec::new();
            if !input.selection.is_empty() {
                let start = wrapped_position(&lines, input.selection.start, window.line_height());
                let end = wrapped_position(&lines, input.selection.end, window.line_height());
                let mut y = start.y;
                while y <= end.y {
                    let left = if y == start.y { start.x } else { px(0.) };
                    let right = if y == end.y { end.x } else { bounds.size.width };
                    multiline_selection.push(fill(
                        Bounds::new(
                            point(bounds.left() + left, bounds.top() + y),
                            size(right - left, window.line_height()),
                        ),
                        gpui::rgba(0xc7a77855),
                    ));
                    y += window.line_height();
                }
            }
            return Prepaint {
                line: None,
                wrapped_lines: lines.into_vec(),
                cursor,
                selection: None,
                multiline_selection,
            };
        }
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
            wrapped_lines: Vec::new(),
            cursor,
            selection,
            multiline_selection: Vec::new(),
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
        for selection in pre.multiline_selection.drain(..) {
            window.paint_quad(selection);
        }
        if let Some(line) = pre.line.take() {
            line.paint(bounds.origin, window.line_height(), window, cx)
                .unwrap();
            self.input.update(cx, |input, _| input.line = Some(line));
        } else {
            let mut y = bounds.top();
            for line in &pre.wrapped_lines {
                line.paint(
                    point(bounds.left(), y),
                    window.line_height(),
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                )
                .unwrap();
                y += line.size(window.line_height()).height;
            }
        }
        if focus.is_focused(window)
            && let Some(cursor) = pre.cursor.take()
        {
            window.paint_quad(cursor);
        }
        self.input.update(cx, |input, _| {
            input.wrapped_lines = std::mem::take(&mut pre.wrapped_lines);
            input.line_height = window.line_height();
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
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::commit))
            .on_action(cx.listener(Self::commit_notes))
            .on_action(cx.listener(Self::cancel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .w_full()
            .when(!self.multiline, |field| {
                field.h(px(if self.compact { 26. } else { 38. }))
            })
            .when(self.multiline, |field| field.min_h(px(176.)))
            .px_2()
            .py(px(if self.compact { 2. } else { 8. }))
            .bg(rgb(0x141414))
            .text_size(px(if self.multiline || self.compact {
                15.
            } else {
                18.
            }))
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
