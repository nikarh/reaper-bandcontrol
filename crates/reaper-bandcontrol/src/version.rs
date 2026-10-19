//! The supported REAPER versions, from `[workspace.metadata.reaper]` in the workspace's Cargo.toml (which CI, the
//! scripts and the docs read as well).

use std::cmp::Ordering;

const WORKSPACE: &str = include_str!("../../../Cargo.toml");

/// The oldest supported REAPER, e.g. `5.93`.
#[must_use]
pub fn min_reaper() -> &'static str {
    metadata("min").unwrap_or("0.0")
}

fn metadata(key: &str) -> Option<&'static str> {
    let section = WORKSPACE.split("[workspace.metadata.reaper]").nth(1)?;
    section
        .lines()
        .take_while(|l| !l.starts_with('['))
        .find_map(|l| l.split_once('=').filter(|(k, _)| k.trim() == key))
        .map(|(_, v)| v.trim().trim_matches('"'))
}

/// `major.minor` of a REAPER version such as `7.82/linux-x86_64`, `6.0` or `7.10+dev0923`. REAPER versions are
/// decimals: 7.1 would be 7.10, so the minor is compared as two digits.
fn parse(version: &str) -> Option<(u32, u32)> {
    let number: String = version
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let (major, minor) = number.split_once('.').unwrap_or((&number, "0"));
    let minor = format!("{:0<2}", minor.get(..2).unwrap_or(minor));
    Some((major.parse().ok()?, minor.parse().ok()?))
}

/// Whether REAPER `version` is older than the oldest supported one; `None` when it can't be told.
#[must_use]
pub fn older_than_supported(version: &str) -> Option<bool> {
    Some(parse(version)?.cmp(&parse(min_reaper())?) == Ordering::Less)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn reads_the_workspace_metadata() {
        assert!(parse(min_reaper()).is_some(), "{}", min_reaper());
        assert!(parse(metadata("tested").unwrap_or_default()) >= parse(min_reaper()));
    }

    #[test]
    fn parses_reaper_versions() {
        assert_eq!(parse("7.82/linux-x86_64"), Some((7, 82)));
        assert_eq!(parse("6.0/x64"), Some((6, 0)));
        assert_eq!(parse("7.10+dev0923/macOS-arm64"), Some((7, 10)));
        assert_eq!(parse("7.1"), Some((7, 10)));
        assert_eq!(parse("5.93"), Some((5, 93)));
        assert_eq!(parse("weird"), None);
    }

    #[test]
    fn compares_like_reaper() {
        assert!(parse("5.93") < parse("5.95"));
        assert!(parse("5.99") < parse("6.0"));
        assert!(parse("6.0") < parse("6.01"));
        assert!(parse("7.09") < parse("7.1"));
        assert_eq!(older_than_supported("5.80/linux-x86_64"), Some(true));
        assert_eq!(older_than_supported(&format!("{}/x64", min_reaper())), Some(false));
        assert_eq!(older_than_supported("8.0/x64"), Some(false));
    }
}
