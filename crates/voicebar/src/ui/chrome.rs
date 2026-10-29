//! Shared visual language for settings and the no-focus dictation overlay.
use gpui::{prelude::*, *};
use gpui_component::{Theme, ThemeMode};

pub(super) use crate::palette::*;

pub(super) fn init_theme(cx: &mut App) {
    gpui_component::init(cx);
    Theme::change(ThemeMode::Dark, None, cx);
    let theme = Theme::global_mut(cx);
    theme.font_family = FONT_FAMILY.into();
    theme.font_size = px(14.);
    theme.radius = px(8.);
    theme.colors.background = rgb(PANEL).into();
    theme.colors.foreground = rgb(TEXT).into();
    theme.colors.border = rgb(LINE).into();
    theme.colors.input = rgb(LINE).into();
    theme.colors.muted = rgb(0x262d38).into();
    theme.colors.muted_foreground = rgb(MUTED).into();
    theme.colors.primary = rgb(MINT).into();
    theme.colors.primary_hover = rgb(0xc4f4e4).into();
    theme.colors.primary_active = rgb(0x83d6b9).into();
    theme.colors.primary_foreground = rgb(0x10251d).into();
    theme.colors.ring = rgba(0xa3ead255).into();
    theme.colors.caret = rgb(MINT).into();
    theme.colors.selection = rgba(0xa3ead23a).into();
    theme.colors.switch = rgb(0x343d4b).into();
    theme.colors.switch_thumb = rgb(TEXT).into();
}

pub(super) fn column() -> Div {
    div().flex().flex_col().flex_shrink_0().gap_3()
}
pub(super) fn row() -> Div {
    div().flex().flex_shrink_0().items_center().gap_3()
}
pub(super) fn note(text: impl Into<SharedString>) -> Div {
    div()
        .flex_shrink_0()
        .text_size(px(12.))
        .text_color(rgb(MUTED))
        .line_height(px(20.))
        .child(text.into())
}
pub(super) fn title(text: impl Into<SharedString>) -> Div {
    div()
        .flex_shrink_0()
        .text_size(px(16.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT))
        .child(text.into())
}
pub(super) fn eyebrow(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(10.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(MUTED))
        .child(text.into())
}
pub(super) fn card() -> Div {
    column()
        .p_5()
        .gap_4()
        .rounded(px(14.))
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgb(PANEL))
}
fn button_base(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .flex_shrink_0()
        .h(px(36.))
        .px_3()
        .rounded(px(8.))
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgb(0x222833))
        .text_size(px(12.))
        .line_height(px(18.))
        .text_color(rgb(TEXT))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .cursor_pointer()
        .child(label.into())
}
pub(super) fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    button_base(id, label).hover(|s| s.bg(rgb(0x2d3543)).border_color(rgb(0x414c5e)))
}
pub(super) fn primary(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    button_base(id, label)
        .bg(rgb(MINT))
        .border_color(rgb(MINT))
        .text_color(rgb(0x10251d))
        .hover(|s| s.bg(rgb(0xc4f4e4)).border_color(rgb(0xc4f4e4)))
}
pub(super) fn keycaps(shortcut: &str) -> Div {
    row()
        .gap_1()
        .flex_wrap()
        .flex_shrink_0()
        .children(shortcut.split('+').map(|key| {
            div()
                .px(px(7.))
                .py(px(3.))
                .rounded(px(5.))
                .border_1()
                .border_color(rgb(0x3c4653))
                .bg(rgb(0x242b35))
                .text_color(rgb(0xd3dce7))
                .text_size(px(11.))
                .whitespace_nowrap()
                .child(key.trim().to_string())
        }))
}
pub(super) fn state_label(state: &str) -> &'static str {
    match state {
        "connecting" => "正在连接",
        "listening" => "正在聆听",
        "processing" => "正在整理",
        "done" => "识别完成",
        "error" => "需要处理",
        _ => "准备就绪",
    }
}
pub(super) fn state_color(state: &str) -> u32 {
    match state {
        "error" => DANGER,
        "connecting" | "processing" => WARNING,
        _ => MINT,
    }
}
pub(super) fn badge(label: impl Into<SharedString>, color: u32) -> Div {
    row()
        .gap_2()
        .flex_shrink_0()
        .px_3()
        .py(px(5.))
        .rounded_full()
        .bg(rgba((color << 8) | 18))
        .text_size(px(11.))
        .text_color(rgb(color))
        .child(
            div()
                .size(px(6.))
                .flex_shrink_0()
                .rounded_full()
                .bg(rgb(color)),
        )
        .child(label.into())
}

#[derive(Clone, Copy)]
pub(super) enum Glyph {
    Home,
    Wave,
    Engine,
    Keyboard,
    Mic,
    Copy,
    Settings,
    Close,
    Play,
    Stop,
    Check,
    Arrow,
    Spark,
}

