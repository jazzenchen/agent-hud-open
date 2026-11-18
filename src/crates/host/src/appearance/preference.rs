#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Preference {
    Light,
    Dark,
    System,
}

impl Preference {
    pub(super) fn parse(value: Option<&str>) -> Self {
        match value {
            Some("light") => Self::Light,
            Some("dark") => Self::Dark,
            _ => Self::System,
        }
    }

    pub(super) fn light(self, system_light: bool) -> bool {
        match self {
            Self::Light => true,
            Self::Dark => false,
            Self::System => system_light,
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }

    pub(super) fn background(self, system_light: bool) -> (u8, u8, u8, u8) {
        if self.light(system_light) {
            (245, 245, 247, 255)
        } else {
            (40, 40, 40, 255)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Preference;

    #[test]
    fn saved_appearance_wins_over_the_opposite_system_theme() {
        let light = Preference::parse(Some("light"));
        assert_eq!(light.name(), "light");
        assert_eq!(light.background(false), (245, 245, 247, 255));
        let dark = Preference::parse(Some("dark"));
        assert_eq!(dark.name(), "dark");
        assert_eq!(dark.background(true), (40, 40, 40, 255));
    }

    #[test]
    fn follow_system_uses_the_current_theme_and_invalid_settings_fall_back() {
        for saved in [None, Some("system"), Some("invalid")] {
            let preference = Preference::parse(saved);
            assert_eq!(preference.name(), "system");
            assert_eq!(preference.background(true), (245, 245, 247, 255));
            assert_eq!(preference.background(false), (40, 40, 40, 255));
        }
    }
}
