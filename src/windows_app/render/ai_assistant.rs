use super::super::ai_chat::{ChatEntry, SUGGESTED_MODEL};
use super::super::*;
use lightline::ai::{self, Role};

// The AI Assistant panel. Until a model is chosen it explains the
// assistant and offers to connect to Ollama; then it shows the conversation
// (questions as bubbles, answers as Markdown) above the message box.
//
// Painting only lays out text the panel already holds, and each answer's
// layout is kept until its text or the panel's width changes, so an open
// panel costs nothing while it's idle.

// What the panel promises about AI, whatever the model.
const AI_PROMISES: [&str; 3] = [
    "Nothing runs in the background",
    "Nothing is sent until you ask",
    "Editing never depends on AI",
];

// Most lines the message box grows to before its text scrolls.
const COMPOSER_LINES: usize = 6;

impl App {
    pub(in crate::windows_app) fn paint_ai_assistant(&self, hdc: HDC, rect: RECT) {
        let s = |value: i32| self.scale(value);
        self.panel_card(
            hdc,
            rect,
            s(CARD_RADIUS),
            self.theme.card_edge,
            self.theme.sidebar_bg,
        );
        *self.ai.hits.borrow_mut() = Default::default();
        let header_bottom = rect.top + s(AI_HEADER);
        self.paint_ai_header(hdc, rect, header_bottom);

        let content = RECT {
            left: rect.left + s(16),
            top: header_bottom + s(14),
            right: rect.right - s(16),
            bottom: rect.bottom - s(14),
        };
        unsafe { SelectObject(hdc, self.ui_font) };
        let line_height = self.text_height(hdc) + s(4);
        if !self.ai_ready() {
            let composer = RECT {
                top: content.bottom - s(50),
                ..content
            };
            self.paint_ai_setup_state(
                hdc,
                RECT {
                    top: content.top + s(10),
                    bottom: composer.top - s(12),
                    ..content
                },
            );
            self.paint_ai_composer(hdc, composer, &[], false);
            return;
        }

        let text_width = (content.right - content.left - s(64)).max(s(40));
        let lines = self.wrap_text(hdc, &self.ai.input, text_width);
        let shown = lines.len().clamp(1, COMPOSER_LINES);
        let composer = RECT {
            top: content.bottom - (shown as i32 * line_height + s(28)).max(s(50)),
            ..content
        };
        let mut conversation_bottom = composer.top - s(10);
        if let Some(label) = self.ai_selection_label() {
            let top = conversation_bottom - line_height;
            self.paint_ai_context(hdc, &label, RECT { top, ..composer });
            conversation_bottom = top - s(6);
        }
        self.paint_ai_conversation(
            hdc,
            RECT {
                bottom: conversation_bottom,
                ..content
            },
        );
        self.paint_ai_composer(
            hdc,
            composer,
            &lines[lines.len() - shown.min(lines.len())..],
            true,
        );
    }

