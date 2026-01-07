//! Time parsing and formatting utilities for UI.

/// Parses a time string in "HH:MM" or "HH" format to a 0.0-1.0 day fraction.
///
/// Examples:
/// - "00:00" -> 0.0 (midnight)
/// - "12:00" -> 0.5 (noon)
/// - "14:30" -> ~0.604 (2:30 PM)
/// - "24" or "24:00" -> 1.0 (end of day, clamped)
pub fn parse_time(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some((h, m)) = s.split_once(':') {
        let hours: f64 = h.trim().parse().ok()?;
        let minutes: f64 = m.trim().parse().unwrap_or(0.0);
        let total_hours = hours + minutes / 60.0;
        Some((total_hours / 24.0).clamp(0.0, 1.0))
    } else {
        // Just hours, no colon
        let hours: f64 = s.parse().ok()?;
        Some((hours / 24.0).clamp(0.0, 1.0))
    }
}

/// Formats a 0.0-1.0 day fraction as "HH:MM" string.
pub fn format_time(v: f64) -> String {
    let hours = (v * 24.0) % 24.0;
    let h = hours as u32;
    let m = ((hours - h as f64) * 60.0) as u32;
    format!("{:02}:{:02}", h, m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_time_midnight() {
        assert_eq!(parse_time("00:00"), Some(0.0));
        assert_eq!(parse_time("0:00"), Some(0.0));
        assert_eq!(parse_time("0"), Some(0.0));
    }

    #[test]
    fn test_parse_time_noon() {
        assert_eq!(parse_time("12:00"), Some(0.5));
        assert_eq!(parse_time("12"), Some(0.5));
    }

    #[test]
    fn test_parse_time_afternoon() {
        let result = parse_time("14:00").unwrap();
        assert!((result - 14.0 / 24.0).abs() < 0.0001);

        let result = parse_time("14:30").unwrap();
        assert!((result - 14.5 / 24.0).abs() < 0.0001);
    }

    #[test]
    fn test_parse_time_with_whitespace() {
        assert_eq!(parse_time("  12:00  "), Some(0.5));
        assert_eq!(parse_time(" 12 : 00 "), Some(0.5));
    }

    #[test]
    fn test_parse_time_minutes_zero() {
        // This was the reported bug - "00" minutes not working
        let result = parse_time("14:00").unwrap();
        assert!((result - 14.0 / 24.0).abs() < 0.0001);

        let result = parse_time("6:00").unwrap();
        assert!((result - 6.0 / 24.0).abs() < 0.0001);
    }

    #[test]
    fn test_parse_time_clamped() {
        // Values beyond 24 should be clamped to 1.0
        assert_eq!(parse_time("24:00"), Some(1.0));
        assert_eq!(parse_time("30"), Some(1.0));
    }

    #[test]
    fn test_parse_time_invalid() {
        assert_eq!(parse_time(""), None);
        assert_eq!(parse_time("abc"), None);
        assert_eq!(parse_time("12:abc"), Some(0.5)); // Invalid minutes default to 0
    }

    #[test]
    fn test_format_time() {
        assert_eq!(format_time(0.0), "00:00");
        assert_eq!(format_time(0.5), "12:00");
        assert_eq!(format_time(14.0 / 24.0), "14:00");
        assert_eq!(format_time(14.5 / 24.0), "14:30");
    }

    #[test]
    fn test_roundtrip() {
        // Parse then format should give same result
        for time_str in ["00:00", "06:00", "12:00", "14:30", "18:45", "23:59"] {
            let parsed = parse_time(time_str).unwrap();
            let formatted = format_time(parsed);
            assert_eq!(formatted, time_str, "Roundtrip failed for {}", time_str);
        }
    }
}
