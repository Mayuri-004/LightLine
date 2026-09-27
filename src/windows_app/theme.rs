// A centralized, runtime-swappable color palette.
//
// Colors are Win32 COLORREF (0x00BBGGRR), matching the existing `rgb()`
// helper -- this stays a windows_app type, not a library-crate one, since
// that byte order is a GDI detail, not portable data.
//
// Two layers make a theme apply to the whole window, not just the editor:
//
// * `Theme` holds named roles (backgrounds, text, accents, syntax, the
//   terminal's ANSI colors) that render code reads directly.
// * Everything else in the UI was drawn with hand-picked `rgb(...)` shades
//   designed against LightLine's own midnight palette. Those go through
//   `ui(...)` / `themed(...)`, which rebuilds each shade from the active
//   theme: every shade is a mix of the background, the text color and one
//   accent (see `Recipe`), so the same mix of another theme's colors gives
//   the matching shade there -- for any theme, light ones included. With the
//   default theme every color comes back exactly as designed.

use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Clone)]
pub(super) struct Theme {
    // Chrome: window backdrop, cards, panels.
    pub(super) shell_bg: u32,
    pub(super) card_edge: u32,
    pub(super) editor_bg: u32,
    pub(super) rail_bg: u32,
    pub(super) sidebar_bg: u32,
    pub(super) tab_bg: u32,
    pub(super) active_bg: u32,
    pub(super) status_bg: u32,
    pub(super) line_bg: u32,
    pub(super) select_bg: u32,
    pub(super) edge: u32,

    // Text.
    pub(super) text: u32,
    pub(super) muted: u32,

    // Accent hues, reused across chrome (buttons, badges, git status, debug
    // toolbar, icons, ...) as well as syntax highlighting below. `sky` is
    // the UI's main interactive accent (buttons, focus, links).
    pub(super) sky: u32,
    pub(super) blue: u32,
    pub(super) violet: u32,
    pub(super) teal: u32,
    pub(super) green: u32,
    pub(super) pink: u32,
    pub(super) orange: u32,

    // Syntax highlighting (see lightline::syntax::Color).
    pub(super) comment: u32,
    pub(super) string: u32,
    pub(super) keyword: u32,
    pub(super) type_color: u32,
    pub(super) number: u32,
    pub(super) macro_color: u32,
    pub(super) function: u32,
    pub(super) operator: u32,
    pub(super) attribute: u32,
    // No distinct rendering exists for punctuation yet (it falls through as
    // plain `text`); this slot exists so a future theme can claim it
    // without another renderer change.
    pub(super) punctuation: u32,

    // Editor semantics.
    pub(super) cursor: u32,
    pub(super) line_number: u32,
    pub(super) line_number_active: u32,
    pub(super) error: u32,
    pub(super) warning: u32,
    // Not distinguished from `warning` in any current render code (LSP
    // "information"/"hint" severities render the same as a warning today);
    // reserved for when that distinction is worth making.
    pub(super) info: u32,

    // The terminal's 16 ANSI colors: black, red, green, yellow, blue,
    // magenta, cyan, white, then the bright variants in the same order.
    pub(super) ansi: [u32; 16],
}

