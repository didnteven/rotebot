//! "Send only when…" for scheduled results: decides from a run's output text whether the
//! email and notification go out. Pure and deterministic, so the same inputs always give the
//! same answer. Stored with the approved schedule outputs, so changing it needs approval again.
use crate::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// The number sits right after the text ("Tide 63%" with text "Tide").
    After,
    /// The number sits right before the text ("-43.7%" with text "%").
    Before,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[derive(Default)]
pub enum SendWhen {
    #[default]
    Always,
    /// The result differs from the previous run's.
    Changed,
    Contains {
        text: String,
    },
    /// A number read from the result is above `value`. Empty `near` means the first number
    /// in the first line.
    Above {
        near: String,
        side: Side,
        value: f64,
    },
    Below {
        near: String,
        side: Side,
        value: f64,
    },
}
// `value` is checked to be finite when a schedule is saved, so equality is reflexive.
impl Eq for SendWhen {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub send: bool,
    /// Plain words for the Today entry; `None` when every run sends and nothing needs saying.
    pub note: Option<String>,
}

impl SendWhen {
    pub fn is_always(&self) -> bool {
        matches!(self, Self::Always)
    }
    pub fn validate(&self) -> CoreResult<()> {
        let text_ok =
            |s: &str, max: usize| s.chars().count() <= max && !s.chars().any(char::is_control);
        let ok = match self {
            Self::Always | Self::Changed => true,
            Self::Contains { text } => !text.trim().is_empty() && text_ok(text, 200),
            Self::Above { near, value, .. } | Self::Below { near, value, .. } => {
                value.is_finite() && text_ok(near, 100)
            }
        };
        if ok {
            Ok(())
        } else {
            Err(CoreError::Invalid(
                "Check the “send only when” choice: enter the text and a number".into(),
            ))
        }
    }
    /// `previous` is the last finished run's output for this schedule, if it is still kept.
    pub fn decide(&self, output: &str, previous: Option<&str>) -> Decision {
        let send = |note: Option<String>| Decision { send: true, note };
        let hold = |note: String| Decision {
            send: false,
            note: Some(note),
        };
        match self {
            Self::Always => send(None),
            Self::Changed => match previous {
                // Nothing to compare with yet: send, so the person sees it working.
                None => send(Some(
                    "First run, so there was nothing to compare with".into(),
                )),
                Some(p) if squash(p) == squash(output) => {
                    hold("The result didn’t change, so nothing was sent".into())
                }
                Some(_) => send(Some("The result changed".into())),
            },
            Self::Contains { text } => {
                if output.to_lowercase().contains(&text.trim().to_lowercase()) {
                    send(Some(format!("The result mentions “{}”", text.trim())))
                } else {
                    hold(format!(
                        "The result doesn’t mention “{}”, so nothing was sent",
                        text.trim()
                    ))
                }
            }
            Self::Above { near, side, value } | Self::Below { near, side, value } => {
                let above = matches!(self, Self::Above { .. });
                let word = if above { "above" } else { "below" };
                match read_number(output, near, *side) {
                    // A missed alert is worse than an extra message: when the number can't be
                    // found (the wording changed), send anyway and say why.
                    None => send(Some(if near.trim().is_empty() {
                        "Couldn’t find a number in the result, so it was sent anyway".into()
                    } else {
                        format!(
                            "Couldn’t find a number {} “{}”, so it was sent anyway",
                            if *side == Side::After {
                                "after"
                            } else {
                                "before"
                            },
                            near.trim()
                        )
                    })),
                    Some(found) => {
                        let hit = if above {
                            found > *value
                        } else {
                            found < *value
                        };
                        let label = if near.trim().is_empty() {
                            "The number".to_string()
                        } else {
                            format!("“{}”", near.trim())
                        };
                        if hit {
                            send(Some(format!(
                                "{label} was {}, {word} {}",
                                show(found),
                                show(*value)
                            )))
                        } else {
                            hold(format!(
                                "{label} was {}, not {word} {}, so nothing was sent",
                                show(found),
                                show(*value)
                            ))
                        }
                    }
                }
            }
        }
    }
}

fn show(n: f64) -> String {
    let text = format!("{n:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Numbers in `text` with their start and end byte offsets. Skips dates (2026-10-30), times
/// (06:00:40.350) and digits glued to letters (the T in 2026-10-30T06), and understands
/// -12.4, 0.9001 and 77,096.24.
fn numbers(text: &str) -> Vec<(usize, usize, f64)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i;
        while j < b.len()
            && (b[j].is_ascii_digit()
                || (matches!(b[j], b',' | b'.') && b.get(j + 1).is_some_and(u8::is_ascii_digit)))
        {
            j += 1;
        }
        let prev = start.checked_sub(1).map(|p| b[p]);
        let before_prev = start.checked_sub(2).map(|p| b[p]);
        let next = b.get(j).copied();
        let digit_after = |k: usize| b.get(k).is_some_and(u8::is_ascii_digit);
        let part_of_date_or_time = (matches!(next, Some(b'-' | b'/' | b':')) && digit_after(j + 1))
            || (matches!(prev, Some(b'-' | b'/' | b':'))
                && before_prev.is_some_and(|c| c.is_ascii_digit()))
            || prev.is_some_and(|c| c.is_ascii_alphabetic());
        if !part_of_date_or_time {
            if let Ok(mut v) = text[start..j].replace(',', "").parse::<f64>() {
                let negative =
                    prev == Some(b'-') && before_prev.is_none_or(|c| !c.is_ascii_alphanumeric());
                if negative {
                    v = -v;
                }
                // A leading minus belongs to the number, so it starts one byte earlier.
                out.push((if negative { start - 1 } else { start }, j, v));
            }
        }
        i = j.max(i + 1);
    }
    out
}

