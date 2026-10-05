use super::*;
use crate::i18n::Message;

impl SettingsPane {
    fn choose_terminal_effect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.terminal_effect_picker.is_some() {
            return;
        }
        let previous = self.runtime.terminal_effects.clone();
        let language = crate::gpui_shell::config::ui_language(cx);
        let handle = Window::window_handle(window);
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(language.text(Message::TerminalEffectPrompt).into()),
        });
        self.terminal_effect_picker = Some(cx.spawn(async move |entity, cx| {
            let result = picked.await;
            let _ = entity.update(cx, |this, cx| {
                if let Some(task) = this.terminal_effect_picker.take() {
                    task.detach();
                }
                if this.runtime.terminal_effects != previous {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(Ok(Some(paths)))
                        if paths.first().and_then(|path| path.to_str()).is_some() =>
                    {
                        let path = paths[0].to_str().expect("checked UTF-8 path");
                        this.persist(
                            &[
                                ("terminal_effect_path", path.to_owned()),
                                ("terminal_effect_enabled", "false".into()),
                            ],
                            cx,
                        );
                    },
                    Ok(Ok(None)) => {},
                    _ => {
                        if let Err(error) = handle.update(cx, |_, window, cx| {
                            crate::gpui_shell::toast::toast(
                                window,
                                cx,
                                crate::gpui_shell::toast::ToastKind::Warning,
                                crate::gpui_shell::config::ui_language(cx)
                                    .text(Message::TerminalEffectPickFailed),
                            );
                        }) {
                            log::debug!("effect picker window released: {error}");
                        }
                    },
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn toggle_terminal_effect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let language = crate::gpui_shell::config::ui_language(cx);
        if self.runtime.terminal_effects.enabled {
            self.persist(&[("terminal_effect_enabled", "false".into())], cx);
            return;
        }
        if self.runtime.terminal_effects.path.is_none() {
            crate::gpui_shell::toast::toast(
                window,
                cx,
                crate::gpui_shell::toast::ToastKind::Warning,
                language.text(Message::TerminalEffectMissing),
            );
            return;
        }
        let expected = self.runtime.terminal_effects.clone();
        let entity = cx.weak_entity();
        window.open_dialog(cx, move |dialog, window, _| {
            let expected = expected.clone();
            let entity = entity.clone();
            confirm_dialog(
                dialog,
                window,
                language.text(Message::TerminalEffectConfirm),
                SharedString::from(language.text(Message::TerminalEffectConfirmDescription)),
                language.text(Message::TerminalEffectEnable),
                language.text(Message::WallpaperShaderCancel),
                ButtonVariant::Primary,
            )
            .on_ok(move |_, _, cx| {
                let _ = entity.update(cx, |this, cx| {
                    this.runtime = RuntimeSettings::load();
                    if this.runtime.terminal_effects == expected {
                        this.persist(&[("terminal_effect_enabled", "true".into())], cx);
                    }
                    cx.notify();
                });
                true
            })
        });
    }

    pub(super) fn terminal_effect_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let config = &self.runtime.terminal_effects;
        let busy = self.terminal_effect_picker.is_some();
        let available = super::super::wallpaper::shader_available();
        let path = config.path.as_deref().map(|path| {
            std::path::Path::new(path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(path)
                .to_owned()
        });
        self.row_with_reset(
            language.text(Message::TerminalEffectTitle),
            language.text(if available {
                Message::TerminalEffectDescription
            } else {
                Message::TerminalEffectUnsupported
            }),
            config.enabled || config.path.is_some(),
            |this, _, cx| {
                this.terminal_effect_picker.take();
                this.persist(
                    &[
                        ("terminal_effect_enabled", "false".into()),
                        ("terminal_effect_path", String::new()),
                    ],
                    cx,
                );
            },
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    NebulaButton::new("terminal-effect-choose")
                        .label(language.text(if busy {
                            Message::TerminalEffectChoosing
                        } else {
                            Message::TerminalEffectChoose
                        }))
                        .disabled(busy || !available)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.choose_terminal_effect(window, cx)
                        })),
                )
                .child(
                    NebulaButton::new("terminal-effect-toggle")
                        .label(language.text(if config.enabled {
                            Message::TerminalEffectDisable
                        } else {
                            Message::TerminalEffectEnable
                        }))
                        .disabled(busy || !available)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.toggle_terminal_effect(window, cx)
                        })),
                )
                .child(
                    NebulaButton::new("terminal-effect-reload")
                        .label(language.text(Message::TerminalEffectReload))
                        .disabled(busy || !available || !config.enabled)
                        .on_click(cx.listener(|_, _, _, cx| {
                            super::super::wallpaper::reload_terminal_effects(cx)
                        })),
                )
                .when_some(path, |row, path| {
                    row.child(
                        div()
                            .max_w(px(180.0))
                            .truncate()
                            .text_color(cx.theme().muted_foreground)
                            .child(path),
                    )
                }),
            cx,
        )
    }
}
