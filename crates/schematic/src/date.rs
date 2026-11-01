use std::fmt::Write;

pub const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December",
];

pub const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Week,
    Month,
    Quarter,
    Year,
}

pub fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap(year) => 29,
        _ => 28,
    }
}

pub fn from_civil(year: i64, month: u32, day: u32) -> Option<i32> {
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    i32::try_from(era * 146097 + doe - 719468).ok()
}

pub fn to_civil(days: i32) -> (i64, u32, u32) {
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

pub fn weekday(days: i32) -> u32 {
    ((days as i64 + 3).rem_euclid(7) + 1) as u32
}

pub fn day_of_year(days: i32) -> u32 {
    let (year, _, _) = to_civil(days);
    (days - from_civil(year, 1, 1).unwrap() + 1) as u32
}

pub fn add_months(days: i32, months: i64) -> Option<i32> {
    let (year, month, day) = to_civil(days);
    let total = year * 12 + month as i64 - 1 + months;
    let (year, month) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
    from_civil(year, month, day.min(days_in_month(year, month)))
}

pub fn truncate(days: i32, unit: Unit) -> i32 {
    let (year, month, _) = to_civil(days);
    match unit {
        Unit::Week => days - (weekday(days) as i32 - 1),
        Unit::Month => from_civil(year, month, 1).unwrap(),
        Unit::Quarter => from_civil(year, (month - 1) / 3 * 3 + 1, 1).unwrap(),
        Unit::Year => from_civil(year, 1, 1).unwrap(),
    }
}

pub fn iso(days: i32) -> String {
    let (year, month, day) = to_civil(days);
    format!("{year:04}-{month:02}-{day:02}")
}

pub fn parse_iso(text: &str) -> Option<i32> {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let number = |s: &str| s.bytes().all(|c| c.is_ascii_digit()).then(|| s.parse::<u32>().ok()).flatten();
    from_civil(number(&text[0..4])? as i64, number(&text[5..7])?, number(&text[8..10])?)
}

enum Token {
    Year(usize),
    Month(usize),
    Day(usize),
    Weekday(usize),
    Literal(char),
}

fn tokens(pattern: &str) -> Vec<Token> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if !matches!(c, 'y' | 'M' | 'd' | 'E') {
            out.push(Token::Literal(c));
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i] == c {
            i += 1;
        }
        out.push(match c {
            'y' => Token::Year(i - start),
            'M' => Token::Month(i - start),
            'd' => Token::Day(i - start),
            _ => Token::Weekday(i - start),
        });
    }
    out
}

pub fn format(days: i32, pattern: &str) -> String {
    let (year, month, day) = to_civil(days);
    let mut out = String::new();
    for token in tokens(pattern) {
        match token {
            Token::Year(n) if n <= 2 => {
                let _ = write!(out, "{:02}", year.rem_euclid(100));
            }
            Token::Year(_) => {
                let _ = write!(out, "{year:04}");
            }
            Token::Month(1) => {
                let _ = write!(out, "{month}");
            }
            Token::Month(2) => {
                let _ = write!(out, "{month:02}");
            }
            Token::Month(3) => out.push_str(&MONTHS[month as usize - 1][..3]),
            Token::Month(_) => out.push_str(MONTHS[month as usize - 1]),
            Token::Day(1) => {
                let _ = write!(out, "{day}");
            }
            Token::Day(_) => {
                let _ = write!(out, "{day:02}");
            }
            Token::Weekday(n) if n <= 3 => out.push_str(&WEEKDAYS[weekday(days) as usize - 1][..3]),
            Token::Weekday(_) => out.push_str(WEEKDAYS[weekday(days) as usize - 1]),
            Token::Literal(c) => out.push(c),
        }
    }
    out
}

