use anyhow::{bail, Result};
use regex::Regex;

/// Parse a duration string like "2h30m10s" into seconds.
pub fn parse_duration(s: &str) -> Result<u64> {
    let s = s.trim();
    if s.is_empty() {
        bail!("empty duration string");
    }

    let mut total_seconds: u64 = 0;
    let mut remaining = s.to_string();

    let re_hours = Regex::new(r"^(\d+)[hH]").unwrap();
    if let Some(caps) = re_hours.captures(&remaining) {
        let hours: u64 = caps[1].parse()?;
        total_seconds += hours * 3600;
        remaining = remaining[caps[0].len()..].to_string();
    }

    let re_minutes = Regex::new(r"^(\d+)[mM]").unwrap();
    if let Some(caps) = re_minutes.captures(&remaining) {
        let minutes: u64 = caps[1].parse()?;
        total_seconds += minutes * 60;
        remaining = remaining[caps[0].len()..].to_string();
    }

    let re_seconds = Regex::new(r"^(\d+)[sS]").unwrap();
    if let Some(caps) = re_seconds.captures(&remaining) {
        let seconds: u64 = caps[1].parse()?;
        total_seconds += seconds;
        remaining = remaining[caps[0].len()..].to_string();
    }

    if !remaining.is_empty() {
        bail!("invalid duration format: unexpected '{remaining}' in '{s}'");
    }

    if total_seconds == 0 {
        bail!("duration must be greater than 0");
    }

    Ok(total_seconds)
}

/// Format seconds into a human-readable duration string like "2h30m10s".
pub fn format_duration(seconds: u64) -> String {
    if seconds == 0 {
        return "0s".to_string();
    }

    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    let mut result = String::new();
    if hours > 0 {
        result.push_str(&format!("{hours}h"));
    }
    if minutes > 0 {
        result.push_str(&format!("{minutes}m"));
    }
    if secs > 0 || result.is_empty() {
        result.push_str(&format!("{secs}s"));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("2h").unwrap(), 7200);
        assert_eq!(parse_duration("30m").unwrap(), 1800);
        assert_eq!(parse_duration("90s").unwrap(), 90);
        assert_eq!(parse_duration("1h30m").unwrap(), 5400);
        assert_eq!(parse_duration("2h30m10s").unwrap(), 9010);
        assert_eq!(parse_duration("1H30M").unwrap(), 5400);
    }

    #[test]
    fn test_parse_duration_errors() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("0h0m0s").is_err());
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(60), "1m");
        assert_eq!(format_duration(90), "1m30s");
        assert_eq!(format_duration(3600), "1h");
        assert_eq!(format_duration(3661), "1h1m1s");
        assert_eq!(format_duration(7200), "2h");
    }
}
