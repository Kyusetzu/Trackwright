//! Regel-Engine: reine Funktion `(MediaInfo, Regeln, Overrides) -> Plan`.
//!
//! Reihenfolge pro Spurtyp:
//! 1. Prüfen, ob alle referenzierten Nummern/IDs existieren (sonst Datei überspringen).
//! 2. „Sprache setzen“ und „Name setzen“.
//! 3. „Behalten“, „Standard“ und „Forced“ gegen den neuen Zustand auswerten.
//! 4. Overrides der Gruppe anwenden.

use crate::lang::{Lang, suggest_from_name};
use crate::model::{MediaInfo, Signature, TrackType};
use crate::plan::{FilePlan, TrackPlan, TrackState};
use crate::rules::{EvalTrack, FlagRule, MissingRef, RuleSet};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// Manuelle Abweichung für eine Spur (Gruppenansicht). `None` = Regel gilt.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackOverride {
    pub keep: Option<bool>,
    pub default: Option<bool>,
    pub forced: Option<bool>,
}

impl TrackOverride {
    pub fn is_empty(&self) -> bool {
        *self == TrackOverride::default()
    }
}

/// Overrides pro Spurposition (gleiche Signatur ⇒ gleiche Positionen).
pub type Overrides = BTreeMap<usize, TrackOverride>;

/// Optionen für die Planung.
#[derive(Debug, Clone, Default)]
pub struct PlanOptions {
    /// Nur Dateien mit dieser Spursignatur planen (Referenzdatei).
    pub reference: Option<Signature>,
    /// Dateien bearbeiten, bei denen keine Audiospur übrig bliebe. Standard: überspringen, weil
    /// das fast immer ein Regelfehler ist (z. B. Sprachregel bei undefinierten Spuren).
    pub allow_no_audio: bool,
}

/// Grund, warum eine Datei übersprungen wird.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanIssue {
    #[error(
        "{kind}: {} existiert nicht – Datei wird übersprungen",
        join_refs(refs)
    )]
    MissingTrack {
        kind: TrackType,
        refs: Vec<MissingRef>,
    },
    #[error("Spuraufbau weicht von der Referenzdatei ab – Datei wird übersprungen")]
    SignatureMismatch,
    #[error(
        "keine Audiospur bliebe übrig – Datei wird übersprungen{}",
        if *undefined { " (Spuren ohne Sprache: zuerst Sprache setzen)" } else { "" }
    )]
    NoAudioLeft { undefined: bool },
}

fn join_refs(refs: &[MissingRef]) -> String {
    let v: Vec<String> = refs.iter().map(|r| r.to_string()).collect();
    v.join(", ")
}

impl fmt::Display for TrackOverride {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if let Some(k) = self.keep {
            parts.push(if k { "behalten" } else { "entfernen" });
        }
        if let Some(d) = self.default {
            parts.push(if d { "Standard an" } else { "Standard aus" });
        }
        if let Some(fo) = self.forced {
            parts.push(if fo { "Forced an" } else { "Forced aus" });
        }
        f.write_str(&parts.join(", "))
    }
}

const EDITABLE: [TrackType; 2] = [TrackType::Audio, TrackType::Subtitles];