pub fn parse(text: &str, pattern: &str) -> Option<i32> {
    let mut rest = text;
    let (mut year, mut month, mut day) = (None, None, None);
    let digits = |rest: &mut &str, min: usize, max: usize| -> Option<i64> {
        let n = rest.bytes().take(max).take_while(u8::is_ascii_digit).count();
        if n < min {
            return None;
        }
        let (head, tail) = rest.split_at(n);
        *rest = tail;
        head.parse().ok()
    };
    let name = |rest: &mut &str, names: &[&str], full: bool| -> Option<usize> {
        for (i, candidate) in names.iter().enumerate() {
            let want = if full { *candidate } else { &candidate[..3] };
            if rest.get(..want.len()).is_some_and(|head| head.eq_ignore_ascii_case(want)) {
                *rest = &rest[want.len()..];
                return Some(i + 1);
            }
        }
        None
    };
    for token in tokens(pattern) {
        match token {
            Token::Year(n) if n <= 2 => {
                let yy = digits(&mut rest, 2, 2)?;
                year = Some(if yy < 70 { 2000 + yy } else { 1900 + yy });
            }
            Token::Year(_) => year = Some(digits(&mut rest, 4, 4)?),
            Token::Month(n) if n <= 2 => month = Some(digits(&mut rest, n, 2)? as u32),
            Token::Month(n) => month = Some(name(&mut rest, &MONTHS, n > 3)? as u32),
            Token::Day(n) => day = Some(digits(&mut rest, n.min(2), 2)? as u32),
            Token::Weekday(n) => {
                name(&mut rest, &WEEKDAYS, n > 3)?;
            }
            Token::Literal(c) => rest = rest.strip_prefix(c)?,
        }
    }
    if !rest.is_empty() {
        return None;
    }
    from_civil(year?, month?, day?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn civil(y: i64, m: u32, d: u32) -> i32 {
        from_civil(y, m, d).unwrap()
    }

    #[test]
    fn civil_dates_round_trip_through_day_numbers() {
        assert_eq!(from_civil(1970, 1, 1), Some(0));
        assert_eq!(to_civil(0), (1970, 1, 1));
        for (y, m, d) in [(2000, 3, 1), (1600, 2, 29), (2024, 12, 31), (1, 1, 1), (-1, 12, 31), (9999, 6, 15)] {
            assert_eq!(to_civil(civil(y, m, d)), (y, m, d));
        }
        assert_eq!(civil(2024, 3, 15) - civil(2024, 2, 29), 15);
        assert_eq!(from_civil(2023, 2, 29), None);
        assert_eq!(from_civil(2024, 13, 1), None);
        assert_eq!(from_civil(2024, 4, 0), None);
    }

    #[test]
    fn weekdays_leap_years_and_days_of_year() {
        assert_eq!(weekday(0), 4);
        assert_eq!(weekday(civil(2024, 3, 15)), 5);
        assert_eq!(weekday(civil(2024, 3, 17)), 7);
        assert_eq!(weekday(civil(2024, 3, 18)), 1);
        assert_eq!(day_of_year(civil(2024, 12, 31)), 366);
        assert_eq!(day_of_year(civil(2023, 1, 1)), 1);
        assert_eq!([is_leap(1900), is_leap(2000), is_leap(2024), is_leap(2023)], [false, true, true, false]);
        assert_eq!([days_in_month(2024, 2), days_in_month(2023, 2), days_in_month(2024, 4)], [29, 28, 30]);
    }

    #[test]
    fn months_add_with_clamping_and_dates_truncate() {
        assert_eq!(add_months(civil(2024, 1, 31), 1), Some(civil(2024, 2, 29)));
        assert_eq!(add_months(civil(2024, 1, 31), -2), Some(civil(2023, 11, 30)));
        assert_eq!(add_months(civil(2024, 2, 29), 12), Some(civil(2025, 2, 28)));
        assert_eq!(add_months(civil(2024, 11, 15), 2), Some(civil(2025, 1, 15)));
        let d = civil(2024, 3, 15);
        assert_eq!(truncate(d, Unit::Week), civil(2024, 3, 11));
        assert_eq!(truncate(civil(2024, 3, 11), Unit::Week), civil(2024, 3, 11));
        assert_eq!(truncate(d, Unit::Month), civil(2024, 3, 1));
        assert_eq!(truncate(civil(2024, 8, 9), Unit::Quarter), civil(2024, 7, 1));
        assert_eq!(truncate(d, Unit::Year), civil(2024, 1, 1));
    }

    #[test]
    fn patterns_format_and_parse() {
        let d = civil(2024, 3, 5);
        assert_eq!(iso(d), "2024-03-05");
        assert_eq!(format(d, "yyyy-MM-dd"), "2024-03-05");
        assert_eq!(format(d, "d/M/yy EEE"), "5/3/24 Tue");
        assert_eq!(format(d, "EEEE, MMMM d"), "Tuesday, March 5");
        assert_eq!(format(civil(7, 1, 1), "yyyy"), "0007");
        assert_eq!(parse("05.03.2024", "dd.MM.yyyy"), Some(d));
        assert_eq!(parse("5.3.2024", "d.M.yyyy"), Some(d));
        assert_eq!(parse("5.3.2024", "dd.MM.yyyy"), None);
        assert_eq!(parse("Tuesday 5 March 2024", "EEEE d MMMM yyyy"), Some(d));
        assert_eq!(parse("5 MAR 24", "d MMM yy"), Some(d));
        assert_eq!(parse("5 Mar 75", "d MMM yy"), Some(civil(1975, 3, 5)));
        assert_eq!(parse("2024-03-05 ", "yyyy-MM-dd"), None);
        assert_eq!(parse("2024/03/05", "yyyy-MM-dd"), None);
        assert_eq!(parse("2024-02-30", "yyyy-MM-dd"), None);
        assert_eq!(parse_iso("2024-03-05"), Some(d));
        assert_eq!(parse_iso("2024-3-05"), None);
        assert_eq!(parse_iso("2024-03-05T00"), None);
    }
}
