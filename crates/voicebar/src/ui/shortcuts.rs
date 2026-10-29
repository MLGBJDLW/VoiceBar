use super::shortcut_capture::{Capture, CaptureUpdate};
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ShortcutField {
    Toggle,
    Cancel,
}

impl ShortcutField {
    fn index(self) -> usize {
        match self {
            Self::Toggle => 0,
            Self::Cancel => 1,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Toggle => "开始 / 结束录音",
            Self::Cancel => "取消本次录音",
        }
    }

    fn value(self, config: &AppConfig) -> &str {
        match self {
            Self::Toggle => &config.hotkey,
            Self::Cancel => &config.cancel_hotkey,
        }
    }

    fn set(self, config: &mut AppConfig, value: String) {
        match self {
            Self::Toggle => config.hotkey = value,
            Self::Cancel => config.cancel_hotkey = value,
        }
    }
}

// Validate only shortcuts here; unrelated unfinished form fields should not
// prevent recording a chord. OS registration remains the save transaction's job.
fn replace_shortcut(
    draft: &mut AppConfig,
    field: ShortcutField,
    value: String,
) -> Result<bool, String> {
    let mut next = draft.clone();
    field.set(&mut next, value);
    next.parsed_hotkeys()?;
    let changed = field.value(draft) != field.value(&next);
    *draft = next;
    Ok(changed)
}

fn restore_shortcuts(draft: &mut AppConfig) -> bool {
    let defaults = AppConfig::default();
    let changed = draft.hotkey != defaults.hotkey || draft.cancel_hotkey != defaults.cancel_hotkey;
    draft.hotkey = defaults.hotkey;
    draft.cancel_hotkey = defaults.cancel_hotkey;
    changed
}

pub(super) struct ShortcutRecording {
    field: ShortcutField,
    capture: Capture,
    preview: String,
}