impl Theme {
    // LightLine's first-party midnight palette. Runtime theme overrides still
    // replace individual roles without changing the layout system.
    pub(super) fn default_dark() -> Self {
        use super::rgb;
        let text = rgb(226, 232, 240);
        let muted = rgb(126, 146, 178);
        let blue = rgb(82, 151, 255);
        let violet = rgb(139, 92, 246);
        let teal = rgb(45, 212, 191);
        let green = rgb(52, 211, 153);
        let warning = rgb(245, 184, 95);
        Self {
            shell_bg: rgb(5, 10, 22),
            card_edge: rgb(27, 48, 82),
            editor_bg: rgb(7, 14, 29),
            rail_bg: rgb(7, 13, 27),
            sidebar_bg: rgb(8, 16, 32),
            tab_bg: rgb(8, 17, 34),
            active_bg: rgb(17, 31, 59),
            status_bg: rgb(7, 14, 29),
            line_bg: rgb(14, 31, 61),
            select_bg: rgb(26, 48, 101),
            edge: rgb(24, 47, 82),
            text,
            muted,
            sky: rgb(56, 189, 248),
            blue,
            violet,
            teal,
            green,
            pink: rgb(236, 72, 153),
            orange: rgb(245, 145, 80),
            comment: muted,
            string: green,
            keyword: blue,
            type_color: teal,
            number: rgb(248, 180, 130),
            macro_color: violet,
            function: rgb(220, 210, 130),
            operator: rgb(200, 200, 220),
            attribute: rgb(180, 140, 230),
            punctuation: text,
            cursor: blue,
            line_number: muted,
            line_number_active: text,
            error: rgb(246, 110, 120),
            warning,
            info: warning,
            ansi: [
                rgb(30, 34, 44),    // black
                rgb(205, 79, 79),   // red
                rgb(119, 221, 119), // green
                rgb(229, 200, 90),  // yellow
                rgb(96, 143, 244),  // blue
                rgb(190, 120, 224), // magenta
                rgb(92, 200, 214),  // cyan
                rgb(210, 218, 235), // white
                rgb(110, 120, 140), // bright black
                rgb(240, 100, 100), // bright red
                rgb(150, 240, 150), // bright green
                rgb(245, 220, 120), // bright yellow
                rgb(130, 170, 250), // bright blue
                rgb(215, 150, 245), // bright magenta
                rgb(130, 225, 235), // bright cyan
                rgb(240, 245, 252), // bright white
            ],
        }
    }

    /// The colors this theme is built from.
    pub(super) fn palette(&self) -> Palette {
        Palette {
            bg: self.editor_bg,
            fg: self.text,
            hues: [
                self.sky,
                self.blue,
                self.teal,
                self.green,
                self.violet,
                self.pink,
                self.orange,
                self.error,
                self.warning,
            ],
        }
    }

    /// LightLine's own theme rebuilt around `palette`: every role keeps its
    /// relationship to the background, text and accents, so a theme that
    /// only defines some roles still gets consistent values for the rest.
    pub(super) fn derived(palette: &Palette) -> Self {
        let default = Self::default_dark();
        let from = default.palette();
        let map = |color: u32| retarget(color, &from, palette);
        let [sky, blue, teal, green, violet, pink, orange, error, warning] = palette.hues;
        Self {
            shell_bg: map(default.shell_bg),
            card_edge: map(default.card_edge),
            editor_bg: palette.bg,
            rail_bg: map(default.rail_bg),
            sidebar_bg: map(default.sidebar_bg),
            tab_bg: map(default.tab_bg),
            active_bg: map(default.active_bg),
            status_bg: map(default.status_bg),
            line_bg: map(default.line_bg),
            select_bg: map(default.select_bg),
            edge: map(default.edge),
            text: palette.fg,
            muted: map(default.muted),
            sky,
            blue,
            violet,
            teal,
            green,
            pink,
            orange,
            comment: map(default.comment),
            string: green,
            keyword: blue,
            type_color: teal,
            number: map(default.number),
            macro_color: violet,
            function: map(default.function),
            operator: map(default.operator),
            attribute: map(default.attribute),
            punctuation: palette.fg,
            cursor: blue,
            line_number: map(default.line_number),
            line_number_active: palette.fg,
            error,
            warning,
            info: warning,
            ansi: default.ansi.map(map),
        }
    }

    // Applies settings.json's `colors` overrides (already loaded/parsed by
    // src/settings.rs, previously only consulted for the 4 syntax colors
    // that had a key; now the single place every themeable key is resolved).
    pub(super) fn with_overrides(
        mut self,
        overrides: &std::collections::HashMap<String, u32>,
    ) -> Self {
        let apply = |key: &str, field: &mut u32| {
            if let Some(&color) = overrides.get(key) {
                *field = color;
            }
        };
        apply("shellBg", &mut self.shell_bg);
        apply("cardEdge", &mut self.card_edge);
        apply("editorBg", &mut self.editor_bg);
        apply("railBg", &mut self.rail_bg);
        apply("sidebarBg", &mut self.sidebar_bg);
        apply("tabBg", &mut self.tab_bg);
        apply("activeBg", &mut self.active_bg);
        apply("statusBg", &mut self.status_bg);
        apply("lineBg", &mut self.line_bg);
        apply("selectBg", &mut self.select_bg);
        apply("edge", &mut self.edge);
        apply("text", &mut self.text);
        apply("muted", &mut self.muted);
        apply("accent", &mut self.sky);
        apply("blue", &mut self.blue);
        apply("violet", &mut self.violet);
        apply("teal", &mut self.teal);
        apply("green", &mut self.green);
        apply("pink", &mut self.pink);
        apply("orange", &mut self.orange);
        apply("comment", &mut self.comment);
        apply("string", &mut self.string);
        apply("keyword", &mut self.keyword);
        apply("type", &mut self.type_color);
        apply("number", &mut self.number);
        apply("macro", &mut self.macro_color);
        apply("function", &mut self.function);
        apply("operator", &mut self.operator);
        apply("attribute", &mut self.attribute);
        apply("punctuation", &mut self.punctuation);
        apply("cursor", &mut self.cursor);
        apply("lineNumber", &mut self.line_number);
        apply("lineNumberActive", &mut self.line_number_active);
        apply("error", &mut self.error);
        apply("warning", &mut self.warning);
        apply("info", &mut self.info);
        self
    }

