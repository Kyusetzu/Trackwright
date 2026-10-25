//! Prüfung des Ergebnisses gegen den Plan.

use crate::i18n::t;
use crate::model::MediaInfo;
use crate::plan::FilePlan;
use crate::tr;

/// Maximale Abweichung der Dauer ohne entfernte Spuren und maximale Verlängerung (1 s).
pub const MAX_DURATION_DIFF_NS: u64 = 1_000_000_000;

/// Mindest-Toleranz für eine Verkürzung, wenn Spuren entfernt wurden (5 s).
pub const MIN_SHORTENING_NS: u64 = 5_000_000_000;

/// Wie viel kürzer das Ergebnis höchstens sein darf.
///
/// Die Containerdauer ist das Ende der *längsten* Spur. Werden Spuren entfernt, wird die Datei
/// legitim kürzer, wenn eine entfernte Spur länger lief als der Rest (typisch: eine Audio- oder
/// Untertitelspur endet ein paar Sekunden nach dem Video). Dann sind bis zu max(5 s, 1 % der
/// Dauer) erlaubt. Ohne entfernte Spuren bleibt es bei 1 s.
pub fn allowed_shortening_ns(plan: &FilePlan) -> u64 {
    if plan.removed_count() == 0 {
        return MAX_DURATION_DIFF_NS;
    }
    let one_percent = plan.duration_ns.unwrap_or(0) / 100;
    MIN_SHORTENING_NS.max(one_percent)
}