    fn paint_ai_header(&self, hdc: HDC, rect: RECT, header_bottom: i32) {
        let s = |value: i32| self.scale(value);
        unsafe { SelectObject(hdc, self.brand_font) };
        self.sparkle_glyph(
            hdc,
            rect.left + s(18),
            rect.top + s(15),
            s(20),
            ui(148, 102, 255),
        );
        let title = "AI Assistant";
        Self::label(
            hdc,
            title,
            rect.left + s(48),
            rect.top + s(16),
            self.theme.text,
            rect,
        );
        let title_right = rect.left + s(48) + self.text_width(hdc, title);
        // Close; its hit target is in ai_click.
        unsafe { SelectObject(hdc, self.ui_font) };
        Self::label(
            hdc,
            "\u{00d7}",
            rect.right - s(25),
            rect.top + s(16),
            self.theme.muted,
            rect,
        );
        if let Some(model) = self.settings.ai_model.as_deref() {
            let new_chat = RECT {
                left: rect.right - s(60),
                top: rect.top + s(12),
                right: rect.right - s(36),
                bottom: rect.top + s(40),
            };
            unsafe { SelectObject(hdc, self.brand_font) };
            self.label_mid(
                hdc,
                "+",
                new_chat.left + s(7),
                (new_chat.top + new_chat.bottom) / 2,
                self.theme.muted,
                new_chat,
            );
            unsafe { SelectObject(hdc, self.ui_font) };
            self.ai.hits.borrow_mut().new_chat = Some(new_chat);

            let chip_left = title_right + s(12);
            let chip_right =
                (chip_left + self.text_width(hdc, model) + s(34)).min(new_chat.left - s(8));
            if chip_right - chip_left >= s(56) {
                let chip = RECT {
                    left: chip_left,
                    top: rect.top + s(13),
                    right: chip_right,
                    bottom: rect.top + s(39),
                };
                self.panel_card(hdc, chip, s(7), ui(66, 47, 126), ui(42, 28, 86));
                self.label_ellipsis(
                    hdc,
                    model,
                    chip.left + s(10),
                    chip.top + s(4),
                    ui(211, 195, 255),
                    RECT {
                        right: chip.right - s(20),
                        ..chip
                    },
                );
                Self::label(
                    hdc,
                    "\u{25be}",
                    chip.right - s(16),
                    chip.top + s(4),
                    ui(195, 165, 255),
                    chip,
                );
                self.ai.hits.borrow_mut().model = Some(chip);
            }
        }
        Self::fill(
            hdc,
            RECT {
                left: rect.left + s(1),
                top: header_bottom,
                right: rect.right - s(1),
                bottom: header_bottom + s(1).max(1),
            },
            self.theme.edge,
        );
    }

    // The sparkle tile and title that open the setup and empty-chat views;
    // returns the y below the title.
    fn paint_ai_intro(&self, hdc: HDC, area: RECT, title: &str) -> i32 {
        let s = |value: i32| self.scale(value);
        let tile = RECT {
            left: area.left,
            top: area.top,
            right: area.left + s(44),
            bottom: area.top + s(44),
        };
        Self::rounded_fill(hdc, tile, s(10), ui(38, 28, 84));
        self.sparkle_glyph(
            hdc,
            tile.left + s(11),
            tile.top + s(11),
            s(22),
            ui(180, 150, 255),
        );
        unsafe { SelectObject(hdc, self.brand_font) };
        let y = tile.bottom + s(16);
        Self::label(hdc, title, area.left, y, self.theme.text, area);
        let below = y + self.text_height(hdc) + s(10);
        unsafe { SelectObject(hdc, self.ui_font) };
        below
    }

    // The promises, one per line with a check mark; returns the y below.
    fn paint_ai_promises(&self, hdc: HDC, area: RECT, mut y: i32, line_height: i32) -> i32 {
        let s = |value: i32| self.scale(value);
        for promise in AI_PROMISES {
            Self::label(hdc, "\u{2713}", area.left, y, self.theme.green, area);
            self.label_ellipsis(hdc, promise, area.left + s(22), y, self.theme.muted, area);
            y += line_height + s(2);
        }
        y
    }

    // No model chosen yet: what the assistant does, and a button that asks
    // the server (Ollama by default) which models it has.
    fn paint_ai_setup_state(&self, hdc: HDC, area: RECT) {
        let s = |value: i32| self.scale(value);
        let mut y = self.paint_ai_intro(hdc, area, "Connect a model");
        let line_height = self.text_height(hdc) + s(4);
        y = self.paint_wrapped(
            hdc,
            "The assistant explains, fixes and writes code with a model that runs \
             on your PC through Ollama: free, private, and it works offline.",
            area,
            y,
            line_height,
            self.theme.text,
        );
        y = self.paint_ai_promises(hdc, area, y + s(12), line_height);

        y += s(12);
        if let Some(problem) = &self.ai.problem {
            y = self.paint_wrapped(hdc, problem, area, y, line_height, self.theme.error) + s(8);
        }
        let default_endpoint = self.settings.ai_endpoint == ai::DEFAULT_ENDPOINT;
        let label = if self.ai.connecting {
            "Connecting\u{2026}"
        } else if self.ai.problem.is_some() {
            "Try again"
        } else if default_endpoint {
            "Connect to Ollama"
        } else {
            "Connect"
        };
        let button = RECT {
            left: area.left,
            top: y,
            right: (area.left + self.text_width(hdc, label) + s(32)).min(area.right),
            bottom: y + s(34),
        };
        if button.bottom <= area.bottom {
            let fill = if self.ai.connecting {
                self.theme.active_bg
            } else {
                self.theme.violet
            };
            Self::rounded_fill(hdc, button, s(7), fill);
            let text = if self.ai.connecting {
                self.theme.muted
            } else {
                label_on(fill, 255, 255, 255)
            };
            self.label_mid(
                hdc,
                label,
                button.left + s(16),
                (button.top + button.bottom) / 2,
                text,
                button,
            );
            if !self.ai.connecting {
                self.ai.hits.borrow_mut().connect = Some(button);
            }
        }
        y = button.bottom + s(12);
        let hint = if default_endpoint {
            format!(
                "No Ollama yet? Get it from ollama.com, then run in a terminal: \
                 ollama pull {SUGGESTED_MODEL}"
            )
        } else {
            format!(
                "Uses the server at {} (aiEndpoint in settings).",
                self.settings.ai_endpoint
            )
        };
        self.paint_wrapped(hdc, &hint, area, y, line_height, self.theme.muted);
    }

