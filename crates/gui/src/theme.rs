//! The controller's look follows what is falling: each effect has its own palette, and the pixel renderer
//! turns the whole window pixel-art (square corners, hard borders and shadows, a blocky font).

use windfall_core::config::{Effect, Element, Renderer};
use windfall_core::palette::{hex, mix, theme};

pub fn css(effect: Effect, renderer: Renderer, element: Element) -> String {
    let mut t = theme(effect);
    // Flowers made of an element tint the controller with it.
    if effect == Effect::Flowers {
        if let Some(a) = windfall_core::palette::element_accent(element) {
            t.accent = a;
        }
    }
    let pixel = renderer == Renderer::Pixel;
    let hov = hex(mix(t.surface, t.accent, 0.18));
    let mut s = format!(
        "@define-color window_bg_color {bg};
@define-color window_fg_color {fg};
@define-color view_bg_color {surface};
@define-color view_fg_color {fg};
@define-color headerbar_bg_color {bg};
@define-color headerbar_fg_color {fg};
@define-color headerbar_border_color {border};
@define-color card_bg_color {surface};
@define-color card_fg_color {fg};
@define-color popover_bg_color {surface};
@define-color popover_fg_color {fg};
@define-color accent_bg_color {accent};
@define-color accent_fg_color {accent_fg};
@define-color accent_color {accent};
window, .background {{ background-color: {bg}; color: {fg}; }}
.dim-label, .subtitle {{ color: {muted}; }}
.preview {{ border: 1px solid {border}; background: {bg}; }}
.effect-row button.toggle:checked {{ background: {accent}; color: {accent_fg}; }}
.effect-row button.toggle {{ background: {surface}; color: {fg}; }}
.effect-row button.toggle:hover {{ background: {hov}; }}
.effect-row button.toggle:checked:hover {{ background: {accent}; }}
scale trough {{ background: {border}; }}
scale highlight {{ background: {accent}; }}
scale slider {{ background: {fg}; }}
",
        bg = hex(t.bg),
        fg = hex(t.fg),
        surface = hex(t.surface),
        muted = hex(t.muted),
        accent = hex(t.accent),
        accent_fg = hex(t.accent_fg),
        border = hex(t.border),
        hov = hov,
    );
    if pixel {
        s.push_str(&format!(
            "* {{ border-radius: 0; font-family: \"Press Start 2P\", \"Pixelify Sans\", \"Silkscreen\", \"JetBrains Mono\", monospace; font-weight: bold; }}
.preview {{ border: 3px solid {border}; }}
button, entry, row, .card, .boxed-list, list, switch, switch slider, scale trough, scale highlight, scale slider, .preview, headerbar, popover contents {{ border-radius: 0; }}
button {{ border: 2px solid {border}; box-shadow: 3px 3px 0 {border}; transition: none; }}
button:active {{ box-shadow: 1px 1px 0 {border}; }}
list.boxed-list, .card {{ border: 2px solid {border}; box-shadow: 4px 4px 0 {border}; }}
switch {{ border: 2px solid {border}; }}
scale trough {{ min-height: 8px; border: 2px solid {border}; }}
scale slider {{ min-width: 14px; min-height: 14px; border: 2px solid {accent_fg}; }}
",
            border = hex(t.border),
            accent_fg = hex(t.accent_fg)
        ));
    }
    s
}