/// Erstellt den Plan für eine Datei.
pub fn plan_file(
    media: &MediaInfo,
    rules: &RuleSet,
    overrides: Option<&Overrides>,
    opts: &PlanOptions,
) -> Result<FilePlan, PlanIssue> {
    if let Some(reference) = &opts.reference
        && media.signature() != *reference
    {
        return Err(PlanIssue::SignatureMismatch);
    }

    // 1. Fehlende Nummern
    for kind in EDITABLE {
        let Some(tr) = rules.for_type(kind) else {
            continue;
        };
        let ids: Vec<u64> = media.tracks_of(kind).map(|t| t.id).collect();
        let count = ids.len() as u32;
        let mut missing = Vec::new();
        for e in tr.exprs() {
            e.missing_refs(count, &ids, &mut missing);
        }
        missing.dedup();
        if !missing.is_empty() {
            return Err(PlanIssue::MissingTrack {
                kind,
                refs: missing,
            });
        }
    }

    let mut tracks: Vec<TrackPlan> = media
        .tracks
        .iter()
        .enumerate()
        .map(|(position, t)| {
            let state = TrackState {
                lang: t.lang(),
                name: t.name.clone().filter(|n| !n.is_empty()),
                default: t.flags.default,
                forced: t.flags.forced,
            };
            TrackPlan {
                position,
                id: t.id,
                uid: t.uid,
                number: t.number,
                kind: t.kind,
                type_index: t.type_index,
                codec: t.codec.clone(),
                keep: true,
                before: state.clone(),
                after: state,
            }
        })
        .collect();

    for kind in EDITABLE {
        let Some(tr) = rules.for_type(kind) else {
            continue;
        };
        let count = media.count_of(kind) as u32;
        let positions: Vec<usize> = media
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.kind == kind)
            .map(|(i, _)| i)
            .collect();

        // 2. Sprache und Name setzen (ausgewertet gegen den Originalzustand).
        for &pos in &positions {
            let track = &media.tracks[pos];
            let orig_lang = track.lang();
            let orig_name = track.name.as_deref();
            let ev = EvalTrack {
                track,
                lang: &orig_lang,
                name: orig_name,
                count,
            };
            for a in &tr.set_language {
                if a.when.eval(&ev) {
                    tracks[pos].after.lang = Lang::parse(&a.lang);
                }
            }
            for a in &tr.set_name {
                if a.when.eval(&ev) {
                    let n = a.name.trim();
                    tracks[pos].after.name = (!n.is_empty()).then(|| n.to_string());
                }
            }
        }

        // 3. Behalten / Standard / Forced gegen den neuen Zustand.
        let mut default_given = false;
        for &pos in &positions {
            let track = &media.tracks[pos];
            let lang = tracks[pos].after.lang.clone();
            let name = tracks[pos].after.name.clone();
            let ev = EvalTrack {
                track,
                lang: &lang,
                name: name.as_deref(),
                count,
            };
            let plan = &mut tracks[pos];
            plan.keep = tr.keep.as_ref().is_none_or(|e| e.eval(&ev));
            if !plan.keep {
                continue;
            }
            match &tr.default {
                FlagRule::Unchanged => {}
                FlagRule::None => plan.after.default = false,
                FlagRule::Match(e) => {
                    let hit = !default_given && e.eval(&ev);
                    default_given |= hit;
                    plan.after.default = hit;
                }
            }
            match &tr.forced {
                FlagRule::Unchanged => {}
                FlagRule::None => plan.after.forced = false,
                FlagRule::Match(e) => plan.after.forced = e.eval(&ev),
            }
        }
    }

    // 4. Overrides (nur Audio/Untertitel).
    if let Some(ov) = overrides {
        for (pos, o) in ov {
            let Some(plan) = tracks.get_mut(*pos) else {
                continue;
            };
            if !EDITABLE.contains(&plan.kind) {
                continue;
            }
            if let Some(k) = o.keep {
                plan.keep = k;
            }
            if let Some(d) = o.default {
                plan.after.default = d;
            }
            if let Some(f) = o.forced {
                plan.after.forced = f;
            }
        }
    }

    // Entfernte Spuren behalten ihren Ausgangszustand (nur Anzeige).
    for t in tracks.iter_mut().filter(|t| !t.keep) {
        t.after = t.before.clone();
    }

    let audio_total = tracks.iter().filter(|t| t.kind == TrackType::Audio).count();
    let audio_kept = tracks
        .iter()
        .filter(|t| t.kind == TrackType::Audio && t.keep)
        .count();
    if audio_total > 0 && audio_kept == 0 && !opts.allow_no_audio {
        return Err(PlanIssue::NoAudioLeft {
            undefined: media
                .tracks_of(TrackType::Audio)
                .any(|t| t.lang().is_undefined()),
        });
    }

    let mut warnings = Vec::new();
    for kind in EDITABLE {
        let total = tracks.iter().filter(|t| t.kind == kind).count();
        let kept = tracks.iter().filter(|t| t.kind == kind && t.keep).count();
        if total > 0 && kept == 0 {
            warnings.push(format!("keine {kind}-Spur bleibt übrig"));
        }
        let defaults = tracks
            .iter()
            .filter(|t| t.kind == kind && t.keep && t.after.default)
            .count();
        if defaults > 1 {
            warnings.push(format!("{defaults} {kind}-Spuren haben das Standard-Flag"));
        }
    }
    let undefined_kept = tracks
        .iter()
        .filter(|t| t.keep && EDITABLE.contains(&t.kind) && t.after.lang.is_undefined())
        .count();
    if undefined_kept > 0 {
        warnings.push(format!(
            "{undefined_kept} behaltene Spur(en) ohne Sprache (undefiniert)"
        ));
    }

    let strategy = FilePlan::compute_strategy(&tracks);
    Ok(FilePlan {
        path: media.path.clone(),
        size: media.size,
        duration_ns: media.duration_ns,
        attachments: media.attachments,
        chapters: media.chapters,
        tracks,
        strategy,
        warnings,
    })
}

