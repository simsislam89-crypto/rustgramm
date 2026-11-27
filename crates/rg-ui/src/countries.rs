//! Справочник стран, автоопределение кода страны и валидация номера телефона для экрана входа.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Country {
    pub iso: &'static str,
    pub name: &'static str,
    pub dial: &'static str,
    pub hint: &'static str,
}

/// Популярные страны в начале списка, далее в алфавитном порядке (70+ стран).
pub const COUNTRIES: &[Country] = &[
    Country { iso: "RU", name: "Россия", dial: "+7", hint: "999 123-45-67" },
    Country { iso: "KZ", name: "Казахстан", dial: "+7", hint: "701 123-45-67" },
    Country { iso: "US", name: "США", dial: "+1", hint: "970 287-6230" },
    Country { iso: "UA", name: "Украина", dial: "+380", hint: "50 123-45-67" },
    Country { iso: "BY", name: "Беларусь", dial: "+375", hint: "29 123-45-67" },
    Country { iso: "UZ", name: "Узбекистан", dial: "+998", hint: "90 123-45-67" },
    Country { iso: "DE", name: "Германия", dial: "+49", hint: "151 23456789" },
    Country { iso: "GB", name: "Великобритания", dial: "+44", hint: "7400 123456" },
    Country { iso: "TR", name: "Турция", dial: "+90", hint: "501 234 56 78" },
    Country { iso: "PL", name: "Польша", dial: "+48", hint: "512 345 678" },
    Country { iso: "CA", name: "Канада", dial: "+1", hint: "416 555-0199" },
    Country { iso: "FR", name: "Франция", dial: "+33", hint: "6 12 34 56 78" },
    Country { iso: "IT", name: "Италия", dial: "+39", hint: "312 345 6789" },
    Country { iso: "ES", name: "Испания", dial: "+34", hint: "612 34 56 78" },
    Country { iso: "NL", name: "Нидерланды", dial: "+31", hint: "6 12345678" },
    Country { iso: "AM", name: "Армения", dial: "+374", hint: "77 123456" },
    Country { iso: "AZ", name: "Азербайджан", dial: "+994", hint: "40 123 45 67" },
    Country { iso: "GE", name: "Грузия", dial: "+995", hint: "555 12 34 56" },
    Country { iso: "KG", name: "Кыргызстан", dial: "+996", hint: "700 123 456" },
    Country { iso: "MD", name: "Молдова", dial: "+373", hint: "62 123 456" },
    Country { iso: "TJ", name: "Таджикистан", dial: "+992", hint: "91 123 4567" },
    Country { iso: "TM", name: "Туркменистан", dial: "+993", hint: "66 123456" },
    Country { iso: "EE", name: "Эстония", dial: "+372", hint: "5123 4567" },
    Country { iso: "LV", name: "Латвия", dial: "+371", hint: "21 234 567" },
    Country { iso: "LT", name: "Литва", dial: "+370", hint: "612 34567" },
    Country { iso: "IL", name: "Израиль", dial: "+972", hint: "50-234-5678" },
    Country { iso: "AE", name: "ОАЭ", dial: "+971", hint: "50 123 4567" },
    Country { iso: "AT", name: "Австрия", dial: "+43", hint: "664 123456" },
    Country { iso: "AU", name: "Австралия", dial: "+61", hint: "412 345 678" },
    Country { iso: "AR", name: "Аргентина", dial: "+54", hint: "11 2345-6789" },
    Country { iso: "BE", name: "Бельгия", dial: "+32", hint: "470 12 34 56" },
    Country { iso: "BG", name: "Болгария", dial: "+359", hint: "48 123 456" },
    Country { iso: "BR", name: "Бразилия", dial: "+55", hint: "11 96123-4567" },
    Country { iso: "CH", name: "Швейцария", dial: "+41", hint: "78 123 45 67" },
    Country { iso: "CN", name: "Китай", dial: "+86", hint: "131 2345 6789" },
    Country { iso: "CY", name: "Кипр", dial: "+357", hint: "96 123456" },
    Country { iso: "CZ", name: "Чехия", dial: "+420", hint: "601 123 456" },
    Country { iso: "DK", name: "Дания", dial: "+45", hint: "32 12 34 56" },
    Country { iso: "EG", name: "Египет", dial: "+20", hint: "100 123 4567" },
    Country { iso: "FI", name: "Финляндия", dial: "+358", hint: "41 2345678" },
    Country { iso: "GR", name: "Греция", dial: "+30", hint: "691 234 5678" },
    Country { iso: "HK", name: "Гонконг", dial: "+852", hint: "5123 4567" },
    Country { iso: "HR", name: "Хорватия", dial: "+385", hint: "92 123 4567" },
    Country { iso: "HU", name: "Венгрия", dial: "+36", hint: "20 123 4567" },
    Country { iso: "ID", name: "Индонезия", dial: "+62", hint: "812-345-678" },
    Country { iso: "IE", name: "Ирландия", dial: "+353", hint: "85 012 3456" },
    Country { iso: "IN", name: "Индия", dial: "+91", hint: "81234 56789" },
    Country { iso: "JP", name: "Япония", dial: "+81", hint: "90-1234-5678" },
    Country { iso: "KR", name: "Южная Корея", dial: "+82", hint: "10-2000-0000" },
    Country { iso: "ME", name: "Черногория", dial: "+382", hint: "67 622 901" },
    Country { iso: "MX", name: "Мексика", dial: "+52", hint: "222 123 4567" },
    Country { iso: "MY", name: "Малайзия", dial: "+60", hint: "12-345 6789" },
    Country { iso: "NO", name: "Норвегия", dial: "+47", hint: "406 12 345" },
    Country { iso: "NZ", name: "Новая Зеландия", dial: "+64", hint: "21 123 456" },
    Country { iso: "PH", name: "Филиппины", dial: "+63", hint: "905 123 4567" },
    Country { iso: "PT", name: "Португалия", dial: "+351", hint: "912 345 678" },
    Country { iso: "RO", name: "Румыния", dial: "+40", hint: "712 034 568" },
    Country { iso: "RS", name: "Сербия", dial: "+381", hint: "60 1234567" },
    Country { iso: "SA", name: "Саудовская Аравия", dial: "+966", hint: "51 234 5678" },
    Country { iso: "SE", name: "Швеция", dial: "+46", hint: "70-123 45 67" },
    Country { iso: "SG", name: "Сингапур", dial: "+65", hint: "8123 4567" },
    Country { iso: "SI", name: "Словения", dial: "+386", hint: "31 234 567" },
    Country { iso: "SK", name: "Словакия", dial: "+421", hint: "912 123 456" },
    Country { iso: "TH", name: "Таиланд", dial: "+66", hint: "81 234 5678" },
    Country { iso: "TW", name: "Тайвань", dial: "+886", hint: "912 345 678" },
    Country { iso: "VN", name: "Вьетнам", dial: "+84", hint: "91 234 56 78" },
    Country { iso: "ZA", name: "ЮАР", dial: "+27", hint: "71 123 4567" },
];

