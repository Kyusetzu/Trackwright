//! Session-Auswahl wie im Vorbild mkv-muxing-batch-gui:
//!
//! - „Nur diese Audios/Untertitel behalten“: Mehrfachauswahl aus Nummer (pro Typ), Sprache und
//!   Name über alle geladenen Dateien. Eine Spur bleibt, wenn sie *irgendeinen* Eintrag trifft.
//! - „Diese Audio/Untertitel als Standard“: genau ein Eintrag; halb angehakt = nur Standard,
//!   ganz angehakt = Standard + Forced.
//!
//! Fehlt eine ausgewählte Spur in einer Datei, wird die Datei übersprungen statt stillschweigend
//! weniger zu behalten.

use crate::lang::Lang;
use crate::model::{MediaInfo, TrackInfo, TrackType};
use crate::plan::{FilePlan, TrackPlan, TrackState};
use std::collections::BTreeSet;
use std::fmt;

/// Ein Eintrag der Auswahl.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TrackRef {
    /// Position im Typ, 1-basiert („Audio 2“).
    Number(u32),
    /// Normalisierte Sprache; [`Lang::Undefined`] ist ein eigener Eintrag.
    Lang(Lang),
    /// Exakter Spurname.
    Name(String),
}

impl TrackRef {
    pub fn matches(&self, t: &TrackInfo) -> bool {
        match self {
            TrackRef::Number(n) => t.type_index == *n,
            TrackRef::Lang(l) => t.lang() == *l,
            TrackRef::Name(n) => t.name.as_deref() == Some(n.as_str()),
        }
    }

    /// Anzeigetext, z. B. „Nr. 2“, „Deutsch (ger)“, „„Signs““.
    pub fn label(&self) -> String {
        match self {
            TrackRef::Number(n) => crate::tr!("No. {n}", "Nr. {n}"),
            TrackRef::Lang(l) => l.display_name(),
            TrackRef::Name(n) => crate::tr!("\"{n}\"", "„{n}“"),
        }
    }
}

impl fmt::Display for TrackRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label())
    }
}

/// „Diese … als Standard“.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DefaultChoice {
    /// Checkbox aus: Flags bleiben unverändert.
    #[default]
    Off,
    /// Halb angehakt: Standard für die gewählte Spur, bei allen anderen entfernen.
    /// `None` = bei allen Spuren des Typs entfernen.
    Default(Option<TrackRef>),
    /// Ganz angehakt: wie [`DefaultChoice::Default`], zusätzlich Forced für die gewählte Spur
    /// (alle anderen verlieren Forced).
    DefaultForced(Option<TrackRef>),
}

/// Auswahl für einen Spurtyp.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeSelection {
    /// `None` = Checkbox „Nur diese behalten“ aus (alle behalten).
    /// `Some(leer)` = alle Spuren dieses Typs verwerfen.
    pub keep: Option<BTreeSet<TrackRef>>,
    pub default: DefaultChoice,
}

impl TypeSelection {
    pub fn is_inactive(&self) -> bool {
        self.keep.is_none() && self.default == DefaultChoice::Off
    }
}

/// Auswahl der Session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub audio: TypeSelection,
    pub subtitles: TypeSelection,
}

impl Selection {
    pub fn for_type(&self, kind: TrackType) -> Option<&TypeSelection> {
        match kind {
            TrackType::Audio => Some(&self.audio),
            TrackType::Subtitles => Some(&self.subtitles),
            _ => None,
        }
    }

    pub fn for_type_mut(&mut self, kind: TrackType) -> Option<&mut TypeSelection> {
        match kind {
            TrackType::Audio => Some(&mut self.audio),
            TrackType::Subtitles => Some(&mut self.subtitles),
            _ => None,
        }
    }
}

/// Grund, warum eine Datei übersprungen wird.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanIssue {
    Missing {
        kind: TrackType,
        refs: Vec<TrackRef>,
    },
    DefaultNotKept {
        kind: TrackType,
        track: TrackRef,
    },
}

