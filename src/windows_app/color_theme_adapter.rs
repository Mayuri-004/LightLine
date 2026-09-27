// Maps a parsed Zed color theme (lightline::color_theme::ZedColorTheme) onto
// LightLine's own Theme. This is where Zed's color names meet LightLine's --
// the parser (src/color_theme.rs) knows nothing about Theme, and Theme knows
// nothing about Zed; only this file does the translation, the same split
// icon_theme.rs (parsing) / icons.rs (GDI rendering) already established.
//
// It works in two steps, so any theme applies to all of LightLine:
//
// 1. The theme's palette -- background, text and accent hues -- is read
//    first (accents from the theme's terminal colors, which every Zed theme
//    defines), and every LightLine role is rebuilt from it (Theme::derived).
//    A role the theme has no key for still gets a value that belongs to it.
// 2. Every key the theme does define then replaces the derived value.

use super::theme::{Palette, Theme};
use lightline::color_theme::{Rgba, ZedColorTheme};

// Alpha-composites `color` over `backdrop` (both already-resolved LightLine
// COLORREFs). Many of Zed's own chrome colors are intentionally translucent
// (e.g. an active-line highlight at ~20% opacity, meant to tint whatever's
// beneath it) -- LightLine's renderer has no alpha blending, so an opaque
// fill needs the blend done once, here, instead of showing the raw
// (much too vivid) foreground color at full strength.
fn composite(color: Rgba, backdrop: u32) -> u32 {
    let a = u32::from(color.a);
    let br = backdrop & 0xff;
    let bg = (backdrop >> 8) & 0xff;
    let bb = (backdrop >> 16) & 0xff;
    let blend = |fg: u8, bd: u32| -> u32 { (u32::from(fg) * a + bd * (255 - a)) / 255 };
    blend(color.r, br) | (blend(color.g, bg) << 8) | (blend(color.b, bb) << 16)
}

