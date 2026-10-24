//! Prüfung des Ergebnisses gegen den Plan.

use crate::model::MediaInfo;
use crate::plan::FilePlan;

/// Maximale Abweichung der Dauer (1 s).
pub const MAX_DURATION_DIFF_NS: u64 = 1_000_000_000;

/// Vergleicht eine neu eingelesene Datei mit dem Plan.
///
/// Geprüft werden Spuranzahl und -reihenfolge, Typen, Sprachen, Namen, Standard/Forced, Dauer
/// (< 1 s Abweichung), Größe > 0 sowie Anzahl der Anhänge und Kapitel. Liefert alle Abweichungen.
pub fn verify(plan: &FilePlan, result: &MediaInfo) -> Result<(), Vec<String>> {
    let mut problems = Vec::new();
    if result.size == 0 {
        problems.push("Zieldatei ist leer".to_string());
    }
    let expected: Vec<_> = plan.kept().collect();
    if expected.len() != result.tracks.len() {
        problems.push(format!(
            "{} Spuren erwartet, {} gefunden",
            expected.len(),
            result.tracks.len()
        ));
    } else {
        for (i, (exp, got)) in expected.iter().zip(&result.tracks).enumerate() {
            let label = format!("Spur {} ({} {})", i, exp.kind, exp.type_index);
            if exp.kind != got.kind {
                problems.push(format!("{label}: Typ {} statt {}", got.kind, exp.kind));
                continue;
            }
            if got.lang() != exp.after.lang {
                problems.push(format!(
                    "{label}: Sprache {} statt {}",
                    got.lang(),
                    exp.after.lang
                ));
            }
            let got_name = got.name.as_deref().filter(|n| !n.is_empty());
            if got_name != exp.after.name.as_deref() {
                problems.push(format!(
                    "{label}: Name „{}“ statt „{}“",
                    got_name.unwrap_or(""),
                    exp.after.name.as_deref().unwrap_or("")
                ));
            }
            if got.flags.default != exp.after.default {
                problems.push(format!("{label}: Standard-Flag {}", got.flags.default));
            }
            if got.flags.forced != exp.after.forced {
                problems.push(format!("{label}: Forced-Flag {}", got.flags.forced));
            }
        }
    }
    if let (Some(a), Some(b)) = (plan.duration_ns, result.duration_ns)
        && a.abs_diff(b) >= MAX_DURATION_DIFF_NS
    {
        problems.push(format!(
            "Dauer weicht ab: {:.3} s statt {:.3} s",
            b as f64 / 1e9,
            a as f64 / 1e9
        ));
    }
    if result.attachments != plan.attachments {
        problems.push(format!(
            "{} Anhänge statt {}",
            result.attachments, plan.attachments
        ));
    }
    if result.chapters != plan.chapters {
        problems.push(format!(
            "{} Kapitel statt {}",
            result.chapters, plan.chapters
        ));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{PlanOptions, plan_file};
    use crate::probe::parse_identification;
    use crate::rules::RuleSet;
    use std::path::Path;

    fn media() -> MediaInfo {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/json/full.json");
        let json = std::fs::read_to_string(p).expect("fixture");
        let mut m = parse_identification(&json, Path::new("/x.mkv")).expect("parse");
        m.size = 10;
        m
    }

    #[test]
    fn identical_file_passes() {
        let m = media();
        let rules: RuleSet = toml::from_str("[audio]\ndefault = \"Nr. 1\"").expect("toml");
        let plan = plan_file(&m, &rules, None, &PlanOptions::default()).expect("plan");
        assert_eq!(verify(&plan, &m), Ok(()));
    }

    #[test]
    fn detects_deviations() {
        let m = media();
        let rules: RuleSet =
            toml::from_str("[audio]\nkeep = \"Nr. 1\"\ndefault = \"keine\"").expect("toml");
        let plan = plan_file(&m, &rules, None, &PlanOptions::default()).expect("plan");
        let errs = verify(&plan, &m).expect_err("Spuranzahl");
        assert!(errs[0].contains("Spuren erwartet"), "{errs:?}");

        let rules: RuleSet = toml::from_str("[audio]\ndefault = \"Nr. 2\"").expect("toml");
        let plan = plan_file(&m, &rules, None, &PlanOptions::default()).expect("plan");
        let mut bad = m.clone();
        bad.duration_ns = Some(9_000_000_000);
        bad.size = 0;
        bad.attachments = 0;
        let errs = verify(&plan, &bad).expect_err("abweichend");
        let all = errs.join("\n");
        for needle in ["leer", "Standard-Flag", "Dauer", "Anhänge"] {
            assert!(all.contains(needle), "{needle}: {all}");
        }
    }
}