impl fmt::Display for PlanIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanIssue::Missing { kind, refs } => {
                let r = join(refs);
                f.write_str(&crate::tr!(
                    "{kind}: {r} not present – file will be skipped",
                    "{kind}: {r} nicht vorhanden – Datei wird übersprungen"
                ))
            }
            PlanIssue::DefaultNotKept { kind, track } => f.write_str(&crate::tr!(
                "{kind}: default track {track} is not kept – file will be skipped",
                "{kind}: Standard-Spur {track} wird nicht behalten – Datei wird übersprungen"
            )),
        }
    }
}

impl std::error::Error for PlanIssue {}

fn join(refs: &[TrackRef]) -> String {
    refs.iter()
        .map(TrackRef::label)
        .collect::<Vec<_>>()
        .join(", ")
}

const EDITABLE: [TrackType; 2] = [TrackType::Audio, TrackType::Subtitles];

/// Auswählbare Einträge eines Typs über alle Dateien: Nummern, Sprachen, Namen (je sortiert).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub numbers: Vec<TrackRef>,
    pub languages: Vec<TrackRef>,
    pub names: Vec<TrackRef>,
}

impl Options {
    pub fn is_empty(&self) -> bool {
        self.numbers.is_empty() && self.languages.is_empty() && self.names.is_empty()
    }

    pub fn all(&self) -> impl Iterator<Item = &TrackRef> {
        self.numbers
            .iter()
            .chain(&self.languages)
            .chain(&self.names)
    }
}

/// Sammelt die Einträge für die Auswahllisten aus allen Dateien.
pub fn options<'a>(files: impl IntoIterator<Item = &'a MediaInfo>, kind: TrackType) -> Options {
    let mut max = 0;
    let mut langs = BTreeSet::new();
    let mut names = BTreeSet::new();
    for m in files {
        for t in m.tracks_of(kind) {
            max = max.max(t.type_index);
            langs.insert(t.lang());
            if let Some(n) = t.name.as_deref().filter(|n| !n.is_empty()) {
                names.insert(n.to_string());
            }
        }
    }
    let mut languages: Vec<Lang> = langs.into_iter().collect();
    languages.sort_by_key(|l| (l.is_undefined(), l.display_name()));
    Options {
        numbers: (1..=max).map(TrackRef::Number).collect(),
        languages: languages.into_iter().map(TrackRef::Lang).collect(),
        names: names.into_iter().map(TrackRef::Name).collect(),
    }
}

