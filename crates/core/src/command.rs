//! Befehlsbau: exakte Argumentlisten für mkvpropedit und mkvmerge.

use crate::model::TrackType;
use crate::plan::{Change, FilePlan, TrackPlan};
use crate::tools::{Program, ToolCommand};
use std::path::Path;

/// Adresse einer Spur für mkvpropedit. Bevorzugt die UID, denn mkvpropedit zählt Nummern anders
/// als mkvmerge-IDs.
pub fn propedit_selector(t: &TrackPlan) -> String {
    selector(t.uid, t.number, t.kind, t.type_index)
}

/// Wie [`propedit_selector`], aus Einzelwerten: UID, sonst Track-Nummer, sonst Typ + Position.
pub fn selector(uid: Option<u64>, number: Option<u64>, kind: TrackType, type_index: u32) -> String {
    if let Some(uid) = uid {
        format!("track:={uid}")
    } else if let Some(n) = number {
        format!("track:@{n}")
    } else {
        let letter = match kind {
            TrackType::Audio => 'a',
            TrackType::Subtitles => 's',
            TrackType::Video => 'v',
            TrackType::Other => 'b',
        };
        format!("track:{letter}{type_index}")
    }
}

/// mkvpropedit-Aufruf für alle Änderungen des Plans an `target`. `None`, wenn nichts zu tun ist.
///
/// Entfernte Spuren werden ignoriert (dafür ist ein Remux nötig).
pub fn propedit_command(plan: &FilePlan, target: &Path) -> Option<ToolCommand> {
    let mut cmd = ToolCommand::new(Program::Mkvpropedit);
    cmd.arg(target);
    let mut any = false;
    for t in plan.kept() {
        let changes = t.changes();
        if changes.is_empty() {
            continue;
        }
        any = true;
        cmd.arg("--edit").arg(propedit_selector(t));
        for c in changes {
            match c {
                Change::Default(b) => {
                    cmd.arg("--set")
                        .arg(format!("flag-default={}", u8::from(b)));
                }
                Change::Forced(b) => {
                    cmd.arg("--set").arg(format!("flag-forced={}", u8::from(b)));
                }
            }
        }
    }
    any.then_some(cmd)
}