/// A single 24-unit vector grid keeps stroke weight and hit targets consistent.
pub(super) fn icon(glyph: Glyph, color: u32, side: f32) -> Canvas<()> {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let paths: &[&[(f32, f32)]] = match glyph {
                Glyph::Home => &[
                    &[(3., 10.), (12., 3.), (21., 10.)],
                    &[
                        (5., 9.),
                        (5., 21.),
                        (10., 21.),
                        (10., 14.),
                        (14., 14.),
                        (14., 21.),
                        (19., 21.),
                        (19., 9.),
                    ],
                ],
                Glyph::Wave => &[
                    &[(3., 10.), (3., 14.)],
                    &[(7., 6.), (7., 18.)],
                    &[(12., 3.), (12., 21.)],
                    &[(17., 7.), (17., 17.)],
                    &[(21., 10.), (21., 14.)],
                ],
                Glyph::Engine => &[
                    &[(6., 6.), (18., 6.), (18., 18.), (6., 18.), (6., 6.)],
                    &[(9., 9.), (15., 9.), (15., 15.), (9., 15.), (9., 9.)],
                    &[(9., 3.), (9., 6.)],
                    &[(15., 3.), (15., 6.)],
                    &[(9., 18.), (9., 21.)],
                    &[(15., 18.), (15., 21.)],
                    &[(3., 9.), (6., 9.)],
                    &[(3., 15.), (6., 15.)],
                    &[(18., 9.), (21., 9.)],
                    &[(18., 15.), (21., 15.)],
                ],
                Glyph::Keyboard => &[
                    &[(3., 6.), (21., 6.), (21., 18.), (3., 18.), (3., 6.)],
                    &[(6., 10.), (8., 10.)],
                    &[(11., 10.), (13., 10.)],
                    &[(16., 10.), (18., 10.)],
                    &[(7., 14.), (17., 14.)],
                ],
                Glyph::Mic => &[
                    &[
                        (9., 11.),
                        (9., 5.),
                        (10., 3.),
                        (14., 3.),
                        (15., 5.),
                        (15., 11.),
                        (14., 13.),
                        (10., 13.),
                        (9., 11.),
                    ],
                    &[
                        (5., 10.),
                        (5., 12.),
                        (7., 16.),
                        (10., 18.),
                        (14., 18.),
                        (17., 16.),
                        (19., 12.),
                        (19., 10.),
                    ],
                    &[(12., 18.), (12., 22.)],
                    &[(8., 22.), (16., 22.)],
                ],
                Glyph::Copy => &[
                    &[(8., 8.), (20., 8.), (20., 21.), (8., 21.), (8., 8.)],
                    &[(16., 5.), (16., 3.), (3., 3.), (3., 16.), (5., 16.)],
                ],
                Glyph::Settings => &[
                    &[(3., 6.), (8., 6.)],
                    &[(12., 6.), (21., 6.)],
                    &[(10., 3.), (10., 9.)],
                    &[(3., 12.), (14., 12.)],
                    &[(18., 12.), (21., 12.)],
                    &[(16., 9.), (16., 15.)],
                    &[(3., 18.), (6., 18.)],
                    &[(10., 18.), (21., 18.)],
                    &[(8., 15.), (8., 21.)],
                ],
                Glyph::Close => &[&[(6., 6.), (18., 18.)], &[(18., 6.), (6., 18.)]],
                Glyph::Play => &[&[(8., 4.), (20., 12.), (8., 20.), (8., 4.)]],
                Glyph::Stop => &[&[(6., 6.), (18., 6.), (18., 18.), (6., 18.), (6., 6.)]],
                Glyph::Check => &[&[(4., 12.), (9., 17.), (20., 6.)]],
                Glyph::Arrow => &[
                    &[(5., 12.), (20., 12.)],
                    &[(14., 6.), (20., 12.), (14., 18.)],
                ],
                Glyph::Spark => &[&[
                    (12., 2.),
                    (15., 9.),
                    (22., 12.),
                    (15., 15.),
                    (12., 22.),
                    (9., 15.),
                    (2., 12.),
                    (9., 9.),
                    (12., 2.),
                ]],
            };
            let scale = f32::from(bounds.size.width) / 24.;
            for points in paths {
                let mut path = PathBuilder::stroke(px(1.6 * scale));
                for (i, &(x, y)) in points.iter().enumerate() {
                    let p = bounds.origin + point(px(x * scale), px(y * scale));
                    if i == 0 {
                        path.move_to(p);
                    } else {
                        path.line_to(p);
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, rgb(color));
                }
            }
        },
    )
    .size(px(side))
    .flex_shrink_0()
}
pub(super) fn icon_button(id: &'static str, glyph: Glyph, color: u32, side: f32) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(side))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(side * 0.27))
        .cursor_pointer()
        .hover(|s| s.bg(rgba(0xffffff12)))
        .child(icon(glyph, color, side * 0.48))
}