    /// True when the background is lighter than the text.
    pub(super) fn is_light(&self) -> bool {
        luminance(channels(self.editor_bg)) > luminance(channels(self.text))
    }
}

/// The colors every shade of a theme is expressed in: background, text,
/// and the accent hues (sky, blue, teal, green, violet, pink, orange, red,
/// yellow).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Palette {
    pub(super) bg: u32,
    pub(super) fg: u32,
    pub(super) hues: [u32; 9],
}

fn channels(color: u32) -> [f32; 3] {
    [
        (color & 0xff) as f32,
        ((color >> 8) & 0xff) as f32,
        ((color >> 16) & 0xff) as f32,
    ]
}

fn pack(color: [f32; 3]) -> u32 {
    let byte = |value: f32| value.round().clamp(0.0, 255.0) as u32;
    byte(color[0]) | (byte(color[1]) << 8) | (byte(color[2]) << 16)
}

fn luminance(color: [f32; 3]) -> f32 {
    0.2126 * color[0] + 0.7152 * color[1] + 0.0722 * color[2]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// How one shade is built from a palette: `bg + t·(fg − bg) + u·(hue − bg)`
/// plus the small remainder the mix doesn't explain.
#[derive(Debug)]
struct Recipe {
    hue: Option<usize>,
    t: f32,
    u: f32,
    remainder: [f32; 3],
    // Brighter than the text itself: a label on a colored fill.
    paper: bool,
}

// How much better an accent must explain a shade than plain grey does, so
// greys aren't tinted by a coincidental fit (RGB distance).
const HUE_PREFERENCE: f32 = 6.0;

fn recipe(color: u32, from: &Palette) -> Recipe {
    let bg = channels(from.bg);
    let fg = channels(from.fg);
    let c = channels(color);
    let axis = sub(fg, bg);
    let offset = sub(c, bg);
    let paper = luminance(c) > luminance(fg) + 4.0 && c.iter().all(|&channel| channel >= 225.0);
    let axis_len = dot(axis, axis).max(1.0);
    let t = dot(axis, offset) / axis_len;
    let grey_rest = [
        offset[0] - t * axis[0],
        offset[1] - t * axis[1],
        offset[2] - t * axis[2],
    ];
    let mut best = Recipe {
        hue: None,
        t,
        u: 0.0,
        remainder: grey_rest,
        paper,
    };
    let grey_error = dot(grey_rest, grey_rest).sqrt();
    let mut best_error = f32::MAX;
    for (index, &hue) in from.hues.iter().enumerate() {
        let toward = sub(channels(hue), bg);
        let (aa, ah, hh) = (axis_len, dot(axis, toward), dot(toward, toward));
        let det = aa * hh - ah * ah;
        if det.abs() < 1e-3 * aa * hh.max(1.0) {
            continue;
        }
        let (ad, hd) = (dot(axis, offset), dot(toward, offset));
        let t = (ad * hh - hd * ah) / det;
        let u = (aa * hd - ah * ad) / det;
        let rest = [
            offset[0] - t * axis[0] - u * toward[0],
            offset[1] - t * axis[1] - u * toward[1],
            offset[2] - t * axis[2] - u * toward[2],
        ];
        let error = dot(rest, rest).sqrt();
        if error < best_error && error + HUE_PREFERENCE < grey_error {
            best_error = error;
            best = Recipe {
                hue: Some(index),
                t,
                u,
                remainder: rest,
                paper,
            };
        }
    }
    best
}

fn cook(recipe: &Recipe, to: &Palette) -> u32 {
    let bg = channels(to.bg);
    let fg = channels(to.fg);
    // A light theme's labels on colored fills use its light background,
    // not the (dark) text color the plain mix would give.
    if recipe.paper && luminance(bg) > luminance(fg) {
        return to.bg;
    }
    let axis = sub(fg, bg);
    let toward = recipe
        .hue
        .map_or([0.0; 3], |index| sub(channels(to.hues[index]), bg));
    let mut out = [0.0; 3];
    for channel in 0..3 {
        out[channel] = bg[channel]
            + recipe.t * axis[channel]
            + recipe.u * toward[channel]
            + recipe.remainder[channel];
    }
    pack(out)
}

/// `color`, designed against palette `from`, rebuilt from palette `to`.
pub(super) fn retarget(color: u32, from: &Palette, to: &Palette) -> u32 {
    if from == to {
        return color;
    }
    cook(&recipe(color, from), to)
}

struct Active {
    from: Palette,
    to: Palette,
    cache: HashMap<u32, u32>,
}

thread_local! {
    // The palette hard-coded UI colors are rebuilt from. It lives beside the
    // window (the UI thread) rather than being passed to every paint helper
    // and dialog; App::apply_theme keeps it in step with App::theme.
    static ACTIVE: RefCell<Active> = RefCell::new({
        let default = Theme::default_dark().palette();
        Active {
            from: default,
            to: default,
            cache: HashMap::new(),
        }
    });
}

/// Makes hard-coded UI colors follow `theme` from now on.
pub(super) fn activate(theme: &Theme) {
    let to = theme.palette();
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        if active.to != to {
            active.to = to;
            active.cache.clear();
        }
    });
}