fn join_ids<'a>(tracks: impl Iterator<Item = &'a TrackPlan>) -> String {
    tracks
        .map(|t| t.id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// mkvmerge-Remux nach `output`. Setzt Standard/Forced gleich mit.
///
/// Video, Kapitel, Anhänge und Tags bleiben erhalten (mkvmerge-Standard).
pub fn remux_command(plan: &FilePlan, output: &Path) -> ToolCommand {
    let mut cmd = ToolCommand::new(Program::Mkvmerge);
    cmd.arg("--gui-mode").arg("-o").arg(output);

    for (kind, select, none) in [
        (TrackType::Audio, "--audio-tracks", "--no-audio"),
        (TrackType::Subtitles, "--subtitle-tracks", "--no-subtitles"),
    ] {
        let all: Vec<&TrackPlan> = plan.tracks.iter().filter(|t| t.kind == kind).collect();
        let kept = all.iter().filter(|t| t.keep).count();
        if kept == all.len() {
            continue;
        }
        if kept == 0 {
            cmd.arg(none);
        } else {
            cmd.arg(select)
                .arg(join_ids(all.iter().copied().filter(|t| t.keep)));
        }
    }

    for t in plan
        .kept()
        .filter(|t| matches!(t.kind, TrackType::Audio | TrackType::Subtitles))
    {
        cmd.arg("--default-track-flag")
            .arg(format!("{}:{}", t.id, u8::from(t.after.default)));
        cmd.arg("--forced-display-flag")
            .arg(format!("{}:{}", t.id, u8::from(t.after.forced)));
    }

    let order: Vec<String> = plan.kept().map(|t| format!("0:{}", t.id)).collect();
    cmd.arg("--track-order").arg(order.join(","));
    cmd.arg(&plan.path);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;
    use crate::probe::parse_identification;
    use crate::selection::{DefaultChoice, Selection, TrackRef, TypeSelection, plan_file};

    fn plan(sel: Selection) -> FilePlan {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/json/full.json");
        let json = std::fs::read_to_string(p).expect("fixture");
        let mut m =
            parse_identification(&json, Path::new("/media/Serie X/Folge 1.mkv")).expect("parse");
        // Feste UIDs für exakte Argumentlisten.
        for (i, t) in m.tracks.iter_mut().enumerate() {
            t.uid = Some(1000 + i as u64);
        }
        plan_file(&m, &sel).expect("plan")
    }

    fn keep(refs: &[TrackRef]) -> Option<std::collections::BTreeSet<TrackRef>> {
        Some(refs.iter().cloned().collect())
    }

    #[test]
    fn propedit_exact_args() {
        let p = plan(Selection {
            audio: TypeSelection {
                default: DefaultChoice::Default(Some(TrackRef::Number(2))),
                ..Default::default()
            },
            subtitles: TypeSelection {
                default: DefaultChoice::DefaultForced(Some(TrackRef::Number(1))),
                ..Default::default()
            },
        });
        let cmd = propedit_command(&p, &p.path).expect("cmd");
        assert_eq!(cmd.program, Program::Mkvpropedit);
        assert_eq!(
            cmd.args_lossy(),
            vec![
                "/media/Serie X/Folge 1.mkv",
                "--edit",
                "track:=1001",
                "--set",
                "flag-default=0",
                "--edit",
                "track:=1002",
                "--set",
                "flag-default=1",
                "--edit",
                "track:=1004",
                "--set",
                "flag-default=1",
                "--set",
                "flag-forced=1",
                "--edit",
                "track:=1005",
                "--set",
                "flag-forced=0",
            ]
        );
        assert!(cmd.to_shell().starts_with(
            "mkvpropedit '/media/Serie X/Folge 1.mkv' --edit track:=1001 --set flag-default=0"
        ));
    }

    #[test]
    fn propedit_nothing_to_do() {
        let p = plan(Selection::default());
        assert!(propedit_command(&p, &p.path).is_none());
    }

    #[test]
    fn selector_fallbacks() {
        let mut p = plan(Selection::default());
        let t = &mut p.tracks[2];
        assert_eq!(propedit_selector(t), "track:=1002");
        t.uid = None;
        t.number = Some(3);
        assert_eq!(propedit_selector(t), "track:@3");
        t.number = None;
        assert_eq!(propedit_selector(t), "track:a2");
    }

    #[test]
    fn remux_exact_args() {
        let p = plan(Selection {
            audio: TypeSelection {
                keep: keep(&[TrackRef::Number(1), TrackRef::Lang(Lang::parse("jpn"))]),
                default: DefaultChoice::Default(Some(TrackRef::Number(1))),
            },
            subtitles: TypeSelection {
                keep: keep(&[TrackRef::Name("Signs".into())]),
                default: DefaultChoice::DefaultForced(None),
            },
        });
        let cmd = remux_command(&p, Path::new("/media/Serie X/Folge 1.trackwright.tmp.mkv"));
        assert_eq!(cmd.program, Program::Mkvmerge);
        assert_eq!(
            cmd.args_lossy(),
            vec![
                "--gui-mode",
                "-o",
                "/media/Serie X/Folge 1.trackwright.tmp.mkv",
                "--audio-tracks",
                "1,2",
                "--subtitle-tracks",
                "5",
                "--default-track-flag",
                "1:1",
                "--forced-display-flag",
                "1:0",
                "--default-track-flag",
                "2:0",
                "--forced-display-flag",
                "2:0",
                "--default-track-flag",
                "5:0",
                "--forced-display-flag",
                "5:0",
                "--track-order",
                "0:0,0:1,0:2,0:5",
                "/media/Serie X/Folge 1.mkv",
            ]
        );
    }

    #[test]
    fn remux_no_subtitles() {
        let p = plan(Selection {
            subtitles: TypeSelection {
                keep: keep(&[]),
                ..Default::default()
            },
            ..Default::default()
        });
        let args = remux_command(&p, Path::new("/o.mkv")).args_lossy();
        assert!(args.contains(&"--no-subtitles".to_string()), "{args:?}");
        assert!(!args.iter().any(|a| a == "--audio-tracks"), "{args:?}");
    }
}