impl Theme {
    pub(super) fn from_zed_color_theme(zed: &ZedColorTheme) -> Theme {
        let default = Theme::default_dark();
        let editor_bg = ["editor.background", "background"]
            .iter()
            .find_map(|key| zed.style_color(key))
            .map_or(default.editor_bg, |color| {
                composite(color, default.editor_bg)
            });
        // The first of `keys` the theme defines, over its background.
        let first = |keys: &[&str], fallback: u32| {
            keys.iter()
                .find_map(|key| {
                    key.strip_prefix("syntax:")
                        .map_or_else(|| zed.style_color(key), |name| zed.syntax_color(name))
                })
                .map_or(fallback, |color| composite(color, editor_bg))
        };
        let text = first(&["text", "editor.foreground"], default.text);
        let palette = Palette {
            bg: editor_bg,
            fg: text,
            hues: [
                first(
                    &[
                        "text.accent",
                        "icon.accent",
                        "border.focused",
                        "terminal.ansi.cyan",
                    ],
                    default.sky,
                ),
                first(&["terminal.ansi.blue", "syntax:function"], default.blue),
                first(&["terminal.ansi.cyan", "syntax:type"], default.teal),
                first(
                    &["terminal.ansi.green", "created", "success", "syntax:string"],
                    default.green,
                ),
                first(&["terminal.ansi.magenta", "syntax:keyword"], default.violet),
                first(
                    &["terminal.ansi.bright_magenta", "terminal.ansi.magenta"],
                    default.pink,
                ),
                first(
                    &["syntax:number", "syntax:constant", "terminal.ansi.yellow"],
                    default.orange,
                ),
                first(&["error", "deleted", "terminal.ansi.red"], default.error),
                first(
                    &["warning", "modified", "terminal.ansi.yellow"],
                    default.warning,
                ),
            ],
        };
        let mut theme = Theme::derived(&palette);

        let backdrop = theme.editor_bg;
        let apply_style = |keys: &[&str], field: &mut u32| {
            if let Some(color) = keys.iter().find_map(|key| zed.style_color(key)) {
                *field = composite(color, backdrop);
            }
        };
        apply_style(&["text.muted", "text.placeholder"], &mut theme.muted);
        apply_style(&["border"], &mut theme.edge);
        apply_style(&["border.variant", "border"], &mut theme.card_edge);
        apply_style(&["status_bar.background"], &mut theme.status_bg);
        apply_style(
            &["tab_bar.background", "tab.inactive_background"],
            &mut theme.tab_bg,
        );
        apply_style(
            &["element.hover", "ghost_element.hover"],
            &mut theme.active_bg,
        );
        apply_style(
            &["panel.background", "surface.background"],
            &mut theme.sidebar_bg,
        );
        apply_style(
            &["panel.background", "surface.background"],
            &mut theme.rail_bg,
        );
        apply_style(&["background", "title_bar.background"], &mut theme.shell_bg);
        apply_style(&["editor.active_line.background"], &mut theme.line_bg);
        apply_style(&["editor.line_number"], &mut theme.line_number);
        apply_style(
            &["editor.active_line_number"],
            &mut theme.line_number_active,
        );
        apply_style(&["players.0.cursor"], &mut theme.cursor);
        apply_style(&["players.0.selection"], &mut theme.select_bg);
        apply_style(&["info", "hint"], &mut theme.info);

        let apply_syntax = |names: &[&str], field: &mut u32| {
            if let Some(color) = names.iter().find_map(|name| zed.syntax_color(name)) {
                *field = composite(color, backdrop);
            }
        };
        apply_syntax(&["comment"], &mut theme.comment);
        apply_syntax(&["string"], &mut theme.string);
        apply_syntax(&["keyword"], &mut theme.keyword);
        apply_syntax(&["type", "type.builtin"], &mut theme.type_color);
        apply_syntax(&["number", "constant"], &mut theme.number);
        apply_syntax(
            &["preproc", "constant", "function.builtin"],
            &mut theme.macro_color,
        );
        apply_syntax(&["function", "function.method"], &mut theme.function);
        apply_syntax(&["operator"], &mut theme.operator);
        apply_syntax(&["attribute", "property"], &mut theme.attribute);
        apply_syntax(&["punctuation"], &mut theme.punctuation);

        // The terminal's own colors, over the terminal's background.
        let terminal_bg = zed
            .style_color("terminal.background")
            .map_or(backdrop, |color| composite(color, backdrop));
        const ANSI_KEYS: [&str; 8] = [
            "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
        ];
        for (index, name) in ANSI_KEYS.iter().enumerate() {
            for (slot, key) in [
                (index, format!("terminal.ansi.{name}")),
                (index + 8, format!("terminal.ansi.bright_{name}")),
            ] {
                if let Some(color) = zed.style_color(&key) {
                    theme.ansi[slot] = composite(color, terminal_bg);
                }
            }
        }
        theme
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_app::rgb;

    #[test]
    fn opaque_color_ignores_backdrop() {
        let color = Rgba {
            r: 0x28,
            g: 0x2a,
            b: 0x36,
            a: 0xff,
        };
        assert_eq!(composite(color, 0x00ffffff), 0x00362a28); // COLORREF: r|g<<8|b<<16
    }

    #[test]
    fn fully_transparent_color_keeps_backdrop_unchanged() {
        let color = Rgba {
            r: 0xff,
            g: 0,
            b: 0,
            a: 0x00,
        };
        let backdrop = 0x00362a28;
        assert_eq!(composite(color, backdrop), backdrop);
    }

    // A small Dracula-like theme: chrome keys, syntax and terminal colors.
    const THEME: &str = r##"{"name": "Test", "themes": [{
        "name": "Test Dark", "appearance": "dark",
        "style": {
            "editor.background": "#282a36ff",
            "text": "#f8f8f2ff",
            "border": "#191a21ff",
            "terminal.ansi.blue": "#bd93f9ff",
            "terminal.ansi.green": "#50fa7bff",
            "terminal.ansi.magenta": "#ff79c6ff",
            "terminal.ansi.cyan": "#8be9fdff",
            "terminal.ansi.red": "#ff5555ff",
            "syntax": {"keyword": {"color": "#ff79c6ff"}}
        }
    }]}"##;

    #[test]
    fn a_theme_reaches_every_role_not_just_the_editor() {
        let zed = ZedColorTheme::parse(THEME).remove(0);
        let theme = Theme::from_zed_color_theme(&zed);
        let default = Theme::default_dark();
        assert_eq!(theme.editor_bg, rgb(0x28, 0x2a, 0x36));
        assert_eq!(theme.edge, rgb(0x19, 0x1a, 0x21));
        assert_eq!(theme.keyword, rgb(0xff, 0x79, 0xc6));
        // Accents come from the theme's terminal colors.
        assert_eq!(theme.blue, rgb(0xbd, 0x93, 0xf9));
        assert_eq!(theme.green, rgb(0x50, 0xfa, 0x7b));
        assert_eq!(theme.violet, rgb(0xff, 0x79, 0xc6));
        assert_eq!(theme.ansi[4], rgb(0xbd, 0x93, 0xf9));
        // Roles the theme has no key for are rebuilt from its palette
        // rather than left in LightLine's midnight blue.
        assert_ne!(theme.shell_bg, default.shell_bg);
        assert_ne!(theme.select_bg, default.select_bg);
        assert_ne!(theme.ansi[0], default.ansi[0]);
    }

    #[test]
    fn a_light_theme_gives_light_chrome() {
        let light = r##"{"name": "L", "themes": [{"name": "L", "appearance": "light",
            "style": {"editor.background": "#ffffffff", "text": "#24292fff"}}]}"##;
        let theme = Theme::from_zed_color_theme(&ZedColorTheme::parse(light).remove(0));
        assert!(theme.is_light());
        let lightness =
            |color: u32| (color & 0xff) + ((color >> 8) & 0xff) + ((color >> 16) & 0xff);
        for surface in [
            theme.shell_bg,
            theme.sidebar_bg,
            theme.tab_bg,
            theme.status_bg,
        ] {
            assert!(lightness(surface) > 600, "surface {surface:06x}");
        }
        assert!(lightness(theme.muted) < 600);
    }
}