// Cache keys for ink(): COLORREFs only use the low 24 bits.
const INK_KEY: u32 = 1 << 24;

fn rebuild(color: u32, paper_allowed: bool) -> u32 {
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        if active.from == active.to {
            return color;
        }
        let key = if paper_allowed {
            color
        } else {
            color | INK_KEY
        };
        if let Some(&mapped) = active.cache.get(&key) {
            return mapped;
        }
        let mut recipe = recipe(color, &active.from);
        recipe.paper &= paper_allowed;
        let mapped = cook(&recipe, &active.to);
        active.cache.insert(key, mapped);
        mapped
    })
}

/// A UI color designed for LightLine's default palette, in the active theme.
pub(super) fn themed(color: u32) -> u32 {
    rebuild(color, true)
}

/// `rgb(r, g, b)` for UI chrome: the shade as designed for the default
/// theme, rebuilt for the active one.
pub(super) fn ui(r: u8, g: u8, b: u8) -> u32 {
    themed(super::rgb(r, g, b))
}

/// `ui` for bright text drawn on a panel surface rather than on a colored
/// fill (a card title, a selected menu row): it follows the theme's text
/// color, so it stays readable in a light theme instead of turning into
/// the light "paper" labels on buttons get.
pub(super) fn ink(r: u8, g: u8, b: u8) -> u32 {
    rebuild(super::rgb(r, g, b), false)
}

/// `ui` for a bright label drawn on `fill` (an already themed color): the
/// light label a button gets, or the theme's text color when the fill turns
/// out light in this theme -- whichever reads better on it.
pub(super) fn label_on(fill: u32, r: u8, g: u8, b: u8) -> u32 {
    let (paper, text) = (ui(r, g, b), ink(r, g, b));
    if paper == text || contrast(paper, fill) >= contrast(text, fill) {
        paper
    } else {
        text
    }
}

