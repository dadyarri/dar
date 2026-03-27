#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Locale(String);

impl Locale {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub fn detect_locale() -> Locale {
    let locale = sys_locale::get_locale().unwrap_or_else(|| "en".to_string());
    Locale(normalize_locale(&locale))
}

fn normalize_locale(locale: &str) -> String {
    let without_encoding = locale.split('.').next().unwrap_or(locale);
    without_encoding.replace('_', "-")
}

#[cfg(test)]
mod tests {
    use super::normalize_locale;

    #[test]
    fn normalizes_locale_from_os() {
        assert_eq!(normalize_locale("ru_RU.UTF-8"), "ru-RU");
    }

    #[test]
    fn leaves_simple_locale_untouched() {
        assert_eq!(normalize_locale("en"), "en");
    }
}




