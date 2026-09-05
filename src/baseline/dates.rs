//! Just enough calendar arithmetic to print a date and read one back.
//! No time zone: everything is UTC, dates are days.

pub const DAY: i64 = 86_400;

/// Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Days since 1970-01-01 for a civil date.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn iso_date(unix: i64) -> String {
    let (y, m, d) = civil_from_days(unix.div_euclid(DAY));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Parse `YYYY-MM-DD` to the unix time of that day's start.
pub fn parse_iso_date(text: &str) -> Option<i64> {
    let mut parts = text.trim().split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let days = days_from_civil(y, m, d);
    // Round-trip guards impossible dates such as 2026-02-30.
    if civil_from_days(days) != (y, m, d) {
        return None;
    }
    Some(days * DAY)
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_known_dates() {
        assert_eq!(iso_date(0), "1970-01-01");
        assert_eq!(
            iso_date(parse_iso_date("2026-09-05").unwrap()),
            "2026-09-05"
        );
        assert_eq!(
            iso_date(parse_iso_date("2000-02-29").unwrap()),
            "2000-02-29"
        );
        assert_eq!(parse_iso_date("2026-02-30"), None);
        assert_eq!(parse_iso_date("2026-13-01"), None);
        assert_eq!(parse_iso_date("yesterday"), None);
    }
}
