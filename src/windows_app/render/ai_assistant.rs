use super::super::*;

impl App {
    pub(in crate::windows_app) fn paint_ai_assistant(&self, hdc: HDC, rect: RECT) {
        let s = |value: i32| self.scale(value);
        let card_radius = s(CARD_RADIUS);
        self.panel_card(
            hdc,
            rect,
            card_radius,
            self.theme.card_edge,
            self.theme.sidebar_bg,
        );

        let clip = rect;
        let header_h = s(52);
        let header_bottom = rect.top + header_h;

        // Header: a stronger brand mark and a quiet model badge establish a
        // clear hierarchy without competing with the conversation.
        unsafe { SelectObject(hdc, self.brand_font) };
        self.sparkle_glyph(
            hdc,
            rect.left + s(18),
            rect.top + s(15),
            s(20),
            ui(148, 102, 255),
        );
        Self::label(
            hdc,
            "AI Assistant",
            rect.left + s(48),
            rect.top + s(16),
            self.theme.text,
            clip,
        );

        let panel_width = rect.right - rect.left;
        if panel_width >= s(330) {
            let model_left = rect.left + s(162);
            let badge_rect = RECT {
                left: model_left,
                top: rect.top + s(13),
                right: model_left + s(72),
                bottom: rect.top + s(39),
            };
            self.panel_card(hdc, badge_rect, s(7), ui(66, 47, 126), ui(42, 28, 86));
            self.sparkle_glyph(
                hdc,
                model_left + s(8),
                rect.top + s(20),
                s(11),
                ui(195, 165, 255),
            );
            unsafe { SelectObject(hdc, self.ui_font) };
            Self::label(
                hdc,
                "Tera",
                model_left + s(25),
                rect.top + s(17),
                ui(211, 195, 255),
                clip,
            );
        }

        // Header controls retain their existing hit targets in input.rs.
        unsafe { SelectObject(hdc, self.ui_font) };
        let btn_y = rect.top + s(16);
        Self::label(
            hdc,
            "\u{2014}",
            rect.right - s(78),
            btn_y,
            self.theme.muted,
            clip,
        );
        Self::label(
            hdc,
            "\u{21bb}",
            rect.right - s(52),
            btn_y,
            self.theme.muted,
            clip,
        );
        Self::label(
            hdc,
            "\u{00d7}",
            rect.right - s(25),
            btn_y,
            self.theme.muted,
            clip,
        );

        // Header bottom divider
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

        let content_left = rect.left + s(18);
        let content_right = rect.right - s(18);
        let chat_top = header_bottom + s(16);

        // User prompt card. A separated avatar rail makes the prompt easier to
        // distinguish from Tera's response at a glance.
        let user_bubble = RECT {
            left: content_left,
            top: chat_top,
            right: content_right,
            bottom: chat_top + s(58),
        };
        self.panel_card(hdc, user_bubble, s(9), ui(42, 77, 133), ui(17, 35, 68));
        let avatar_cell_right = user_bubble.left + s(48);
        Self::fill(
            hdc,
            RECT {
                left: avatar_cell_right,
                top: user_bubble.top + s(1),
                right: avatar_cell_right + s(1),
                bottom: user_bubble.bottom - s(1),
            },
            ui(37, 68, 116),
        );
        let user_avatar = RECT {
            left: user_bubble.left + s(10),
            top: user_bubble.top + s(13),
            right: user_bubble.left + s(38),
            bottom: user_bubble.top + s(41),
        };
        Self::rounded_fill(hdc, user_avatar, s(7), ui(21, 44, 80));
        unsafe {
            let brush = CreateSolidBrush(ui(142, 172, 220));
            let previous_brush = SelectObject(hdc, brush);
            let previous_pen = SelectObject(hdc, GetStockObject(NULL_PEN));
            Ellipse(
                hdc,
                user_avatar.left + s(10),
                user_avatar.top + s(5),
                user_avatar.left + s(18),
                user_avatar.top + s(13),
            );
            SelectObject(hdc, previous_pen);
            SelectObject(hdc, previous_brush);
            DeleteObject(brush);
        }
        Self::rounded_fill(
            hdc,
            RECT {
                left: user_avatar.left + s(7),
                top: user_avatar.top + s(15),
                right: user_avatar.right - s(7),
                bottom: user_avatar.bottom - s(5),
            },
            s(5),
            ui(142, 172, 220),
        );
        unsafe { SelectObject(hdc, self.ui_font) };
        Self::label(
            hdc,
            "Explain this function and suggest an",
            avatar_cell_right + s(14),
            user_bubble.top + s(10),
            self.theme.text,
            user_bubble,
        );
        Self::label(
            hdc,
            "optimization if possible.",
            avatar_cell_right + s(14),
            user_bubble.top + s(29),
            self.theme.text,
            user_bubble,
        );

        // Tera response header.
        let assistant_top = user_bubble.bottom + s(18);
        let avatar_rect = RECT {
            left: content_left,
            top: assistant_top,
            right: content_left + s(32),
            bottom: assistant_top + s(32),
        };
        Self::rounded_fill(hdc, avatar_rect, s(8), ui(102, 74, 226));
        self.sparkle_glyph(
            hdc,
            avatar_rect.left + s(7),
            avatar_rect.top + s(7),
            s(18),
            self.theme.text,
        );

        unsafe { SelectObject(hdc, self.brand_font) };
        Self::label(
            hdc,
            "Tera",
            avatar_rect.right + s(12),
            assistant_top + s(7),
            self.theme.text,
            clip,
        );

        unsafe { SelectObject(hdc, self.ui_font) };
        let text_y = assistant_top + s(44);
        let text_clip = RECT {
            left: content_left,
            top: text_y,
            right: content_right,
            bottom: rect.bottom - s(126),
        };

        Self::label(
            hdc,
            "This function calculates the tab layout for a window.",
            text_clip.left,
            text_y,
            self.theme.text,
            text_clip,
        );
        Self::label(
            hdc,
            "It iterates through visible tabs, computes their",
            text_clip.left,
            text_y + s(19),
            self.theme.text,
            text_clip,
        );
        Self::label(
            hdc,
            "bounds, and draws them on screen.",
            text_clip.left,
            text_y + s(38),
            self.theme.text,
            text_clip,
        );

        Self::label(
            hdc,
            "Possible optimization:",
            text_clip.left,
            text_y + s(68),
            ui(151, 178, 218),
            text_clip,
        );
        let optimization_rows = [
            (
                "1",
                "Cache scaled dimensions before the loop.",
                "Avoid repeated work on every tab.",
            ),
            (
                "2",
                "Iterate over visible tabs directly to",
                "simplify index handling.",
            ),
        ];
        for (index, (number, first, second)) in optimization_rows.iter().enumerate() {
            let row_top = text_y + s(91 + index as i32 * 46);
            let number_rect = RECT {
                left: text_clip.left,
                top: row_top,
                right: text_clip.left + s(26),
                bottom: row_top + s(26),
            };
            Self::rounded_fill(hdc, number_rect, s(13), ui(22, 48, 88));
            let number_width = self.text_width(hdc, number);
            Self::label(
                hdc,
                number,
                number_rect.left + (number_rect.right - number_rect.left - number_width) / 2,
                row_top + s(4),
                self.theme.text,
                number_rect,
            );
            Self::label(
                hdc,
                first,
                number_rect.right + s(10),
                row_top,
                self.theme.text,
                text_clip,
            );
            Self::label(
                hdc,
                second,
                number_rect.right + s(10),
                row_top + s(19),
                self.theme.muted,
                text_clip,
            );
        }

        // Code card with a dedicated toolbar and more breathing room.
        let code_top = text_y + s(188);
        let input_top = rect.bottom - s(112);
        let code_rect = RECT {
            left: content_left,
            top: code_top,
            right: content_right,
            bottom: (code_top + s(202)).min(input_top - s(18)),
        };
        if code_rect.bottom > code_rect.top + s(82) {
            self.panel_card(hdc, code_rect, s(8), ui(32, 61, 105), ui(8, 19, 36));
            let code_header_bottom = code_rect.top + s(38);
            Self::fill(
                hdc,
                RECT {
                    left: code_rect.left + s(1),
                    top: code_header_bottom,
                    right: code_rect.right - s(1),
                    bottom: code_header_bottom + s(1),
                },
                ui(29, 56, 96),
            );
            unsafe { SelectObject(hdc, self.brand_font) };
            Self::label(
                hdc,
                "Rust",
                code_rect.left + s(14),
                code_rect.top + s(10),
                ui(159, 185, 226),
                code_rect,
            );
            unsafe { SelectObject(hdc, self.ui_font) };
            let copy_text = "Copy";
            let copy_width = self.text_width(hdc, copy_text);
            Self::label(
                hdc,
                copy_text,
                code_rect.right - s(14) - copy_width,
                code_rect.top + s(10),
                ui(151, 178, 218),
                code_rect,
            );
            self.card_outline(
                hdc,
                RECT {
                    left: code_rect.right - s(48) - copy_width,
                    top: code_rect.top + s(12),
                    right: code_rect.right - s(38) - copy_width,
                    bottom: code_rect.top + s(23),
                },
                s(2),
                ui(126, 163, 216),
            );
            self.card_outline(
                hdc,
                RECT {
                    left: code_rect.right - s(44) - copy_width,
                    top: code_rect.top + s(8),
                    right: code_rect.right - s(34) - copy_width,
                    bottom: code_rect.top + s(19),
                },
                s(2),
                ui(126, 163, 216),
            );

            unsafe { SelectObject(hdc, self.font) };
            let code_line_h = self.line_height.min(s(19));
            let c_top = code_header_bottom + s(14);
            let lines = [
                "let width = self.scale(TAB_WIDTH);",
                "let height = self.scale(TAB_HEIGHT);",
                "",
                "for (index, tab) in self.visible_tabs() {",
                "    let bounds = tab_bounds(index, width, height);",
                "    self.draw_tab(tab, bounds);",
                "}",
            ];
            for (index, line) in lines.iter().enumerate() {
                let y = c_top + index as i32 * code_line_h;
                if y + code_line_h > code_rect.bottom - s(7) {
                    break;
                }
                let color = if line.trim_start().starts_with("let ") || line.starts_with("for ") {
                    ui(103, 181, 255)
                } else {
                    self.theme.text
                };
                Self::label(hdc, line, code_rect.left + s(16), y, color, code_rect);
            }
        }

        // Sticky composer and footer selectors.
        unsafe { SelectObject(hdc, self.ui_font) };
        let input_rect = RECT {
            left: content_left,
            top: input_top,
            right: content_right,
            bottom: input_top + s(50),
        };
        self.panel_card(hdc, input_rect, s(9), ui(43, 78, 134), ui(11, 25, 48));

        Self::label(
            hdc,
            "Ask Tera anything...",
            input_rect.left + s(14),
            input_rect.top + s(15),
            self.theme.muted,
            input_rect,
        );

        let send_btn = RECT {
            left: input_rect.right - s(42),
            top: input_rect.top + s(7),
            right: input_rect.right - s(7),
            bottom: input_rect.bottom - s(7),
        };
        Self::rounded_fill(hdc, send_btn, s(9), ui(92, 75, 238));
        unsafe {
            let brush = CreateSolidBrush(self.theme.text);
            let previous_brush = SelectObject(hdc, brush);
            let previous_pen = SelectObject(hdc, GetStockObject(NULL_PEN));
            let points = [
                POINT {
                    x: send_btn.left + s(10),
                    y: send_btn.top + s(8),
                },
                POINT {
                    x: send_btn.right - s(8),
                    y: (send_btn.top + send_btn.bottom) / 2,
                },
                POINT {
                    x: send_btn.left + s(10),
                    y: send_btn.bottom - s(8),
                },
                POINT {
                    x: send_btn.left + s(14),
                    y: (send_btn.top + send_btn.bottom) / 2,
                },
            ];
            Polygon(hdc, points.as_ptr(), points.len() as i32);
            SelectObject(hdc, previous_pen);
            SelectObject(hdc, previous_brush);
            DeleteObject(brush);
        }

        let chip_y = input_rect.bottom + s(10);
        let tera_chip = RECT {
            left: content_left,
            top: chip_y,
            right: content_left + s(88),
            bottom: chip_y + s(30),
        };
        self.panel_card(hdc, tera_chip, s(7), ui(66, 48, 126), ui(35, 25, 74));
        self.sparkle_glyph(
            hdc,
            tera_chip.left + s(10),
            chip_y + s(9),
            s(12),
            ui(180, 150, 255),
        );
        Self::label(
            hdc,
            "Tera",
            tera_chip.left + s(29),
            chip_y + s(6),
            ui(200, 185, 255),
            clip,
        );
        Self::label(
            hdc,
            "\u{25be}",
            tera_chip.right - s(18),
            chip_y + s(6),
            ui(157, 174, 210),
            clip,
        );

        let reasoning_chip = RECT {
            left: tera_chip.right + s(10),
            top: chip_y,
            right: tera_chip.right + s(124),
            bottom: chip_y + s(30),
        };
        self.panel_card(hdc, reasoning_chip, s(7), ui(36, 69, 119), ui(14, 31, 57));
        Self::label(
            hdc,
            "\u{25c9}",
            reasoning_chip.left + s(10),
            chip_y + s(6),
            ui(145, 174, 218),
            clip,
        );
        Self::label(
            hdc,
            "Medium",
            reasoning_chip.left + s(32),
            chip_y + s(6),
            self.theme.muted,
            clip,
        );
        Self::label(
            hdc,
            "\u{25be}",
            reasoning_chip.right - s(18),
            chip_y + s(6),
            self.theme.muted,
            clip,
        );
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