impl Settings {
    pub(super) fn subscribe_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for field in [ShortcutField::Toggle, ShortcutField::Cancel] {
            self._subscriptions.push(cx.on_blur(
                &self.shortcut_focus[field.index()],
                window,
                move |this, _, cx| {
                    if this
                        .shortcut_recording
                        .as_ref()
                        .is_some_and(|r| r.field == field)
                    {
                        this.cancel_shortcut(cx);
                    }
                },
            ));
        }
        self._subscriptions
            .push(cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    this.cancel_shortcut(cx);
                }
            }));
    }

    pub(super) fn cancel_shortcut(&mut self, cx: &mut Context<Self>) {
        if self.shortcut_recording.take().is_some() {
            self.message = "已取消快捷键录入。".into();
            cx.notify();
        }
    }

    pub(super) fn begin_shortcut(
        &mut self,
        field: ShortcutField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_shortcut(cx);
        if self.runtime.active.lock().unwrap().is_some() {
            self.message = "请先结束录音，再设置快捷键。".into();
            cx.notify();
            return;
        }
        self.shortcut_focus[field.index()].focus(window);
        match Capture::start(native_handle(window)) {
            Ok(capture) => {
                self.shortcut_recording = Some(ShortcutRecording {
                    field,
                    capture,
                    preview: String::new(),
                });
                self.message = format!(
                    "正在录入「{}」：按下快捷键并松开；再次点击录入框可取消。",
                    field.label()
                );
            }
            Err(error) => self.message = error,
        }
        cx.notify();
    }

    fn apply_shortcut(&mut self, field: ShortcutField, value: String, cx: &mut Context<Self>) {
        self.message = match replace_shortcut(&mut self.draft, field, value) {
            Ok(changed) => {
                self.dirty |= changed;
                "快捷键已更新，点击「保存更改」后立即生效。".into()
            }
            Err(error) => error,
        };
        cx.notify();
    }

    pub(super) fn poll_shortcut(&mut self, cx: &mut Context<Self>) {
        let Some(recording) = self.shortcut_recording.as_ref() else {
            return;
        };
        let update = recording.capture.poll();
        match update {
            CaptureUpdate::Pending(preview) => {
                let recording = self.shortcut_recording.as_mut().unwrap();
                if recording.preview != preview {
                    recording.preview = preview;
                    cx.notify();
                }
            }
            CaptureUpdate::Complete(result) => {
                let field = self.shortcut_recording.take().unwrap().field;
                match result {
                    Ok(value) => self.apply_shortcut(field, value, cx),
                    Err(error) => {
                        self.message = error;
                        cx.notify();
                    }
                }
            }
            CaptureUpdate::Canceled(reason) => {
                self.shortcut_recording = None;
                self.message = reason;
                cx.notify();
            }
        }
    }

    fn shortcut_field(&self, field: ShortcutField, cx: &Context<Self>) -> Div {
        let recording = self
            .shortcut_recording
            .as_ref()
            .filter(|r| r.field == field);
        let active = recording.is_some();
        let value = recording.map_or_else(
            || field.value(&self.draft),
            |recording| recording.preview.as_str(),
        );
        column().gap_2().min_w_0().child(note(field.label())).child(
            row()
                .flex_wrap()
                .gap_2()
                .child(
                    row()
                        .id(("shortcut-record", field.index()))
                        .track_focus(&self.shortcut_focus[field.index()])
                        .tab_index(0)
                        .flex_1()
                        .min_w(px(200.))
                        .min_h(px(48.))
                        .px_3()
                        .py_2()
                        .rounded(px(8.))
                        .border_1()
                        .border_color(rgb(if active { MINT } else { LINE }))
                        .bg(rgb(if active { TINT } else { INSET }))
                        .cursor_pointer()
                        .hover(|style| style.border_color(rgb(MINT)))
                        .focus(|style| style.border_color(rgb(MINT)))
                        .child(icon(
                            Glyph::Keyboard,
                            if active { MINT } else { MUTED },
                            17.,
                        ))
                        .child(div().flex_1().min_w_0().child(if value.is_empty() {
                            note(if active {
                                "请按下快捷键…"
                            } else {
                                "已禁用"
                            })
                        } else {
                            keycaps(value)
                        }))
                        .child(
                            note(if active {
                                "点击取消"
                            } else {
                                "点击录入"
                            })
                            .text_size(px(10.)),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this
                                .shortcut_recording
                                .as_ref()
                                .is_some_and(|r| r.field == field)
                            {
                                this.cancel_shortcut(cx);
                            } else {
                                this.begin_shortcut(field, window, cx);
                            }
                        }))
                        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                            if !event.is_held
                                && this.shortcut_recording.is_none()
                                && matches!(event.keystroke.key.as_str(), "enter" | "space")
                            {
                                window.prevent_default();
                                cx.stop_propagation();
                                this.begin_shortcut(field, window, cx);
                            }
                        }))
                        .on_mouse_down_out(cx.listener(move |this, _, _, cx| {
                            if this
                                .shortcut_recording
                                .as_ref()
                                .is_some_and(|r| r.field == field)
                            {
                                this.cancel_shortcut(cx);
                            }
                        })),
                )
                .child(
                    button(("shortcut-default", field.index()), "恢复默认").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.cancel_shortcut(cx);
                            let defaults = AppConfig::default();
                            this.apply_shortcut(field, field.value(&defaults).to_owned(), cx);
                        },
                    )),
                )
                .when(field == ShortcutField::Cancel, |row| {
                    row.child(
                        button("shortcut-disable-cancel", "禁用").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.cancel_shortcut(cx);
                                this.apply_shortcut(ShortcutField::Cancel, String::new(), cx);
                            },
                        )),
                    )
                }),
        )
    }

    pub(super) fn shortcut_controls(&self, cx: &Context<Self>) -> Div {
        let defaults = AppConfig::default();
        column()
            .gap_4()
            .child(
                row()
                    .justify_between()
                    .flex_wrap()
                    .child(title("一个快捷键，贯穿整个桌面"))
                    .child(
                        button("shortcuts-restore-all", "全部恢复默认")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_shortcut(cx);
                                this.dirty |= restore_shortcuts(&mut this.draft);
                                this.message = "快捷键已恢复默认，点击「保存更改」后生效。".into();
                                cx.notify();
                            })),
                    ),
            )
            .child(note("点击录入框，按下快捷键并松开即可。支持组合键、F1–F24 和 Escape，保存后立即生效。"))
            .child(self.shortcut_field(ShortcutField::Toggle, cx))
            .child(self.shortcut_field(ShortcutField::Cancel, cx))
            .child(note(format!("默认：开始 / 结束为 {}，取消录音为 {}。录入时再次点击可取消，15 秒未完成自动退出。", defaults.hotkey, defaults.cancel_hotkey)).text_size(px(11.)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn duplicate_chord_is_rejected_without_changing_the_draft() {
        let mut draft = AppConfig::default();
        let original = draft.clone();
        assert!(
            replace_shortcut(&mut draft, ShortcutField::Cancel, "Alt+Ctrl+Space".into()).is_err()
        );
        assert_eq!(draft, original);
        assert!(replace_shortcut(&mut draft, ShortcutField::Toggle, String::new()).is_err());
        assert_eq!(draft, original);
    }

    #[test]
    fn restore_and_disable_only_change_shortcuts_and_do_not_save() {
        let mut draft = AppConfig::default();
        draft.provider.model = "unfinished model".into();
        draft.appearance.scale = 1.3;
        let original = draft.clone();
        assert!(replace_shortcut(&mut draft, ShortcutField::Toggle, "Alt+F8".into()).unwrap());
        assert!(replace_shortcut(&mut draft, ShortcutField::Cancel, String::new()).unwrap());
        assert!(draft.parsed_hotkeys().unwrap().1.is_none());
        assert!(restore_shortcuts(&mut draft));
        assert_eq!(draft, original);
        assert!(!restore_shortcuts(&mut draft));
    }
}