/// Sprachvorschlag für eine Spur ohne Sprache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub position: usize,
    pub kind: TrackType,
    pub type_index: u32,
    pub name: String,
    pub lang: Lang,
}

/// Vorschläge aus Spurnamen für Spuren mit undefinierter Sprache. Werden nie automatisch
/// angewendet.
pub fn language_suggestions(media: &MediaInfo) -> Vec<Suggestion> {
    media
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| EDITABLE.contains(&t.kind) && t.lang().is_undefined())
        .filter_map(|(position, t)| {
            let name = t.name.as_deref()?;
            Some(Suggestion {
                position,
                kind: t.kind,
                type_index: t.type_index,
                name: name.to_string(),
                lang: suggest_from_name(name)?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Change, Strategy, TrackStatus};
    use crate::probe::parse_identification;
    use crate::rules::{Expr, LangAssign, NameAssign, NumSpec, TypeRules};
    use std::path::Path;

    fn media(name: &str) -> MediaInfo {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/json")
            .join(name);
        let json = std::fs::read_to_string(p).expect("fixture");
        let mut m = parse_identification(&json, Path::new("/media/x.mkv")).expect("parse");
        m.size = 1000;
        m
    }

    fn e(s: &str) -> Expr {
        s.parse().expect("expr")
    }

    fn fr(s: &str) -> FlagRule {
        s.parse().expect("flagrule")
    }

    fn rules(audio: TypeRules, subtitles: TypeRules) -> RuleSet {
        RuleSet { audio, subtitles }
    }

    fn keep(s: &str) -> TypeRules {
        TypeRules {
            keep: Some(e(s)),
            ..Default::default()
        }
    }

    /// Kompakte Darstellung: pro Audio/Untertitel-Spur „K/R“ + Default/Forced.
    fn summary(p: &FilePlan, kind: TrackType) -> String {
        p.tracks
            .iter()
            .filter(|t| t.kind == kind)
            .map(|t| {
                if !t.keep {
                    "-".to_string()
                } else {
                    format!(
                        "{}{}",
                        if t.after.default { "D" } else { "d" },
                        if t.after.forced { "F" } else { "f" }
                    )
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    // full.json: Audio 1 ger „Deutsch“ (default), Audio 2 jpn „日本語“, Audio 3 eng „Kommentar“
    //            (commentary, 1 Kanal); Untertitel 1 ger „Deutsch“, 2 ger „Signs“ (forced),
    //            3 eng „English“.

    #[test]
    fn table_keep_default_forced() {
        let m = media("full.json");
        struct Case {
            name: &'static str,
            audio: TypeRules,
            subs: TypeRules,
            audio_expect: &'static str,
            subs_expect: &'static str,
            strategy: Strategy,
        }
        let cases = vec![
            Case {
                name: "keine Regeln",
                audio: TypeRules::default(),
                subs: TypeRules::default(),
                audio_expect: "Df df df",
                subs_expect: "df dF df",
                strategy: Strategy::Nothing,
            },
            Case {
                name: "Handover-Beispiel",
                audio: TypeRules {
                    keep: Some(e("Nr. 1 ODER Sprache in {ger, jpn}")),
                    default: fr("Nr. 1"),
                    ..Default::default()
                },
                subs: TypeRules {
                    keep: Some(e("Nr. 1,2 UND Name !~ /(?i)kommentar|commentary/")),
                    forced: fr("Nr. 2 ODER Name ~ /(?i)forced|signs|schilder/"),
                    default: fr("keine"),
                    ..Default::default()
                },
                audio_expect: "Df df -",
                subs_expect: "df dF -",
                strategy: Strategy::Remux,
            },
            Case {
                name: "nur Flags → propedit",
                audio: TypeRules {
                    default: fr("Sprache = jpn"),
                    ..Default::default()
                },
                subs: TypeRules {
                    default: fr("Name ~ /Signs/"),
                    forced: fr("keine"),
                    ..Default::default()
                },
                audio_expect: "df Df df",
                subs_expect: "df Df df",
                strategy: Strategy::Propedit,
            },
            Case {
                name: "Standard nur für ersten Treffer",
                audio: TypeRules {
                    default: fr("alle"),
                    ..Default::default()
                },
                subs: TypeRules {
                    forced: fr("Sprache = ger"),
                    ..Default::default()
                },
                audio_expect: "Df df df",
                subs_expect: "dF dF df",
                strategy: Strategy::Propedit,
            },
            Case {
                name: "Standard folgt auf behaltene Spur",
                audio: TypeRules {
                    keep: Some(e("Sprache != ger")),
                    default: fr("alle"),
                    ..Default::default()
                },
                subs: TypeRules::default(),
                audio_expect: "- Df df",
                subs_expect: "df dF df",
                strategy: Strategy::Remux,
            },
            Case {
                name: "Flag-, Kanal- und Codec-Kriterien",
                audio: keep("NICHT Flag kommentar UND Kanäle >= 2 UND Codec = aac"),
                subs: keep("Codec in {ass, substationalpha}"),
                audio_expect: "Df df -",
                subs_expect: "- dF -",
                strategy: Strategy::Remux,
            },
            Case {
                name: "letzte und Bereich",
                audio: keep("Nr. letzte"),
                subs: keep("Nr. 2-3"),
                audio_expect: "- - df",
                subs_expect: "- dF df",
                strategy: Strategy::Remux,
            },
            Case {
                name: "absolute ID",
                audio: keep("ID 2"),
                subs: TypeRules::default(),
                audio_expect: "- df -",
                subs_expect: "df dF df",
                strategy: Strategy::Remux,
            },
        ];
        for c in cases {
            let p = plan_file(&m, &rules(c.audio, c.subs), None, &PlanOptions::default())
                .unwrap_or_else(|err| panic!("{}: {err}", c.name));
            assert_eq!(
                summary(&p, TrackType::Audio),
                c.audio_expect,
                "{} (Audio)",
                c.name
            );
            assert_eq!(
                summary(&p, TrackType::Subtitles),
                c.subs_expect,
                "{} (Untertitel)",
                c.name
            );
            assert_eq!(p.strategy, c.strategy, "{}", c.name);
            // Video bleibt immer.
            assert!(p.tracks[0].keep && p.tracks[0].changes().is_empty());
        }
    }

    #[test]
    fn missing_number_skips_file() {
        let m = media("full.json");
        let r = rules(keep("Nr. 1 ODER Nr. 4"), TypeRules::default());
        let err = plan_file(&m, &r, None, &PlanOptions::default()).expect_err("fehlt");
        assert_eq!(
            err,
            PlanIssue::MissingTrack {
                kind: TrackType::Audio,
                refs: vec![MissingRef::Number(NumSpec::Index(4))]
            }
        );
        assert!(err.to_string().contains("Nr. 4"));

        // Auch in Standard-/Forced-Regeln und für IDs anderer Typen.
        let r = rules(
            TypeRules::default(),
            TypeRules {
                forced: fr("ID 1"), // ID 1 ist eine Audiospur
                ..Default::default()
            },
        );
        assert!(matches!(
            plan_file(&m, &r, None, &PlanOptions::default()),
            Err(PlanIssue::MissingTrack {
                kind: TrackType::Subtitles,
                ..
            })
        ));
    }

    #[test]
    fn undefined_language_never_matches_a_language() {
        let m = media("und.json");
        // Weder „Sprache = ger“ noch „Sprache != ger“ dürfen undefinierte Spuren als ger zählen.
        let r = rules(keep("Sprache = ger"), keep("Sprache in {ger, eng}"));
        assert_eq!(
            plan_file(&m, &r, None, &PlanOptions::default()),
            Err(PlanIssue::NoAudioLeft { undefined: true })
        );
        let allow = PlanOptions {
            allow_no_audio: true,
            ..Default::default()
        };
        let p = plan_file(&m, &r, None, &allow).expect("plan");
        assert_eq!(summary(&p, TrackType::Audio), "- -");
        assert_eq!(summary(&p, TrackType::Subtitles), "- -");
        assert!(p.warnings.iter().any(|w| w.contains("Audio")));

        let p = plan_file(
            &m,
            &rules(keep("Sprache = undefiniert"), TypeRules::default()),
            None,
            &PlanOptions::default(),
        )
        .expect("plan");
        assert_eq!(summary(&p, TrackType::Audio), "df df");
        assert!(p.warnings.iter().any(|w| w.contains("ohne Sprache")));
    }

    #[test]
    fn set_language_before_rules() {
        let m = media("und.json");
        let audio = TypeRules {
            set_language: vec![
                LangAssign {
                    when: e("Nr. 1"),
                    lang: "jpn".into(),
                },
                LangAssign {
                    when: e("Name ~ /(?i)deutsch|german|ger/"),
                    lang: "de".into(),
                },
            ],
            keep: Some(e("Sprache = ger")),
            default: fr("Sprache = ger"),
            ..Default::default()
        };
        let subs = TypeRules {
            set_language: vec![LangAssign {
                when: e("Name ~ /English/"),
                lang: "eng".into(),
            }],
            set_name: vec![NameAssign {
                when: e("Nr. 1"),
                name: "".into(),
            }],
            ..Default::default()
        };
        let p = plan_file(&m, &rules(audio, subs), None, &PlanOptions::default()).expect("plan");
        // Audio 1 „Deutsch“: zuerst jpn, dann von der Namensregel auf ger überschrieben → behalten.
        let a1 = &p.tracks[1];
        assert!(a1.keep);
        assert_eq!(a1.after.lang, Lang::Code("ger".into()));
        assert!(a1.after.default);
        // Audio 2 „English“ bleibt undefiniert → entfernt.
        assert!(!p.tracks[2].keep);
        // Untertitel 2 bekommt eng, Untertitel 1 verliert den Namen.
        assert_eq!(p.tracks[4].after.lang, Lang::Code("eng".into()));
        assert_eq!(
            p.tracks[3].changes(),
            vec![Change::Name {
                from: Some("Deutsch".into()),
                to: None
            }]
        );
        assert_eq!(p.strategy, Strategy::Remux);
    }

    #[test]
    fn language_only_is_propedit() {
        let m = media("und.json");
        let audio = TypeRules {
            set_language: vec![LangAssign {
                when: e("Nr. 1"),
                lang: "ger".into(),
            }],
            ..Default::default()
        };
        let p = plan_file(
            &m,
            &rules(audio, TypeRules::default()),
            None,
            &PlanOptions::default(),
        )
        .expect("plan");
        assert_eq!(p.strategy, Strategy::Propedit);
        assert_eq!(p.changed_count(), 1);
        assert_eq!(p.write_bytes(), 0);
        // Gleiche Sprache in anderer Schreibweise ist keine Änderung.
        let m = media("full.json");
        let audio = TypeRules {
            set_language: vec![LangAssign {
                when: e("Nr. 1"),
                lang: "deu".into(),
            }],
            ..Default::default()
        };
        let p = plan_file(
            &m,
            &rules(audio, TypeRules::default()),
            None,
            &PlanOptions::default(),
        )
        .expect("plan");
        assert_eq!(p.strategy, Strategy::Nothing);
    }

    #[test]
    fn overrides_win_over_rules() {
        let m = media("full.json");
        let r = rules(keep("Nr. 1"), TypeRules::default());
        let mut ov = Overrides::new();
        ov.insert(
            3,
            TrackOverride {
                keep: Some(true),
                default: Some(true),
                forced: None,
            },
        );
        ov.insert(
            5,
            TrackOverride {
                forced: Some(false),
                ..Default::default()
            },
        );
        ov.insert(
            0,
            TrackOverride {
                keep: Some(false),
                ..Default::default()
            },
        ); // Video: ignoriert
        let p = plan_file(&m, &r, Some(&ov), &PlanOptions::default()).expect("plan");
        assert_eq!(summary(&p, TrackType::Audio), "Df - Df");
        assert_eq!(summary(&p, TrackType::Subtitles), "df df df");
        assert!(p.tracks[0].keep);
        assert!(p.warnings.iter().any(|w| w.contains("Standard-Flag")));
        assert_eq!(p.tracks[2].status(), TrackStatus::Removed);
        assert_eq!(p.tracks[3].status(), TrackStatus::Changed);
    }

    #[test]
    fn signature_lock() {
        let full = media("full.json");
        let und = media("und.json");
        let opts = PlanOptions {
            reference: Some(full.signature()),
            ..Default::default()
        };
        assert!(plan_file(&full, &RuleSet::default(), None, &opts).is_ok());
        assert_eq!(
            plan_file(&und, &RuleSet::default(), None, &opts),
            Err(PlanIssue::SignatureMismatch)
        );
    }

    #[test]
    fn suggestions_for_undefined_tracks() {
        let s = language_suggestions(&media("und.json"));
        let got: Vec<(usize, String)> =
            s.iter().map(|s| (s.position, s.lang.to_string())).collect();
        assert_eq!(
            got,
            vec![
                (1, "ger".into()),
                (2, "eng".into()),
                (3, "ger".into()),
                (4, "eng".into())
            ]
        );
        assert!(language_suggestions(&media("full.json")).is_empty());
    }

    #[test]
    fn diff_rendering() {
        let m = media("full.json");
        let r = rules(
            TypeRules {
                keep: Some(e("Nr. 1,2")),
                default: fr("Nr. 2"),
                ..Default::default()
            },
            TypeRules::default(),
        );
        let p = plan_file(&m, &r, None, &PlanOptions::default()).expect("plan");
        let d = p.render_diff();
        assert!(d.contains("- Audio 3"), "{d}");
        assert!(d.contains("~ Audio 1"), "{d}");
        assert!(d.contains("Standard aus"), "{d}");
        assert!(d.contains("Standard an"), "{d}");
    }
}
