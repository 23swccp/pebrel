use crate::RawSettings;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EffectAnimation {
    Off,
    #[default]
    Focused,
    Always,
}

impl EffectAnimation {
    pub const VALUES: &'static [&'static str] = &["off", "focused", "always"];

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "off" => Some(Self::Off),
            "focused" => Some(Self::Focused),
            "always" => Some(Self::Always),
            _ => None,
        }
    }

    pub fn settings_value(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Focused => "focused",
            Self::Always => "always",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalEffects {
    pub enabled: bool,
    pub path: Option<String>,
    pub animation: EffectAnimation,
}

impl TerminalEffects {
    pub fn from_raw(raw: &RawSettings) -> Self {
        Self {
            enabled: raw.bool_on("terminal_effect_enabled").unwrap_or(false),
            path: raw.value("terminal_effect_path").map(str::to_owned),
            animation: raw
                .value("terminal_effect_animation")
                .and_then(EffectAnimation::parse)
                .unwrap_or_default(),
        }
    }

    pub fn request(&self) -> Result<Option<&str>, &'static str> {
        if !self.enabled {
            return Ok(None);
        }
        self.path
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .map(Some)
            .ok_or("terminal effect is enabled without a WGSL source")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_activation_is_independent_of_background_and_animation() {
        let text = "terminal_effect_path=效果.wgsl\nterminal_effect_animation=always\nbackground_effect_neon_vortex=true\n";
        let raw = RawSettings::from_text(text);
        assert_eq!(TerminalEffects::from_raw(&raw).request(), Ok(None));
        let updated = crate::apply_updates(text, &[("terminal_effect_enabled", "true".into())]);
        let settings = crate::RuntimeSettings::from_raw(&RawSettings::from_text(&updated));
        assert_eq!(settings.terminal_effects.request(), Ok(Some("效果.wgsl")));
        assert_eq!(settings.terminal_effects.animation, EffectAnimation::Always);
        assert!(settings.background_effects.neon_vortex);
    }

    #[test]
    fn defaults_and_invalid_values_do_not_authorize_execution() {
        assert_eq!(TerminalEffects::default().animation, EffectAnimation::Focused);
        assert!(
            TerminalEffects::from_raw(&RawSettings::from_text("terminal_effect_enabled=true\n"))
                .request()
                .is_err()
        );
        let settings = TerminalEffects::from_raw(&RawSettings::from_text(
            "terminal_effect_enabled=trueish\nterminal_effect_animation=unknown\n",
        ));
        assert_eq!(settings, TerminalEffects::default());
        for value in EffectAnimation::VALUES {
            assert_eq!(EffectAnimation::parse(value).unwrap().settings_value(), *value);
        }
    }
}