    // A model is chosen but nothing has been asked yet.
    fn paint_ai_empty_chat(&self, hdc: HDC, area: RECT) {
        let s = |value: i32| self.scale(value);
        let mut y = self.paint_ai_intro(hdc, area, "Ask about your code");
        let line_height = self.text_height(hdc) + s(4);
        let model = self.settings.ai_model.as_deref().unwrap_or_default();
        let local = ["//localhost", "//127.0.0.1", "//[::1]"]
            .iter()
            .any(|host| self.settings.ai_endpoint.contains(host));
        let source = if ai::is_cloud_model(model) {
            format!(
                "Answers come from {model}, a cloud model: your questions and the code \
                 you include are sent to ollama.com."
            )
        } else if local {
            format!("Answers come from {model}, running on your PC.")
        } else {
            format!(
                "Answers come from {model} at {}.",
                self.settings.ai_endpoint
            )
        };
        y = self.paint_wrapped(
            hdc,
            &format!("Select code in the editor to include it with your question. {source}"),
            area,
            y,
            line_height,
            self.theme.text,
        );
        self.paint_ai_promises(hdc, area, y + s(12), line_height);
    }

    // The line above the message box saying which code will be sent.
    fn paint_ai_context(&self, hdc: HDC, label: &str, row: RECT) {
        let s = |value: i32| self.scale(value);
        unsafe { SelectObject(hdc, self.ui_font) };
        let dot = RECT {
            left: row.left + s(2),
            top: row.top + s(7),
            right: row.left + s(9),
            bottom: row.top + s(14),
        };
        Self::rounded_fill(hdc, dot, s(3), self.theme.violet);
        self.label_ellipsis(
            hdc,
            &format!("Includes {label}"),
            row.left + s(16),
            row.top,
            self.theme.muted,
            row,
        );
    }

