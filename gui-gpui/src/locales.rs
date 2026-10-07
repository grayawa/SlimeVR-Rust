// Generated: retain every original SlimeVR language.
pub const LOCALES: &[&str] = &[
    "ar", "cs", "da", "de", "el", "en", "en-x-owo", "es-419", "es-ES", "et", "fi", "fr", "he",
    "it", "ja", "ko", "lt", "nb-NO", "nl", "pl", "pt-BR", "ru", "sv-SE", "th", "tr", "uk", "vi",
    "zh-Hans", "zh-Hant",
];
pub fn source(locale: &str) -> Option<&'static str> {
    match locale {
        "ar" => Some(include_str!("../../gui/public/i18n/ar/translation.ftl")),
        "cs" => Some(include_str!("../../gui/public/i18n/cs/translation.ftl")),
        "da" => Some(include_str!("../../gui/public/i18n/da/translation.ftl")),
        "de" => Some(include_str!("../../gui/public/i18n/de/translation.ftl")),
        "el" => Some(include_str!("../../gui/public/i18n/el/translation.ftl")),
        "en" => Some(include_str!("../../gui/public/i18n/en/translation.ftl")),
        "en-x-owo" => Some(include_str!(
            "../../gui/public/i18n/en-x-owo/translation.ftl"
        )),
        "es-419" => Some(include_str!("../../gui/public/i18n/es-419/translation.ftl")),
        "es-ES" => Some(include_str!("../../gui/public/i18n/es-ES/translation.ftl")),
        "et" => Some(include_str!("../../gui/public/i18n/et/translation.ftl")),
        "fi" => Some(include_str!("../../gui/public/i18n/fi/translation.ftl")),
        "fr" => Some(include_str!("../../gui/public/i18n/fr/translation.ftl")),
        "he" => Some(include_str!("../../gui/public/i18n/he/translation.ftl")),
        "it" => Some(include_str!("../../gui/public/i18n/it/translation.ftl")),
        "ja" => Some(include_str!("../../gui/public/i18n/ja/translation.ftl")),
        "ko" => Some(include_str!("../../gui/public/i18n/ko/translation.ftl")),
        "lt" => Some(include_str!("../../gui/public/i18n/lt/translation.ftl")),
        "nb-NO" => Some(include_str!("../../gui/public/i18n/nb-NO/translation.ftl")),
        "nl" => Some(include_str!("../../gui/public/i18n/nl/translation.ftl")),
        "pl" => Some(include_str!("../../gui/public/i18n/pl/translation.ftl")),
        "pt-BR" => Some(include_str!("../../gui/public/i18n/pt-BR/translation.ftl")),
        "ru" => Some(include_str!("../../gui/public/i18n/ru/translation.ftl")),
        "sv-SE" => Some(include_str!("../../gui/public/i18n/sv-SE/translation.ftl")),
        "th" => Some(include_str!("../../gui/public/i18n/th/translation.ftl")),
        "tr" => Some(include_str!("../../gui/public/i18n/tr/translation.ftl")),
        "uk" => Some(include_str!("../../gui/public/i18n/uk/translation.ftl")),
        "vi" => Some(include_str!("../../gui/public/i18n/vi/translation.ftl")),
        "zh-Hans" => Some(include_str!(
            "../../gui/public/i18n/zh-Hans/translation.ftl"
        )),
        "zh-Hant" => Some(include_str!(
            "../../gui/public/i18n/zh-Hant/translation.ftl"
        )),
        _ => None,
    }
}

/// Original language picker display names.
pub fn name(locale: &str) -> &str {
    match locale {
        "ar" => "عربى",
        "cs" => "Čeština",
        "da" => "Dansk",
        "de" => "Deutsch",
        "en" => "English",
        "es-419" => "Español Latinoamericano",
        "es-ES" => "Español España",
        "et" => "Eesti",
        "fi" => "Suomi",
        "fr" => "Français",
        "it" => "Italiano",
        "ja" => "日本語",
        "ko" => "한국어",
        "nb-NO" => "Norsk bokmål",
        "nl" => "Nederlands",
        "pl" => "Polski",
        "pt-BR" => "Português Brasileiro",
        "ru" => "Русский",
        "sv-SE" => "Svenska (Sverige)",
        "uk" => "Українська",
        "th" => "ไทย",
        "vi" => "Tiếng Việt",
        "zh-Hans" => "简体中文",
        "zh-Hant" => "繁體中文",
        "en-x-owo" => "Engwish~ OwO",
        _ => locale,
    }
}