/// Подбирает индекс страны по введённому коду (`+7`, `380`, `+1` и т.д.).
/// Ищет самое длинное совпадение префикса (например `+380` раньше `+3`).
#[cfg_attr(not(test), allow(dead_code))]
pub fn find_country_by_dial(raw_code: &str) -> Option<usize> {
    let digits: String = raw_code.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let prefixed = format!("+{digits}");
    COUNTRIES
        .iter()
        .enumerate()
        .filter(|(_, c)| prefixed == c.dial)
        .map(|(i, _)| i)
        .next()
        .or_else(|| {
            COUNTRIES
                .iter()
                .enumerate()
                .filter(|(_, c)| prefixed.starts_with(c.dial))
                .max_by(|(i1, c1), (i2, c2)| c1.dial.len().cmp(&c2.dial.len()).then_with(|| i2.cmp(i1)))
                .map(|(i, _)| i)
        })
}

/// Если пользователь вставил полный номер (`+19702876230` или `+380501234567`) в поле номера,
/// автоматически разделяет его на `(индекс_страны, код, локальный_номер)`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn split_full_phone(raw: &str) -> Option<(usize, &'static str, String)> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('+') {
        return None;
    }
    let digits: String = trimmed.chars().filter(char::is_ascii_digit).collect();
    if digits.len() < 4 {
        return None;
    }
    let prefixed = format!("+{digits}");
    let (idx, country) = COUNTRIES
        .iter()
        .enumerate()
        .filter(|(_, c)| prefixed.starts_with(c.dial))
        .max_by(|(i1, c1), (i2, c2)| c1.dial.len().cmp(&c2.dial.len()).then_with(|| i2.cmp(i1)))?;
    let local = prefixed[country.dial.len()..].to_owned();
    Some((idx, country.dial, local))
}

/// Собирает и проверяет международный номер телефона в формате `+19702876230`.
pub fn normalize_phone(dial_code: &str, local_number: &str) -> Result<String, &'static str> {
    let local_trim = local_number.trim();
    let digits: String = if local_trim.starts_with('+') {
        local_trim.chars().filter(char::is_ascii_digit).collect()
    } else {
        let mut d: String = dial_code.chars().filter(char::is_ascii_digit).collect();
        let local_digits: String = local_trim.chars().filter(char::is_ascii_digit).collect();
        if d == "7" && local_digits.len() == 11 && (local_digits.starts_with('8') || local_digits.starts_with('7')) {
            d.push_str(&local_digits[1..]);
        } else {
            d.push_str(&local_digits);
        }
        d
    };
    if digits.len() < 7 {
        return Err("Номер слишком короткий — проверьте код страны и цифры номера");
    }
    if digits.len() > 15 {
        return Err("Номер слишком длинный (максимум 15 цифр по стандарту E.164)");
    }
    Ok(format!("+{digits}"))
}

