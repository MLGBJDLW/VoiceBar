//! Shared visual language for settings and the no-focus dictation overlay.
//!
//! 所有页面都从这里取"积木"：文字层级、卡片、按钮、分段选择器、徽标。
//! 想改整体风格时，优先改这里和 palette.rs，而不是去每个页面里改颜色。
use gpui::{prelude::*, *};
use gpui_component::{Theme, ThemeMode};

pub(super) use crate::palette::*;

pub(super) fn init_theme(cx: &mut App) {
    gpui_component::init(cx);
    Theme::change(ThemeMode::Dark, None, cx);
    let theme = Theme::global_mut(cx);
    theme.font_family = FONT_FAMILY.into();
    theme.font_size = px(13.);
    theme.radius = px(7.);
    // 输入框使用 background 作为底色：比卡片略暗，看起来是"凹进去"的。
    theme.colors.background = rgb(INSET).into();
    theme.colors.foreground = rgb(TEXT).into();
    theme.colors.border = rgb(LINE).into();
    theme.colors.input = rgb(LINE_STRONG).into();
    theme.colors.muted = rgb(RAISED).into();
    theme.colors.muted_foreground = rgb(MUTED).into();
    theme.colors.primary = rgb(MINT).into();
    theme.colors.primary_hover = rgb(0xc0f3e2).into();
    theme.colors.primary_active = rgb(0x86d8bb).into();
    theme.colors.primary_foreground = rgb(ON_MINT).into();
    theme.colors.ring = rgba(0xa3ead266).into();
    theme.colors.caret = rgb(MINT).into();
    theme.colors.selection = rgba(0xa3ead238).into();
    theme.colors.switch = rgb(0x30333a).into();
    theme.colors.switch_thumb = rgb(TEXT).into();
}

pub(super) fn column() -> Div {
    div().flex().flex_col().flex_shrink_0().gap_3()
}
pub(super) fn row() -> Div {
    div().flex().flex_shrink_0().items_center().gap_3()
}
/// 次要说明文字：12px、灰色。
pub(super) fn note(text: impl Into<SharedString>) -> Div {
    div()
        .flex_shrink_0()
        .text_size(px(12.))
        .text_color(rgb(MUTED))
        .line_height(px(19.))
        .child(text.into())
}
/// 卡片内的小标题：14px 半粗体。层级主要靠字重和颜色，而不是靠放大字号。
pub(super) fn title(text: impl Into<SharedString>) -> Div {
    div()
        .flex_shrink_0()
        .text_size(px(14.))
        .line_height(px(20.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT))
        .child(text.into())
}
/// 分组标签，例如侧栏里的"设置"。
pub(super) fn eyebrow(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(SUBTLE))
        .child(text.into())
}
/// 卡片：一组相关设置。细边框 + 比画布略亮的底色，不使用阴影。
pub(super) fn card() -> Div {
    column()
        .p_5()
        .gap_4()
        .rounded(px(12.))
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
        .gap(px(6.))
        .flex_shrink_0()
        .h(px(32.))
        .px(px(12.))
        .rounded(px(7.))
        .border_1()
        .border_color(rgb(LINE_STRONG))
        .bg(rgb(RAISED))
        .text_size(px(12.))
        .line_height(px(18.))
        .text_color(rgb(TEXT))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .cursor_pointer()
        .child(label.into())
}
/// 次级按钮。注意：GPUI 中同一个元素只能声明一次 hover，
/// 所以调用方不要再对返回值调用 `.hover(...)`。
pub(super) fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    button_base(id, label).hover(|s| s.bg(rgb(HOVER)).border_color(rgb(0x45484f)))
}
/// 主按钮：一页里通常只有一个。
pub(super) fn primary(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    button_base(id, label)
        .bg(rgb(MINT))
        .border_color(rgb(MINT))
        .text_color(rgb(ON_MINT))
        .font_weight(FontWeight::SEMIBOLD)
        .hover(|s| s.bg(rgb(0xc0f3e2)).border_color(rgb(0xc0f3e2)))
}
/// 分段选择器的外框：把一组互斥选项装进一条"轨道"里。
pub(super) fn segmented() -> Div {
    row()
        .flex_wrap()
        .gap_1()
        .p(px(3.))
        .rounded(px(9.))
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgb(INSET))
}
/// 分段选择器中的一个选项；选中时"浮起"并显示强调色。
pub(super) fn segment(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .h(px(28.))
        .px(px(12.))
        .rounded(px(6.))
        .text_size(px(12.))
        .whitespace_nowrap()
        .cursor_pointer()
        .when(selected, |s| {
            s.bg(rgb(HOVER))
                .text_color(rgb(TEXT))
                .font_weight(FontWeight::MEDIUM)
        })
        .when(!selected, |s| s.text_color(rgb(MUTED)))
        .hover(move |s| {
            if selected {
                s
            } else {
                s.bg(rgb(RAISED)).text_color(rgb(TEXT))
            }
        })
        .child(label.into())
}
/// 键帽：把 "Ctrl+Alt+Space" 拆成一个个小按键。
pub(super) fn keycaps(shortcut: &str) -> Div {
    row()
        .gap_1()
        .flex_wrap()
        .flex_shrink_0()
        .children(shortcut.split('+').map(|key| {
            div()
                .min_w(px(22.))
                .h(px(22.))
                .px(px(6.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .border_1()
                .border_b_2()
                .border_color(rgb(LINE_STRONG))
                .bg(rgb(RAISED))
                .text_color(rgb(0xd4d8de))
                .text_size(px(11.))
                .whitespace_nowrap()
                .child(key.trim().to_string())
        }))
}
pub(super) fn state_label(state: &str) -> &'static str {
    match state {
        "connecting" => "正在连接",
        "listening" => "正在聆听",
        "paused" => "写入暂停",
        "processing" => "正在整理",
        "done" => "识别完成",
        "error" => "需要处理",
        _ => "准备就绪",
    }
}
pub(super) fn state_color(state: &str) -> u32 {
    match state {
        "error" => DANGER,
        "connecting" | "processing" | "paused" => WARNING,
        _ => MINT,
    }
}
/// 状态徽标：小圆点 + 文字，底色是同色的 7% 透明度。
pub(super) fn badge(label: impl Into<SharedString>, color: u32) -> Div {
    row()
        .gap(px(6.))
        .flex_shrink_0()
        .h(px(24.))
        .px(px(9.))
        .rounded_full()
        .bg(rgba((color << 8) | 0x12))
        .text_size(px(11.))
        .text_color(rgb(color))
        .whitespace_nowrap()
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