/// The number the person pointed at. With text, the number sitting right next to it (a
/// space, colon, equals sign, dollar sign or bracket may come between), tried at each place
/// the text occurs: "Tide 63%" works although "tide" also appears earlier in the sentence.
/// Without text, the first number in the first line.
fn read_number(output: &str, near: &str, side: Side) -> Option<f64> {
    let near = near.trim();
    if near.is_empty() {
        return numbers(output.lines().next().unwrap_or(""))
            .first()
            .map(|n| n.2);
    }
    let all = numbers(output);
    let (hay, needle) = (output.to_lowercase(), near.to_lowercase());
    // Lowercasing can change byte lengths for unusual characters; then offsets don't line up.
    if hay.len() != output.len() {
        return None;
    }
    let filler = [' ', ':', '=', '$', '(', '\u{a0}'];
    let mut from = 0;
    while let Some(at) = hay[from..].find(&needle) {
        let (begin, end) = (from + at, from + at + needle.len());
        let found = match side {
            Side::After => {
                let gap = output[end..].len() - output[end..].trim_start_matches(filler).len();
                (gap <= 3)
                    .then(|| all.iter().find(|n| n.0 == end + gap).map(|n| n.2))
                    .flatten()
            }
            Side::Before => {
                let gap = output[..begin].len() - output[..begin].trim_end_matches(filler).len();
                (gap <= 3)
                    .then(|| all.iter().find(|n| n.1 == begin - gap).map(|n| n.2))
                    .flatten()
            }
        };
        if found.is_some() {
            return found;
        }
        from = end;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    // Real outputs, produced by running programs in the sandbox on live API responses.
    const TIDE: &str =
        "Mission Bay tide right now (2026-10-31T18:15): -0.98 m, Tide 9% (full being 100%)\n";
    const WEATHER: &str = "Auckland today (2026-10-31): high 17.6C, low 11.6C\n";
    const LOSER: &str =
        "Biggest loser today: LVWR (LiveWire Group Inc. Common Stock) -43.744% at $0.9001\n";
    const BTC: &str = "Bitcoin: $77,096.24\n";
    const QUAKE: &str =
        "Latest NZ quake: magnitude 3.2, 20 km west of Porangahau (2026-10-30T06:00:40.350Z)\n";
    const FX: &str = "1 EUR = 1.98 NZD (as of 2026-10-30)\n";
    const AIR: &str = "Auckland air quality: US AQI 66, PM2.5 14.7, PM10 24.3\n";
    const NEWS: &str = "Top BBC story: US approves sale of 500 Patriot missiles to Ukraine\n";
    const ISS: &str = "ISS is at latitude -51.7, longitude 132.8, altitude 435 km\n";
    fn above(near: &str, side: Side, value: f64) -> SendWhen {
        SendWhen::Above {
            near: near.into(),
            side,
            value,
        }
    }
    fn below(near: &str, side: Side, value: f64) -> SendWhen {
        SendWhen::Below {
            near: near.into(),
            side,
            value,
        }
    }
    fn read(out: &str, near: &str, side: Side) -> Option<f64> {
        read_number(out, near, side)
    }

    #[test]
    fn dates_times_and_timestamps_are_never_numbers() {
        let n = |t: &str| numbers(t).iter().map(|x| x.2).collect::<Vec<_>>();
        assert_eq!(n(QUAKE), vec![3.2, 20.0]);
        assert_eq!(n(TIDE), vec![-0.98, 9.0, 100.0]);
        assert_eq!(n(FX), vec![1.0, 1.98]);
        assert_eq!(n("Updated 17:05 on 31/10/2026: 7"), vec![7.0]);
    }
    #[test]
    fn the_first_number_is_often_the_wrong_one_so_the_text_next_to_it_decides() {
        assert_eq!(read(FX, "", Side::After), Some(1.0)); // not the rate
        assert_eq!(read(FX, "=", Side::After), Some(1.98));
        assert_eq!(read(TIDE, "", Side::After), Some(-0.98)); // not the 9%
        assert_eq!(read(TIDE, "Tide", Side::After), Some(9.0)); // "tide" appears twice
        assert_eq!(read(NEWS, "", Side::After), Some(500.0));
        assert_eq!(read(NEWS, "story", Side::After), None); // no number right after it
    }
    #[test]
    fn a_number_can_be_found_by_the_unit_after_it() {
        assert_eq!(read(LOSER, "%", Side::Before), Some(-43.744));
        assert_eq!(read(TIDE, "%", Side::Before), Some(9.0));
        assert_eq!(read(TIDE, " m", Side::Before), Some(-0.98));
        assert_eq!(read(ISS, "km", Side::Before), Some(435.0));
        assert_eq!(read(WEATHER, "C", Side::Before), Some(17.6));
        assert_eq!(read(QUAKE, "km", Side::Before), Some(20.0));
    }
    #[test]
    fn a_name_with_digits_does_not_move_the_number() {
        let a = "Biggest loser today: MMM (3M Company) -4.2% at $98.10\n";
        assert_eq!(read(a, "%", Side::Before), Some(-4.2));
        assert_eq!(read(a, "", Side::After), Some(3.0)); // why the unit is the sturdier choice
    }
    #[test]
    fn real_alerts_fire_and_hold_with_a_plain_reason() {
        let d = above("Tide", Side::After, 80.0).decide(TIDE, None);
        assert!(!d.send);
        assert_eq!(
            d.note.as_deref(),
            Some("“Tide” was 9, not above 80, so nothing was sent")
        );
        let d = below("Tide", Side::After, 15.0).decide(TIDE, None);
        assert!(d.send);
        assert_eq!(d.note.as_deref(), Some("“Tide” was 9, below 15"));
        assert!(below("%", Side::Before, -30.0).decide(LOSER, None).send);
        assert!(!below("%", Side::Before, -50.0).decide(LOSER, None).send);
        assert!(!above("$", Side::After, 80_000.0).decide(BTC, None).send);
        assert!(above("$", Side::After, 70_000.0).decide(BTC, None).send);
        assert!(
            above("magnitude", Side::After, 3.0)
                .decide(QUAKE, None)
                .send
        );
        assert!(
            !above("magnitude", Side::After, 5.0)
                .decide(QUAKE, None)
                .send
        );
        assert!(!above("AQI", Side::After, 100.0).decide(AIR, None).send);
        assert!(above("PM2.5", Side::After, 10.0).decide(AIR, None).send);
        assert!(below("latitude", Side::After, -50.0).decide(ISS, None).send);
        assert!(above("high", Side::After, 17.0).decide(WEATHER, None).send);
        assert!(!above("", Side::After, 1.0).decide(FX, None).send); // first number is 1
        assert!(above("=", Side::After, 1.5).decide(FX, None).send);
    }
    #[test]
    fn a_missing_number_sends_anyway_and_says_so() {
        let d = above("Tide", Side::After, 80.0).decide("Tide data is not available", None);
        assert!(d.send);
        assert_eq!(
            d.note.as_deref(),
            Some("Couldn’t find a number after “Tide”, so it was sent anyway")
        );
        let d = below("%", Side::Before, 0.0).decide("No change today", None);
        assert!(d.send);
        assert_eq!(
            d.note.as_deref(),
            Some("Couldn’t find a number before “%”, so it was sent anyway")
        );
    }
    #[test]
    fn changed_and_contains() {
        assert!(SendWhen::Changed.decide(NEWS, None).send);
        let same = NEWS.replace("US approves", "US  approves");
        assert!(!SendWhen::Changed.decide(&same, Some(NEWS)).send);
        assert!(
            SendWhen::Changed
                .decide("Top BBC story: Something else", Some(NEWS))
                .send
        );
        let c = SendWhen::Contains {
            text: "ukraine".into(),
        };
        assert!(c.decide(NEWS, None).send);
        let d = SendWhen::Contains {
            text: "election".into(),
        }
        .decide(NEWS, None);
        assert!(!d.send);
        assert_eq!(
            d.note.as_deref(),
            Some("The result doesn’t mention “election”, so nothing was sent")
        );
        assert!(SendWhen::Always.decide(NEWS, None).note.is_none());
    }
    #[test]
    fn unusual_text_cannot_misalign_offsets_or_panic() {
        // 'İ' lowercases to a longer string, which would shift byte offsets.
        assert_eq!(read("İ Tide 5%", "tide", Side::After), None);
        assert_eq!(read("", "x", Side::Before), None);
        assert_eq!(read("5", "5", Side::Before), None);
        assert_eq!(read("-", "", Side::After), None);
        let long = "9".repeat(5000);
        assert!(numbers(&long).len() <= 1);
    }
    #[test]
    fn only_sensible_conditions_can_be_saved() {
        assert!(SendWhen::Contains { text: "  ".into() }.validate().is_err());
        assert!(above("Tide", Side::After, f64::NAN).validate().is_err());
        assert!(above("Tide", Side::After, f64::INFINITY)
            .validate()
            .is_err());
        assert!(above("Ti\nde", Side::After, 1.0).validate().is_err());
        assert!(SendWhen::Contains {
            text: "x".repeat(201)
        }
        .validate()
        .is_err());
        assert!(above("Tide", Side::After, 80.0).validate().is_ok());
        assert!(SendWhen::Changed.validate().is_ok());
    }
}