/// Подбирает индекс страны по введённому полному номеру (или началу номера `+7`, `+380`, `+1`).
pub fn detect_country_from_full(raw: &str) -> Option<usize> {
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let prefixed = format!("+{digits}");
    COUNTRIES
        .iter()
        .enumerate()
        .filter(|(_, c)| prefixed == c.dial)
        .map(|(i, _)| i)
        .next()
        .or_else(|| {
            COUNTRIES
                .iter()
                .enumerate()
                .filter(|(_, c)| prefixed.starts_with(c.dial))
                .max_by(|(i1, c1), (i2, c2)| c1.dial.len().cmp(&c2.dial.len()).then_with(|| i2.cmp(i1)))
                .map(|(i, _)| i)
        })
}

/// Мягкое форматирование поля единого ввода номера (`+7 999 123 45 67` / `+7`), сохраняя удобное стирание.
pub fn format_phone_input(raw: &str) -> String {
    let has_plus = raw.trim_start().starts_with('+');
    let mut digits: String = raw.chars().filter(char::is_ascii_digit).take(15).collect();
    if digits.is_empty() {
        return if raw.contains('+') { "+".to_owned() } else { String::new() };
    }
    if !has_plus && digits.len() == 11 && digits.starts_with('8') {
        digits.replace_range(..1, "7");
    }
    let prefixed = format!("+{digits}");
    let dial_len = COUNTRIES
        .iter()
        .filter(|c| prefixed.starts_with(c.dial))
        .map(|c| c.dial.len())
        .max()
        .unwrap_or(2.min(prefixed.len()));
    let dial = &prefixed[..dial_len.min(prefixed.len())];
    let rest = &prefixed[dial.len()..];
    if rest.is_empty() {
        if raw.ends_with(' ') {
            format!("{dial} ")
        } else {
            dial.to_owned()
        }
    } else {
        format!("{dial} {rest}")
    }
}

/// Заменяет префикс кода страны в строке полного номера при выборе страны из выпадающего списка.
pub fn replace_dial_in_phone(current: &str, old_dial: &str, new_dial: &str) -> String {
    let cur_digits: String = current.chars().filter(char::is_ascii_digit).collect();
    let old_digits: String = old_dial.chars().filter(char::is_ascii_digit).collect();
    let local = cur_digits.strip_prefix(&old_digits).unwrap_or(&cur_digits);
    if local.is_empty() {
        format!("{new_dial} ")
    } else {
        format!("{new_dial} {local}")
    }
}

/// Фильтрация стран по поиску (название, ISO-код или телефонный код).
pub fn filter_countries(query: &str) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return (0..COUNTRIES.len()).collect();
    }
    COUNTRIES
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            c.name.to_lowercase().contains(&q)
                || c.iso.to_lowercase().contains(&q)
                || c.dial.contains(&q)
        })
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn country_detection_and_full_phone_split_work() {
        assert_eq!(COUNTRIES[find_country_by_dial("+7").unwrap()].iso, "RU");
        assert_eq!(COUNTRIES[find_country_by_dial("1").unwrap()].iso, "US");
        assert_eq!(COUNTRIES[find_country_by_dial("+380").unwrap()].iso, "UA");

        let (idx, dial, local) = split_full_phone("+19702876230").unwrap();
        assert_eq!(COUNTRIES[idx].iso, "US");
        assert_eq!(dial, "+1");
        assert_eq!(local, "9702876230");

        let (idx2, dial2, local2) = split_full_phone("+380 50 123-45-67").unwrap();
        assert_eq!(COUNTRIES[idx2].iso, "UA");
        assert_eq!(dial2, "+380");
        assert_eq!(local2, "501234567");
    }

    #[test]
    fn normalize_phone_validates_e164() {
        assert_eq!(normalize_phone("+1", "(970) 287-6230").unwrap(), "+19702876230");
        assert_eq!(normalize_phone("+7", "+19702876230").unwrap(), "+19702876230");
        assert_eq!(normalize_phone("+7", "89991234567").unwrap(), "+79991234567");
        assert_eq!(normalize_phone("+7", "79991234567").unwrap(), "+79991234567");
        assert_eq!(normalize_phone("+7", "7011234567").unwrap(), "+77011234567");
        assert!(normalize_phone("+7", "123").is_err());
        assert!(normalize_phone("+7", "12345678901234567").is_err());
    }

    #[test]
    fn format_phone_handles_ru_and_asian_prefixes() {
        assert_eq!(format_phone_input("89991234567"), "+7 9991234567");
        assert_eq!(format_phone_input("+819012345678"), "+81 9012345678");
        assert_eq!(format_phone_input("+8613800138000"), "+86 13800138000");
    }

    #[test]
    fn detect_country_and_format_full_phone_work() {
        assert_eq!(COUNTRIES[detect_country_from_full("+7 999 111").unwrap()].iso, "RU");
        assert_eq!(COUNTRIES[detect_country_from_full("+380501234567").unwrap()].iso, "UA");
        assert_eq!(format_phone_input("+7"), "+7");
        assert_eq!(format_phone_input("+79991234567"), "+7 9991234567");
        assert_eq!(replace_dial_in_phone("+7 999123", "+7", "+1"), "+1 999123");
    }
}
