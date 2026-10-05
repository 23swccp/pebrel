//! Explicit local authorization for background effects. Renderers own capabilities.
use crate::RawSettings;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackgroundEffects {
    pub grain: bool,
    pub wgsl: bool,
    pub wgsl_path: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgroundEffectRequest<'a> {
    Disabled,
    Grain,
    Wgsl(&'a str),
}

impl BackgroundEffects {
    pub const KEYS: &'static [&'static str] =
        &["background_effect_grain", "background_wgsl_enabled", "background_wgsl_path"];

    pub fn from_raw(raw: &RawSettings) -> Self {
        Self {
            grain: raw.bool_on("background_effect_grain").unwrap_or(false),
            wgsl: raw.bool_on("background_wgsl_enabled").unwrap_or(false),
            wgsl_path: raw.value("background_wgsl_path").map(str::to_owned),
        }
    }

    /// A path alone never enables execution. Explicit WGSL wins over controlled grain.
    /// This performs no I/O, starts no compiler and subscribes to no playback tick.
    pub fn request(&self) -> Result<BackgroundEffectRequest<'_>, &'static str> {
        if self.wgsl {
            return self
                .wgsl_path
                .as_deref()
                .map(BackgroundEffectRequest::Wgsl)
                .ok_or("background_wgsl_enabled requires background_wgsl_path");
        }
        Ok(if self.grain {
            BackgroundEffectRequest::Grain
        } else {
            BackgroundEffectRequest::Disabled
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RuntimeSettings, apply_updates};

    #[test]
    fn wgsl_authorization_round_trips_and_retired_keys_cannot_grant_execution() {
        let text = apply_updates(
            "background_glsl_path=untrusted.glsl\nunknown=keep\n",
            &[
                ("background_wgsl_enabled", "true".into()),
                ("background_wgsl_path", "背景.wgsl".into()),
            ],
        );
        let loaded = RuntimeSettings::from_raw(&RawSettings::from_text(&text));
        assert_eq!(
            loaded.background_effects.request(),
            Ok(BackgroundEffectRequest::Wgsl("背景.wgsl"))
        );
        let both = apply_updates(&text, &[("background_glsl_enabled", "true".into())]);
        assert_eq!(
            RuntimeSettings::from_raw(&RawSettings::from_text(&both)).background_effects.request(),
            Ok(BackgroundEffectRequest::Wgsl("背景.wgsl"))
        );
        let missing =
            RawSettings::from_text("background_wgsl_enabled=true\nbackground_wgsl_path= \n");
        assert!(BackgroundEffects::from_raw(&missing).request().is_err());
        let inactive = RawSettings::from_text(
            "background_wgsl_enabled=false\nbackground_wgsl_path=missing.wgsl\n",
        );
        assert_eq!(
            BackgroundEffects::from_raw(&inactive).request(),
            Ok(BackgroundEffectRequest::Disabled)
        );
        assert!(text.contains("unknown=keep"));
    }

    #[test]
    fn older_and_invalid_configs_do_not_authorize_an_effect() {
        for value in ["", "trueish", "2", "enabled"] {
            let text = format!(
                "background_effect_grain={value}\nbackground_wgsl_enabled={value}\nbackground_wgsl_path=missing.wgsl\n"
            );
            assert_eq!(
                BackgroundEffects::from_raw(&RawSettings::from_text(&text)).request(),
                Ok(BackgroundEffectRequest::Disabled)
            );
        }
        assert_eq!(
            RuntimeSettings::from_raw(&RawSettings::default()).background_effects,
            BackgroundEffects::default()
        );
    }

    #[test]
    fn explicit_false_wins_and_a_path_does_not_grant_authorization() {
        for value in ["0", "false", "off", "no", " FALSE "] {
            let text = format!(
                "background_effect_grain={value}\nbackground_wgsl_enabled={value}\nbackground_wgsl_path=missing.wgsl\n"
            );
            assert_eq!(
                BackgroundEffects::from_raw(&RawSettings::from_text(&text)).request(),
                Ok(BackgroundEffectRequest::Disabled)
            );
        }
    }

    #[test]
    fn explicit_grain_and_wgsl_choose_one_pipeline() {
        for value in ["1", "true", "on", "yes", " TRUE "] {
            let mut text = format!("background_effect_grain={value}\n");
            assert_eq!(
                BackgroundEffects::from_raw(&RawSettings::from_text(&text)).request(),
                Ok(BackgroundEffectRequest::Grain)
            );
            text.push_str(&format!(
                "background_wgsl_enabled={value}\nbackground_wgsl_path=背景.wgsl\n"
            ));
            assert_eq!(
                BackgroundEffects::from_raw(&RawSettings::from_text(&text)).request(),
                Ok(BackgroundEffectRequest::Wgsl("背景.wgsl"))
            );
        }
    }

    #[test]
    fn enabled_missing_source_is_an_error_instead_of_silent_fallback() {
        for path in ["", "  "] {
            let text = format!(
                "background_wgsl_enabled=true\nbackground_wgsl_path={path}\nbackground_effect_grain=true\n"
            );
            assert!(BackgroundEffects::from_raw(&RawSettings::from_text(&text)).request().is_err());
        }
    }

    #[test]
    fn authorization_round_trips_without_modifying_media_or_unknown_preferences() {
        let text = apply_updates(
            "background_image=poster.png\nfuture_setting=keep\n",
            &[
                ("background_effect_grain", "true".into()),
                ("background_wgsl_enabled", "false".into()),
                ("background_wgsl_path", "C:/主题/背景.wgsl".into()),
            ],
        );
        let settings = RuntimeSettings::from_raw(&RawSettings::from_text(&text));
        assert_eq!(settings.background_effects.request(), Ok(BackgroundEffectRequest::Grain));
        assert_eq!(settings.background_effects.wgsl_path.as_deref(), Some("C:/主题/背景.wgsl"));
        assert_eq!(settings.background_image.as_deref(), Some("poster.png"));
        assert!(text.contains("future_setting=keep"));
    }
}