    fn paint_ai_conversation(&self, hdc: HDC, area: RECT) {
        let s = |value: i32| self.scale(value);
        if area.bottom <= area.top {
            return;
        }
        let entries = &self.ai.entries;
        if entries.is_empty() {
            self.paint_ai_empty_chat(hdc, area);
            return;
        }
        let width = (area.right - area.left).max(s(60));
        let gap = s(16);
        let heights: Vec<i32> = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                self.ai_entry_height(hdc, entry, index + 1 == entries.len(), width)
            })
            .collect();
        let total = heights.iter().sum::<i32>() + gap * (entries.len() as i32 - 1);
        let max_scroll = (total - (area.bottom - area.top)).max(0);
        let scroll = if self.ai.follow.get() {
            max_scroll
        } else {
            self.ai.scroll.get().min(max_scroll)
        };
        // Back at the bottom: follow new text again.
        self.ai.follow.set(scroll >= max_scroll);
        self.ai.scroll.set(scroll);
        unsafe {
            let saved = SaveDC(hdc);
            IntersectClipRect(hdc, area.left, area.top, area.right, area.bottom);
            let mut top = area.top - scroll;
            for (index, (entry, height)) in entries.iter().zip(&heights).enumerate() {
                let bottom = top + height;
                if bottom >= area.top && top <= area.bottom {
                    let bounds = RECT {
                        left: area.left,
                        top,
                        right: area.left + width,
                        bottom,
                    };
                    self.paint_ai_entry(hdc, entry, index + 1 == entries.len(), bounds, area);
                }
                top = bottom + gap;
            }
            RestoreDC(hdc, saved);
        }
    }

    // Whether `entry` is the answer still arriving.
    fn ai_receiving(&self, last: bool) -> bool {
        last && self.ai.busy()
    }

    fn ai_entry_height(&self, hdc: HDC, entry: &ChatEntry, last: bool, width: i32) -> i32 {
        let s = |value: i32| self.scale(value);
        unsafe { SelectObject(hdc, self.ui_font) };
        let line_height = self.text_height(hdc) + s(4);
        if entry.role == Role::User {
            let lines = self.wrap_text(hdc, &entry.text, width - s(24)).len() as i32;
            let context = if entry.context.is_some() {
                line_height + s(2)
            } else {
                0
            };
            return s(20) + lines * line_height + context;
        }
        let mut height = s(26);
        let (answer, thinking) = ai::visible_answer(&entry.text);
        if !answer.is_empty() {
            height += self.snippet_height(
                hdc,
                &entry.view,
                &self.ai.fonts,
                (answer, entry.revision),
                width,
            );
        } else if thinking || self.ai_receiving(last) {
            height += line_height;
        }
        if let Some(error) = &entry.error {
            unsafe { SelectObject(hdc, self.ui_font) };
            height += s(6) + self.wrap_text(hdc, error, width).len() as i32 * line_height;
        }
        if !answer.is_empty() && !self.ai_receiving(last) {
            height += s(28);
        }
        height
    }

    fn paint_ai_entry(&self, hdc: HDC, entry: &ChatEntry, last: bool, bounds: RECT, clip: RECT) {
        let s = |value: i32| self.scale(value);
        unsafe { SelectObject(hdc, self.ui_font) };
        let line_height = self.text_height(hdc) + s(4);
        if entry.role == Role::User {
            self.panel_card(hdc, bounds, s(9), ui(42, 77, 133), ui(17, 35, 68));
            let inner = RECT {
                left: bounds.left + s(12),
                right: bounds.right - s(12),
                ..clip
            };
            let mut y = bounds.top + s(10);
            for line in self.wrap_text(hdc, &entry.text, inner.right - inner.left) {
                Self::label(hdc, &line, inner.left, y, self.theme.text, inner);
                y += line_height;
            }
            if let Some(context) = &entry.context {
                self.label_ellipsis(
                    hdc,
                    &format!("Includes {context}"),
                    inner.left,
                    y + s(2),
                    self.theme.muted,
                    inner,
                );
            }
            return;
        }

        self.sparkle_glyph(
            hdc,
            bounds.left,
            bounds.top + s(3),
            s(14),
            ui(148, 102, 255),
        );
        self.label_ellipsis(
            hdc,
            self.settings.ai_model.as_deref().unwrap_or("Assistant"),
            bounds.left + s(22),
            bounds.top,
            self.theme.muted,
            RECT {
                left: bounds.left,
                right: bounds.right,
                ..clip
            },
        );
        let mut y = bounds.top + s(26);
        let (answer, thinking) = ai::visible_answer(&entry.text);
        let visible = |rect: &RECT| rect.bottom > clip.top && rect.top < clip.bottom;
        if !answer.is_empty() {
            let height = self.snippet_height(
                hdc,
                &entry.view,
                &self.ai.fonts,
                (answer, entry.revision),
                bounds.right - bounds.left,
            );
            self.paint_snippet(hdc, &entry.view, &self.ai.fonts, (bounds.left, y), clip);
            // A Copy button in each code block's corner.
            unsafe { SelectObject(hdc, self.ui_font) };
            let copy_width = self.text_width(hdc, "Copy");
            for (block, code) in entry.view.code_blocks() {
                let button = RECT {
                    left: bounds.left + block.right - copy_width - s(20),
                    top: y + block.top + s(4),
                    right: bounds.left + block.right - s(4),
                    bottom: y + block.top + s(4) + line_height,
                };
                if visible(&button) {
                    Self::rounded_fill(hdc, button, s(4), self.theme.sidebar_bg);
                    Self::label(
                        hdc,
                        "Copy",
                        button.left + s(8),
                        button.top + s(1),
                        self.theme.muted,
                        clip,
                    );
                    self.ai.hits.borrow_mut().copies.push((button, code));
                }
            }
            y += height;
        } else if thinking || self.ai_receiving(last) {
            Self::label(
                hdc,
                "Thinking\u{2026}",
                bounds.left,
                y,
                self.theme.muted,
                clip,
            );
            y += line_height;
        }
        unsafe { SelectObject(hdc, self.ui_font) };
        if let Some(error) = &entry.error {
            let color = if error == "Stopped" {
                self.theme.muted
            } else {
                self.theme.error
            };
            y = self.paint_wrapped(
                hdc,
                error,
                RECT {
                    left: bounds.left,
                    right: bounds.right,
                    ..clip
                },
                y + s(6),
                line_height,
                color,
            );
        }
        if !answer.is_empty() && !self.ai_receiving(last) {
            let label = "Copy answer";
            let button = RECT {
                left: bounds.left,
                top: y + s(6),
                right: bounds.left + self.text_width(hdc, label) + s(4),
                bottom: y + s(6) + line_height,
            };
            if visible(&button) {
                Self::label(hdc, label, button.left, button.top, self.theme.muted, clip);
                self.ai
                    .hits
                    .borrow_mut()
                    .copies
                    .push((button, answer.to_string()));
            }
        }
    }

    // The message box: `lines` of typed text (already wrapped), and the Send
    // button, which is Stop while an answer arrives. Disabled until a model
    // is chosen.
    fn paint_ai_composer(&self, hdc: HDC, input: RECT, lines: &[String], enabled: bool) {
        let s = |value: i32| self.scale(value);
        if input.bottom - input.top < s(30) {
            return;
        }
        let focused = enabled && self.ai_typing();
        let edge = if focused {
            self.theme.violet
        } else {
            self.theme.edge
        };
        self.panel_card(hdc, input, s(9), edge, self.theme.editor_bg);
        let send = RECT {
            left: input.right - s(42),
            top: input.bottom - s(43),
            right: input.right - s(7),
            bottom: input.bottom - s(8),
        };
        let text = RECT {
            left: input.left + s(14),
            top: input.top + s(14),
            right: send.left - s(8),
            bottom: input.bottom - s(10),
        };
        unsafe { SelectObject(hdc, self.ui_font) };
        let line_height = self.text_height(hdc) + s(4);
        let empty = self.ai.input.is_empty();
        if !enabled || empty {
            let placeholder = if enabled {
                "Ask about your code\u{2026}"
            } else {
                "Connect a model to start chatting"
            };
            self.label_ellipsis(
                hdc,
                placeholder,
                text.left,
                text.top,
                self.theme.muted,
                text,
            );
        } else {
            let mut y = text.top;
            for line in lines {
                Self::label(hdc, line, text.left, y, self.theme.text, text);
                y += line_height;
            }
        }
        if focused {
            let (x, y) = match lines.last() {
                Some(line) if !empty => (
                    text.left + self.text_width(hdc, line),
                    text.top + (lines.len() as i32 - 1) * line_height,
                ),
                _ => (text.left, text.top),
            };
            Self::fill(
                hdc,
                RECT {
                    left: x.min(text.right),
                    top: y,
                    right: x.min(text.right) + s(1).max(1),
                    bottom: y + self.text_height(hdc),
                },
                self.theme.text,
            );
        }

        let busy = enabled && self.ai.busy();
        let ready = enabled && !self.ai.input.trim().is_empty();
        let fill = if ready && !busy {
            self.theme.violet
        } else {
            self.theme.active_bg
        };
        Self::rounded_fill(hdc, send, s(9), fill);
        unsafe {
            if busy {
                // Stop: a square.
                let half = s(5);
                let (cx, cy) = ((send.left + send.right) / 2, (send.top + send.bottom) / 2);
                Self::fill(
                    hdc,
                    RECT {
                        left: cx - half,
                        top: cy - half,
                        right: cx + half,
                        bottom: cy + half,
                    },
                    self.theme.text,
                );
            } else {
                let color = if ready {
                    label_on(fill, 255, 255, 255)
                } else {
                    self.theme.muted
                };
                let brush = CreateSolidBrush(color);
                let previous_brush = SelectObject(hdc, brush);
                let previous_pen = SelectObject(hdc, GetStockObject(NULL_PEN));
                let middle = (send.top + send.bottom) / 2;
                let points = [
                    POINT {
                        x: send.left + s(10),
                        y: send.top + s(8),
                    },
                    POINT {
                        x: send.right - s(8),
                        y: middle,
                    },
                    POINT {
                        x: send.left + s(10),
                        y: send.bottom - s(8),
                    },
                    POINT {
                        x: send.left + s(14),
                        y: middle,
                    },
                ];
                Polygon(hdc, points.as_ptr(), points.len() as i32);
                SelectObject(hdc, previous_pen);
                SelectObject(hdc, previous_brush);
                DeleteObject(brush);
            }
        }
        if enabled {
            let mut hits = self.ai.hits.borrow_mut();
            hits.composer = Some(input);
            hits.send = Some(send);
        }
    }

    // Draws `text` word-wrapped to `area`'s width from `y`, stopping at its
    // bottom; returns the y below the last line.
    fn paint_wrapped(
        &self,
        hdc: HDC,
        text: &str,
        area: RECT,
        mut y: i32,
        line_height: i32,
        color: u32,
    ) -> i32 {
        for line in self.wrap_text(hdc, text, area.right - area.left) {
            if y + line_height > area.bottom {
                break;
            }
            Self::label(hdc, &line, area.left, y, color, area);
            y += line_height;
        }
        y
    }

    // `text` wrapped to `width` pixels in the font selected into `hdc`:
    // at spaces where possible, inside a word only when it can't fit on a
    // line of its own. Line breaks in the text are kept.
    fn wrap_text(&self, hdc: HDC, text: &str, width: i32) -> Vec<String> {
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            let mut line = String::new();
            for word in paragraph.split(' ') {
                let candidate = if line.is_empty() {
                    word.to_string()
                } else {
                    format!("{line} {word}")
                };
                if self.text_width(hdc, &candidate) <= width {
                    line = candidate;
                    continue;
                }
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                // A word longer than the line is split between characters.
                for ch in word.chars() {
                    line.push(ch);
                    if line.chars().count() > 1 && self.text_width(hdc, &line) > width {
                        line.pop();
                        lines.push(std::mem::replace(&mut line, ch.to_string()));
                    }
                }
            }
            lines.push(line);
        }
        lines
    }

    pub(in crate::windows_app) fn sparkle_glyph(
        &self,
        hdc: HDC,
        x: i32,
        y: i32,
        size: i32,
        color: u32,
    ) {
        unsafe {
            let pen = CreatePen(PS_SOLID, self.scale(1).max(1), color);
            let brush = CreateSolidBrush(color);
            let prev_pen = SelectObject(hdc, pen);
            let prev_brush = SelectObject(hdc, brush);

            let s = size;
            let cx = x + s / 2;
            let cy = y + s / 2;
            let points = [
                POINT { x: cx, y },
                POINT {
                    x: cx + s / 6,
                    y: cy - s / 6,
                },
                POINT { x: x + s, y: cy },
                POINT {
                    x: cx + s / 6,
                    y: cy + s / 6,
                },
                POINT { x: cx, y: y + s },
                POINT {
                    x: cx - s / 6,
                    y: cy + s / 6,
                },
                POINT { x, y: cy },
                POINT {
                    x: cx - s / 6,
                    y: cy - s / 6,
                },
            ];
            Polygon(hdc, points.as_ptr(), points.len() as i32);

            SelectObject(hdc, prev_brush);
            SelectObject(hdc, prev_pen);
            DeleteObject(brush);
            DeleteObject(pen);
        }
    }
}