/// Vergleicht eine neu eingelesene Datei mit dem Plan.
///
/// Geprüft werden Spuranzahl und -reihenfolge, Typen, Sprachen, Namen, Standard/Forced, Dauer
/// (siehe [`allowed_shortening_ns`]), Größe > 0 sowie Anzahl der Anhänge und Kapitel. Liefert alle
/// Abweichungen.
pub fn verify(plan: &FilePlan, result: &MediaInfo) -> Result<(), Vec<String>> {
    let mut problems = Vec::new();
    if result.size == 0 {
        problems.push(t("target file is empty", "Zieldatei ist leer").to_string());
    }
    let expected: Vec<_> = plan.kept().collect();
    if expected.len() != result.tracks.len() {
        let (e, g) = (expected.len(), result.tracks.len());
        problems.push(tr!(
            "expected {e} tracks, found {g}",
            "{e} Spuren erwartet, {g} gefunden"
        ));
    } else {
        for (i, (exp, got)) in expected.iter().zip(&result.tracks).enumerate() {
            let label = tr!(
                "Track {i} ({} {})",
                "Spur {i} ({} {})",
                exp.kind,
                exp.type_index
            );
            if exp.kind != got.kind {
                let (g, x) = (got.kind, exp.kind);
                problems.push(tr!(
                    "{label}: type {g} instead of {x}",
                    "{label}: Typ {g} statt {x}"
                ));
                continue;
            }
            if got.lang() != exp.after.lang {
                let (g, x) = (got.lang(), &exp.after.lang);
                problems.push(tr!(
                    "{label}: language {g} instead of {x}",
                    "{label}: Sprache {g} statt {x}"
                ));
            }
            let got_name = got.name.as_deref().filter(|n| !n.is_empty());
            if got_name != exp.after.name.as_deref() {
                let g = got_name.unwrap_or("");
                let x = exp.after.name.as_deref().unwrap_or("");
                problems.push(tr!(
                    "{label}: name \"{g}\" instead of \"{x}\"",
                    "{label}: Name „{g}“ statt „{x}“"
                ));
            }
            if got.flags.default != exp.after.default {
                let v = got.flags.default;
                problems.push(tr!(
                    "{label}: default flag {v}",
                    "{label}: Standard-Flag {v}"
                ));
            }
            if got.flags.forced != exp.after.forced {
                let v = got.flags.forced;
                problems.push(tr!("{label}: forced flag {v}", "{label}: Forced-Flag {v}"));
            }
        }
    }
    if let (Some(before), Some(after)) = (plan.duration_ns, result.duration_ns) {
        let secs = |ns: u64| ns as f64 / 1e9;
        if after > before + MAX_DURATION_DIFF_NS {
            let (a, b) = (secs(after), secs(before));
            problems.push(tr!(
                "duration became longer: {a:.3} s instead of {b:.3} s",
                "Dauer ist länger geworden: {a:.3} s statt {b:.3} s"
            ));
        } else if before.saturating_sub(after) > allowed_shortening_ns(plan) {
            let (a, b, c) = (secs(after), secs(before), secs(allowed_shortening_ns(plan)));
            problems.push(tr!(
                "duration differs: {a:.3} s instead of {b:.3} s (allowed: up to {c:.1} s shorter)",
                "Dauer weicht ab: {a:.3} s statt {b:.3} s (erlaubt: bis {c:.1} s kürzer)"
            ));
        }
    }
    if result.attachments != plan.attachments {
        let (a, b) = (result.attachments, plan.attachments);
        problems.push(tr!(
            "{a} attachments instead of {b}",
            "{a} Anhänge statt {b}"
        ));
    }
    if result.chapters != plan.chapters {
        let (a, b) = (result.chapters, plan.chapters);
        problems.push(tr!("{a} chapters instead of {b}", "{a} Kapitel statt {b}"));
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
    use crate::probe::parse_identification;
    use crate::selection::{DefaultChoice, Selection, TrackRef, TypeSelection, plan_file};
    use std::path::Path;

    fn sel(keep: Option<Vec<u32>>, default: Option<u32>) -> Selection {
        Selection {
            audio: TypeSelection {
                keep: keep.map(|k| k.into_iter().map(TrackRef::Number).collect()),
                default: match default {
                    Some(n) => DefaultChoice::Default(Some(TrackRef::Number(n))),
                    None => DefaultChoice::Default(None),
                },
            },
            ..Default::default()
        }
    }

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
        let plan = plan_file(&m, &sel(None, Some(1))).expect("plan");
        assert_eq!(verify(&plan, &m), Ok(()));
    }

    #[test]
    fn detects_deviations() {
        let m = media();
        let plan = plan_file(&m, &sel(Some(vec![1]), None)).expect("plan");
        let errs = verify(&plan, &m).expect_err("Spuranzahl");
        assert!(errs[0].contains("tracks, found"), "{errs:?}");

        let plan = plan_file(&m, &sel(None, Some(2))).expect("plan");
        let mut bad = m.clone();
        bad.duration_ns = Some(9_000_000_000);
        bad.size = 0;
        bad.attachments = 0;
        let errs = verify(&plan, &bad).expect_err("abweichend");
        let all = errs.join("\n");
        for needle in ["empty", "default flag", "duration", "attachments"] {
            assert!(all.contains(needle), "{needle}: {all}");
        }
    }

    #[test]
    fn shorter_after_removing_tracks_is_tolerated() {
        let mut m = media();
        m.duration_ns = Some(1_422_000_000_000); // 23:42
        let plan = plan_file(&m, &sel(Some(vec![1]), None)).expect("plan");
        assert_eq!(allowed_shortening_ns(&plan), 14_220_000_000, "1 % > 5 s");
        let mut result = m.clone();
        result
            .tracks
            .retain(|t| !(t.kind == crate::model::TrackType::Audio && t.type_index > 1));
        for t in result.tracks.iter_mut() {
            if t.kind == crate::model::TrackType::Audio {
                t.flags.default = false;
            }
        }
        // Realer Fall: entfernte Spur lief 1,9 s länger als der Rest.
        result.duration_ns = Some(1_420_095_000_000);
        assert_eq!(verify(&plan, &result), Ok(()));
        // Deutlich kürzer (abgeschnitten) bleibt ein Fehler.
        result.duration_ns = Some(1_300_000_000_000);
        assert!(verify(&plan, &result).is_err());
        // Länger werden darf es nie.
        result.duration_ns = Some(1_424_000_000_000);
        let errs = verify(&plan, &result).expect_err("länger");
        assert!(errs.iter().any(|e| e.contains("longer")), "{errs:?}");
    }

    #[test]
    fn tolerance_without_removed_tracks_stays_strict() {
        let m = media();
        let plan = plan_file(&m, &sel(None, Some(2))).expect("plan");
        assert_eq!(allowed_shortening_ns(&plan), MAX_DURATION_DIFF_NS);
        let short = media();
        let plan_short = plan_file(&short, &sel(Some(vec![1, 2]), None)).expect("plan");
        assert_eq!(
            allowed_shortening_ns(&plan_short),
            MIN_SHORTENING_NS,
            "5-s-Fixture: Mindestwert"
        );
    }
}
