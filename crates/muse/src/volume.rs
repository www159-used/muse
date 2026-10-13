// SPDX-License-Identifier: AGPL-3.0-only

/// Hand-parsed volume command. Tokens are `set`, `inc`, and `dec` only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeCommand {
    Query,
    Set(u8),
    Inc(u32),
    Dec(u32),
}

const USAGE: &str = "usage: volume | volume set <0-100> | volume inc <steps> | volume dec <steps>";

pub fn parse(args: &[&str]) -> Result<VolumeCommand, String> {
    match args {
        [] => Ok(VolumeCommand::Query),
        ["set", raw] => parse_set(raw),
        ["inc", raw] => Ok(VolumeCommand::Inc(parse_step("inc", raw)?)),
        ["dec", raw] => Ok(VolumeCommand::Dec(parse_step("dec", raw)?)),
        ["set"] => Err("volume set expects an integer from 0 to 100".into()),
        ["inc"] => Err("volume inc expects an integer of 0 or more, for example 5".into()),
        ["dec"] => Err("volume dec expects an integer of 0 or more, for example 5".into()),
        _ => Err(USAGE.into()),
    }
}

fn parse_set(raw: &str) -> Result<VolumeCommand, String> {
    match classify(raw) {
        Number::Value(value) if value <= 100 => Ok(VolumeCommand::Set(value as u8)),
        Number::Value(_) | Number::Huge => Err(format!(
            "volume set {raw} is out of range; use an integer from 0 to 100"
        )),
        Number::Bad => Err(format!(
            "volume set expects an integer from 0 to 100; got {raw}"
        )),
    }
}

fn parse_step(operation: &str, raw: &str) -> Result<u32, String> {
    match classify(raw) {
        Number::Value(value) => Ok(value),
        Number::Huge | Number::Bad => Err(format!(
            "volume {operation} expects an integer of 0 or more, for example 5; got {raw}"
        )),
    }
}

enum Number {
    Value(u32),
    Huge,
    Bad,
}

fn classify(raw: &str) -> Number {
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Number::Bad;
    }
    raw.parse().map(Number::Value).unwrap_or(Number::Huge)
}

/// Status and `:volume` share this text. `Volume` is the UI scale; `Gain` is
/// the square-law amplitude from [`muse_playback::perceptual_gain`].
pub fn format_observation(volume: u8) -> String {
    format!(
        "Volume: {volume}%\nGain: {:.4}",
        muse_playback::perceptual_gain(volume)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_rejects_out_of_range_and_inc_rejects_bad_format() {
        let range = parse(&["set", "999"]).unwrap_err();
        assert!(range.contains("out of range"), "{range}");
        assert!(range.contains('0') && range.contains("100"), "{range}");
        let format = parse(&["inc", "abc"]).unwrap_err();
        assert!(format.contains("abc"), "{format}");
        assert!(format.contains("integer"), "{format}");
        assert!(parse(&["set", "+5"]).unwrap_err().contains("0 to 100"));
        assert!(parse(&["dec", "-5"]).unwrap_err().contains("integer"));
        assert_eq!(parse(&["set", "50"]).unwrap(), VolumeCommand::Set(50));
        assert_eq!(parse(&["inc", "5"]).unwrap(), VolumeCommand::Inc(5));
        assert_eq!(parse(&["dec", "5"]).unwrap(), VolumeCommand::Dec(5));
        assert_eq!(parse(&[]).unwrap(), VolumeCommand::Query);
    }

    #[test]
    fn observation_reports_ui_percent_and_square_law_gain() {
        assert_eq!(format_observation(50), "Volume: 50%\nGain: 0.2500");
        assert_eq!(format_observation(0), "Volume: 0%\nGain: 0.0000");
        assert_eq!(format_observation(100), "Volume: 100%\nGain: 1.0000");
    }
}