/// Erstellt den Plan für eine Datei.
pub fn plan_file(media: &MediaInfo, sel: &Selection) -> Result<FilePlan, PlanIssue> {
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
        let Some(ts) = sel.for_type(kind) else {
            continue;
        };
        let of_type: Vec<usize> = (0..media.tracks.len())
            .filter(|&i| media.tracks[i].kind == kind)
            .collect();

        // Behalten
        if let Some(keep) = &ts.keep {
            let missing: Vec<TrackRef> = keep
                .iter()
                .filter(|r| !of_type.iter().any(|&i| r.matches(&media.tracks[i])))
                .cloned()
                .collect();
            if !missing.is_empty() {
                return Err(PlanIssue::Missing {
                    kind,
                    refs: missing,
                });
            }
            for &i in &of_type {
                tracks[i].keep = keep.iter().any(|r| r.matches(&media.tracks[i]));
            }
        }

        // Standard (+ Forced)
        let (choice, forced) = match &ts.default {
            DefaultChoice::Off => continue,
            DefaultChoice::Default(c) => (c, false),
            DefaultChoice::DefaultForced(c) => (c, true),
        };
        let target = match choice {
            None => None,
            Some(r) => {
                let hit = of_type
                    .iter()
                    .copied()
                    .find(|&i| tracks[i].keep && r.matches(&media.tracks[i]));
                match hit {
                    Some(i) => Some(i),
                    None if of_type.iter().any(|&i| r.matches(&media.tracks[i])) => {
                        return Err(PlanIssue::DefaultNotKept {
                            kind,
                            track: r.clone(),
                        });
                    }
                    None => {
                        return Err(PlanIssue::Missing {
                            kind,
                            refs: vec![r.clone()],
                        });
                    }
                }
            }
        };
        for &i in &of_type {
            if !tracks[i].keep {
                continue;
            }
            let is_target = Some(i) == target;
            tracks[i].after.default = is_target;
            if forced {
                tracks[i].after.forced = is_target;
            }
        }
    }

    // Entfernte Spuren behalten ihren Ausgangszustand (nur Anzeige).
    for t in tracks.iter_mut().filter(|t| !t.keep) {
        t.after = t.before.clone();
    }

    let mut warnings = Vec::new();
    for kind in EDITABLE {
        let total = tracks.iter().filter(|t| t.kind == kind).count();
        let kept = tracks.iter().filter(|t| t.kind == kind && t.keep).count();
        if total > 0 && kept == 0 {
            warnings.push(crate::tr!(
                "all {kind} tracks will be discarded",
                "alle {kind}-Spuren werden verworfen"
            ));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Strategy;
    use crate::probe::parse_identification;
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

    fn keep(refs: &[TrackRef]) -> Option<BTreeSet<TrackRef>> {
        Some(refs.iter().cloned().collect())
    }

    fn lang(c: &str) -> TrackRef {
        TrackRef::Lang(Lang::parse(c))
    }

    fn name(n: &str) -> TrackRef {
        TrackRef::Name(n.into())
    }

    /// Pro Spur des Typs: „-“ entfernt, sonst D/d (Standard) und F/f (Forced).
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

    // full.json: Audio 1 ger „Deutsch“ (Standard), Audio 2 jpn „日本語“, Audio 3 eng „Kommentar“;
    //            Untertitel 1 ger „Deutsch“, 2 ger „Signs“ (forced), 3 eng „English“.

    #[test]
    fn table() {
        let m = media("full.json");
        struct Case {
            name: &'static str,
            sel: Selection,
            audio: &'static str,
            subs: &'static str,
            strategy: Strategy,
        }
        let cases = vec![
            Case {
                name: "nichts ausgewählt",
                sel: Selection::default(),
                audio: "Df df df",
                subs: "df dF df",
                strategy: Strategy::Nothing,
            },
            Case {
                name: "Behalten nach Sprache (ODER-Verknüpfung)",
                sel: Selection {
                    audio: TypeSelection {
                        keep: keep(&[lang("ger"), lang("jpn")]),
                        ..Default::default()
                    },
                    subtitles: TypeSelection {
                        keep: keep(&[TrackRef::Number(1), name("Signs")]),
                        ..Default::default()
                    },
                },
                audio: "Df df -",
                subs: "df dF -",
                strategy: Strategy::Remux,
            },
            Case {
                name: "alle Untertitel verwerfen",
                sel: Selection {
                    subtitles: TypeSelection {
                        keep: keep(&[]),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                audio: "Df df df",
                subs: "- - -",
                strategy: Strategy::Remux,
            },
            Case {
                name: "Standard halb (nur Standard)",
                sel: Selection {
                    audio: TypeSelection {
                        default: DefaultChoice::Default(Some(lang("jpn"))),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                audio: "df Df df",
                subs: "df dF df",
                strategy: Strategy::Propedit,
            },
            Case {
                name: "Standard + Forced",
                sel: Selection {
                    subtitles: TypeSelection {
                        default: DefaultChoice::DefaultForced(Some(TrackRef::Number(3))),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                audio: "Df df df",
                subs: "df df DF",
                strategy: Strategy::Propedit,
            },
            Case {
                name: "Standard bei allen entfernen",
                sel: Selection {
                    audio: TypeSelection {
                        default: DefaultChoice::Default(None),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                audio: "df df df",
                subs: "df dF df",
                strategy: Strategy::Propedit,
            },
            Case {
                name: "Standard = erste passende behaltene Spur",
                sel: Selection {
                    subtitles: TypeSelection {
                        keep: keep(&[name("Signs"), lang("eng")]),
                        default: DefaultChoice::Default(Some(lang("ger"))),
                    },
                    ..Default::default()
                },
                audio: "Df df df",
                subs: "- DF df",
                strategy: Strategy::Remux,
            },
        ];
        for c in cases {
            let p = plan_file(&m, &c.sel).unwrap_or_else(|e| panic!("{}: {e}", c.name));
            assert_eq!(summary(&p, TrackType::Audio), c.audio, "{} (Audio)", c.name);
            assert_eq!(
                summary(&p, TrackType::Subtitles),
                c.subs,
                "{} (Untertitel)",
                c.name
            );
            assert_eq!(p.strategy, c.strategy, "{}", c.name);
            assert!(p.tracks[0].keep, "Video bleibt");
        }
    }

    #[test]
    fn missing_selection_skips_file() {
        let m = media("full.json");
        let sel = Selection {
            audio: TypeSelection {
                keep: keep(&[TrackRef::Number(1), TrackRef::Number(4), lang("fre")]),
                ..Default::default()
            },
            ..Default::default()
        };
        let err = plan_file(&m, &sel).expect_err("fehlt");
        assert_eq!(
            err,
            PlanIssue::Missing {
                kind: TrackType::Audio,
                refs: vec![TrackRef::Number(4), lang("fre")]
            }
        );
        assert!(err.to_string().contains("No. 4"), "{err}");

        let sel = Selection {
            subtitles: TypeSelection {
                default: DefaultChoice::Default(Some(name("Gibt es nicht"))),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(matches!(
            plan_file(&m, &sel),
            Err(PlanIssue::Missing {
                kind: TrackType::Subtitles,
                ..
            })
        ));

        let sel = Selection {
            audio: TypeSelection {
                keep: keep(&[TrackRef::Number(1)]),
                default: DefaultChoice::Default(Some(TrackRef::Number(2))),
            },
            ..Default::default()
        };
        assert!(matches!(
            plan_file(&m, &sel),
            Err(PlanIssue::DefaultNotKept { .. })
        ));
    }

    #[test]
    fn undefined_is_its_own_language() {
        let m = media("und.json");
        let sel = Selection {
            audio: TypeSelection {
                keep: keep(&[lang("ger")]),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(plan_file(&m, &sel).is_err(), "und zählt nie als ger");
        let sel = Selection {
            audio: TypeSelection {
                keep: keep(&[TrackRef::Lang(Lang::Undefined)]),
                ..Default::default()
            },
            ..Default::default()
        };
        let p = plan_file(&m, &sel).expect("plan");
        assert_eq!(p.strategy, Strategy::Nothing);
    }

    #[test]
    fn options_union_over_files() {
        let full = media("full.json");
        let und = media("und.json");
        let o = options([&full, &und], TrackType::Audio);
        assert_eq!(o.numbers, (1..=3).map(TrackRef::Number).collect::<Vec<_>>());
        let langs: Vec<String> = o.languages.iter().map(TrackRef::label).collect();
        assert_eq!(
            langs,
            vec![
                "English (eng)",
                "German (ger)",
                "Japanese (jpn)",
                "undefined"
            ]
        );
        let names: Vec<String> = o.names.iter().map(TrackRef::label).collect();
        assert_eq!(
            names,
            vec!["\"Deutsch\"", "\"English\"", "\"Kommentar\"", "\"日本語\""]
        );
        assert!(options(std::iter::empty(), TrackType::Audio).is_empty());
    }
}
