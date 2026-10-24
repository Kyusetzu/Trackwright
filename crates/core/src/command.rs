//! Befehlsbau: exakte Argumentlisten für mkvpropedit und mkvmerge.

use crate::lang::Lang;
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

fn set_language(cmd: &mut ToolCommand, lang: &Lang) {
    cmd.arg("--set")
        .arg(format!("language={}", lang.iso639_2()))
        .arg("--set")
        .arg(format!("language-ietf={}", lang.ietf()));
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
                Change::Language { to, .. } => set_language(&mut cmd, &to),
                Change::Name { to: Some(name), .. } => {
                    cmd.arg("--set").arg(format!("name={name}"));
                }
                Change::Name { to: None, .. } => {
                    cmd.arg("--delete").arg("name");
                }
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

/// mkvmerge-Remux nach `output`. Setzt Flags, Sprache und Name gleich mit.
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
        if t.after.lang != t.before.lang {
            cmd.arg("--language")
                .arg(format!("{}:{}", t.id, t.after.lang.ietf()));
        }
        if t.after.name != t.before.name {
            cmd.arg("--track-name").arg(format!(
                "{}:{}",
                t.id,
                t.after.name.as_deref().unwrap_or("")
            ));
        }
    }

    let order: Vec<String> = plan.kept().map(|t| format!("0:{}", t.id)).collect();
    cmd.arg("--track-order").arg(order.join(","));
    cmd.arg(&plan.path);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{PlanOptions, plan_file};
    use crate::probe::parse_identification;
    use crate::rules::{LangAssign, NameAssign, RuleSet, TypeRules};

    fn plan(rules: RuleSet) -> FilePlan {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/json/full.json");
        let json = std::fs::read_to_string(p).expect("fixture");
        let mut m =
            parse_identification(&json, Path::new("/media/Serie X/Folge 1.mkv")).expect("parse");
        // Feste UIDs für exakte Argumentlisten.
        for (i, t) in m.tracks.iter_mut().enumerate() {
            t.uid = Some(1000 + i as u64);
        }
        plan_file(&m, &rules, None, &PlanOptions::default()).expect("plan")
    }

    fn tr(keep: Option<&str>, default: &str, forced: &str) -> TypeRules {
        TypeRules {
            keep: keep.map(|k| k.parse().expect("keep")),
            default: default.parse().expect("default"),
            forced: forced.parse().expect("forced"),
            ..Default::default()
        }
    }

    #[test]
    fn propedit_exact_args() {
        let p = plan(RuleSet {
            audio: tr(None, "Nr. 2", ""),
            subtitles: tr(None, "", "keine"),
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
                "track:=1005",
                "--set",
                "flag-forced=0",
            ]
        );
        assert_eq!(
            cmd.to_shell(),
            "mkvpropedit '/media/Serie X/Folge 1.mkv' --edit track:=1001 --set flag-default=0 \
             --edit track:=1002 --set flag-default=1 --edit track:=1005 --set flag-forced=0"
        );
    }

    #[test]
    fn propedit_language_and_name() {
        let mut audio = TypeRules::default();
        audio.set_language.push(LangAssign {
            when: "Nr. 3".parse().expect("expr"),
            lang: "und".into(),
        });
        audio.set_name.push(NameAssign {
            when: "Nr. 2".parse().expect("expr"),
            name: "Japanisch [Stereo]".into(),
        });
        audio.set_name.push(NameAssign {
            when: "Nr. 3".parse().expect("expr"),
            name: String::new(),
        });
        let p = plan(RuleSet {
            audio,
            ..Default::default()
        });
        let cmd = propedit_command(&p, Path::new("/x.mkv")).expect("cmd");
        assert_eq!(
            cmd.args_lossy(),
            vec![
                "/x.mkv",
                "--edit",
                "track:=1002",
                "--set",
                "name=Japanisch [Stereo]",
                "--edit",
                "track:=1003",
                "--set",
                "language=und",
                "--set",
                "language-ietf=und",
                "--delete",
                "name",
            ]
        );
    }

    #[test]
    fn propedit_nothing_to_do() {
        let p = plan(RuleSet::default());
        assert!(propedit_command(&p, &p.path).is_none());
    }

    #[test]
    fn selector_fallbacks() {
        let mut p = plan(RuleSet::default());
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
        let p = plan(RuleSet {
            audio: tr(Some("Nr. 1,2"), "Nr. 1", ""),
            subtitles: tr(Some("Nr. 2"), "keine", "Nr. 2"),
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
                "5:1",
                "--track-order",
                "0:0,0:1,0:2,0:5",
                "/media/Serie X/Folge 1.mkv",
            ]
        );
    }

    #[test]
    fn remux_no_subtitles_language_and_name() {
        let mut audio = tr(Some("Nr. 1"), "", "");
        audio.set_language.push(LangAssign {
            when: "Nr. 1".parse().expect("expr"),
            lang: "jpn".into(),
        });
        audio.set_name.push(NameAssign {
            when: "Nr. 1".parse().expect("expr"),
            name: String::new(),
        });
        let p = plan(RuleSet {
            audio,
            subtitles: tr(Some("Sprache = fre"), "", ""),
        });
        let args = remux_command(&p, Path::new("/o.mkv")).args_lossy();
        assert_eq!(
            args,
            vec![
                "--gui-mode",
                "-o",
                "/o.mkv",
                "--audio-tracks",
                "1",
                "--no-subtitles",
                "--default-track-flag",
                "1:1",
                "--forced-display-flag",
                "1:0",
                "--language",
                "1:ja",
                "--track-name",
                "1:",
                "--track-order",
                "0:0,0:1",
                "/media/Serie X/Folge 1.mkv",
            ]
        );
    }
}