// WCAG contrast ratio between two colors.
fn contrast(a: u32, b: u32) -> f32 {
    let relative = |color: u32| {
        let linear = |channel: f32| {
            let c = channel / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let [r, g, b] = channels(color).map(linear);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    };
    let (la, lb) = (relative(a), relative(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_app::rgb;

    fn dracula() -> Palette {
        Palette {
            bg: rgb(0x28, 0x2a, 0x36),
            fg: rgb(0xf8, 0xf8, 0xf2),
            hues: [
                rgb(0x8b, 0xe9, 0xfd),
                rgb(0xbd, 0x93, 0xf9),
                rgb(0x8b, 0xe9, 0xfd),
                rgb(0x50, 0xfa, 0x7b),
                rgb(0xff, 0x79, 0xc6),
                rgb(0xff, 0x79, 0xc6),
                rgb(0xff, 0xb8, 0x6c),
                rgb(0xff, 0x55, 0x55),
                rgb(0xf1, 0xfa, 0x8c),
            ],
        }
    }

    fn light() -> Palette {
        Palette {
            bg: rgb(255, 255, 255),
            fg: rgb(36, 41, 47),
            hues: [
                rgb(9, 105, 218),
                rgb(9, 105, 218),
                rgb(27, 124, 131),
                rgb(26, 127, 55),
                rgb(130, 80, 223),
                rgb(191, 57, 137),
                rgb(188, 76, 0),
                rgb(207, 34, 46),
                rgb(154, 103, 0),
            ],
        }
    }

    #[test]
    fn the_default_theme_keeps_every_color_exactly() {
        let default = Theme::default_dark().palette();
        for color in [
            rgb(16, 27, 45),
            rgb(56, 189, 248),
            rgb(255, 255, 255),
            rgb(1, 2, 3),
        ] {
            assert_eq!(retarget(color, &default, &default), color);
        }
        // Rebuilding the default theme around its own palette changes
        // nothing either.
        let derived = Theme::derived(&default);
        assert_eq!(derived.shell_bg, Theme::default_dark().shell_bg);
        assert_eq!(derived.select_bg, Theme::default_dark().select_bg);
    }

    #[test]
    fn palette_colors_map_onto_the_target_palette() {
        let default = Theme::default_dark().palette();
        let target = dracula();
        // The anchors themselves land exactly on the target's.
        assert_eq!(retarget(default.bg, &default, &target), target.bg);
        for (from, to) in default.hues.iter().zip(target.hues) {
            let mapped = channels(retarget(*from, &default, &target));
            let expected = channels(to);
            for channel in 0..3 {
                assert!((mapped[channel] - expected[channel]).abs() <= 1.0);
            }
        }
    }

    #[test]
    fn dark_surfaces_become_light_in_a_light_theme() {
        let default = Theme::default_dark().palette();
        let card = retarget(rgb(16, 27, 45), &default, &light());
        let text = retarget(rgb(226, 234, 248), &default, &light());
        assert!(luminance(channels(card)) > 200.0, "card {card:06x}");
        assert!(luminance(channels(text)) < 80.0, "text {text:06x}");
        // A white label on a colored button stays light, so it stays
        // readable on the (darker) button.
        let label = retarget(rgb(255, 255, 255), &default, &light());
        assert!(luminance(channels(label)) > 240.0);
    }

    #[test]
    fn titles_on_surfaces_stay_readable_in_a_light_theme() {
        activate(&Theme::derived(&light()));
        // The same near-white: a button label stays light, a card title
        // follows the (dark) text.
        assert!(luminance(channels(ui(244, 248, 255))) > 240.0);
        assert!(luminance(channels(ink(244, 248, 255))) < 80.0);
        // A label follows whichever reads on its fill.
        assert_eq!(label_on(rgb(20, 20, 20), 255, 255, 255), ui(255, 255, 255));
        assert_eq!(
            label_on(rgb(230, 230, 240), 255, 255, 255),
            ink(255, 255, 255)
        );
        activate(&Theme::default_dark());
        assert_eq!(label_on(rgb(30, 50, 88), 255, 255, 255), rgb(255, 255, 255));
        assert_eq!(ink(244, 248, 255), rgb(244, 248, 255));
    }

    #[test]
    fn greys_stay_grey_and_accents_follow_their_hue() {
        let default = Theme::default_dark().palette();
        let target = dracula();
        // A mid slate grey keeps a Dracula-grey feel: no strong cast.
        let grey = channels(retarget(rgb(148, 163, 184), &default, &target));
        let spread = grey.iter().cloned().fold(0.0f32, f32::max)
            - grey.iter().cloned().fold(255.0f32, f32::min);
        assert!(spread < 60.0, "grey {grey:?}");
        // A dark green badge background becomes a dark Dracula green.
        let badge = channels(retarget(rgb(15, 58, 45), &default, &target));
        assert!(
            badge[1] > badge[0] && badge[1] > badge[2],
            "badge {badge:?}"
        );
    }
}
