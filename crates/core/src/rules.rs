//! Regelmodell und Kurzsyntax.
//!
//! Beispiele der Kurzsyntax (Groß-/Kleinschreibung der Schlüsselwörter egal, englische Aliase
//! erlaubt):
//!
//! ```text
//! Nr. 1 ODER Sprache in {ger, jpn}
//! Nr. 1,2 UND Name !~ /(?i)kommentar|commentary/
//! Nr. 2 ODER Name ~ /(?i)forced|signs|schilder/
//! Sprache = undefiniert UND NICHT Flag kommentar
//! Kanäle >= 6 UND Codec in {ac3, dts}
//! Nr. 2-4,letzte
//! ID 5
//! ```

use crate::lang::Lang;
use crate::model::{TrackInfo, TrackType};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// Fehler beim Parsen der Kurzsyntax.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Regel „{input}“: {message} (Zeichen {position})")]
pub struct ParseError {
    pub input: String,
    pub message: String,
    pub position: usize,
}

/// Element einer Nummernliste (Position innerhalb des Typs, 1-basiert).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumSpec {
    Index(u32),
    Range(u32, u32),
    Last,
}

impl NumSpec {
    fn matches(self, index: u32, count: u32) -> bool {
        match self {
            NumSpec::Index(n) => index == n,
            NumSpec::Range(a, b) => (a..=b).contains(&index),
            NumSpec::Last => index == count,
        }
    }

    /// Existiert die referenzierte Nummer bei `count` Spuren dieses Typs?
    fn exists(self, count: u32) -> bool {
        match self {
            NumSpec::Index(n) => n >= 1 && n <= count,
            NumSpec::Range(a, b) => a >= 1 && a <= b && b <= count,
            NumSpec::Last => count >= 1,
        }
    }
}

impl fmt::Display for NumSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NumSpec::Index(n) => write!(f, "{n}"),
            NumSpec::Range(a, b) => write!(f, "{a}-{b}"),
            NumSpec::Last => f.write_str("letzte"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl CmpOp {
    fn apply(self, a: u32, b: u32) -> bool {
        match self {
            CmpOp::Eq => a == b,
            CmpOp::Ne => a != b,
            CmpOp::Lt => a < b,
            CmpOp::Le => a <= b,
            CmpOp::Gt => a > b,
            CmpOp::Ge => a >= b,
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            CmpOp::Eq => "=",
            CmpOp::Ne => "!=",
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagKind {
    Default,
    Forced,
    HearingImpaired,
    Commentary,
}

impl FlagKind {
    fn keyword(self) -> &'static str {
        match self {
            FlagKind::Default => "default",
            FlagKind::Forced => "forced",
            FlagKind::HearingImpaired => "hörgeschädigt",
            FlagKind::Commentary => "kommentar",
        }
    }

    fn parse(word: &str) -> Option<FlagKind> {
        Some(match word.to_lowercase().as_str() {
            "default" | "standard" => FlagKind::Default,
            "forced" | "erzwungen" => FlagKind::Forced,
            "hi" | "hearing_impaired" | "hearing-impaired" | "hörgeschädigt"
            | "hoergeschaedigt" | "sdh" => FlagKind::HearingImpaired,
            "commentary" | "kommentar" => FlagKind::Commentary,
            _ => return None,
        })
    }
}

/// Regex für Spurnamen; Gleichheit über den Quelltext.
#[derive(Debug, Clone)]
pub struct NameRegex(pub Regex);

impl PartialEq for NameRegex {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

/// Ein einzelnes Kriterium.
#[derive(Debug, Clone, PartialEq)]
pub enum Criterion {
    /// Position im Typ: `Nr. 1,3-4,letzte`.
    Number(Vec<NumSpec>),
    /// Absolute mkvmerge-ID: `ID 5`.
    Id(Vec<u64>),
    Type(TrackType),
    /// Normalisierte Sprachen, [`Lang::Undefined`] ist ein eigener Wert.
    Language(Vec<Lang>),
    /// Teilstring (ohne Groß-/Kleinschreibung) von Codec-Name oder Codec-ID.
    Codec(Vec<String>),
    Channels(CmpOp, u32),
    Name(NameRegex),
    Flag(FlagKind),
}

/// Logischer Ausdruck über Kriterien.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Trifft jede Spur.
    All,
    Crit(Criterion),
    Not(Box<Expr>),
    And(Vec<Expr>),
    Or(Vec<Expr>),
}

/// Zustand einer Spur, gegen den ausgewertet wird (Sprache/Name nach „setzen“).
pub struct EvalTrack<'a> {
    pub track: &'a TrackInfo,
    pub lang: &'a Lang,
    pub name: Option<&'a str>,
    /// Anzahl der Spuren dieses Typs in der Datei.
    pub count: u32,
}

/// Eine referenzierte Spur fehlt in der Datei.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingRef {
    Number(NumSpec),
    Id(u64),
}

impl fmt::Display for MissingRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MissingRef::Number(n) => write!(f, "Nr. {n}"),
            MissingRef::Id(id) => write!(f, "ID {id}"),
        }
    }
}

impl Expr {
    pub fn crit(c: Criterion) -> Expr {
        Expr::Crit(c)
    }

    pub fn number(n: u32) -> Expr {
        Expr::Crit(Criterion::Number(vec![NumSpec::Index(n)]))
    }

    pub fn eval(&self, t: &EvalTrack<'_>) -> bool {
        match self {
            Expr::All => true,
            Expr::Crit(c) => eval_crit(c, t),
            Expr::Not(e) => !e.eval(t),
            Expr::And(v) => v.iter().all(|e| e.eval(t)),
            Expr::Or(v) => v.iter().any(|e| e.eval(t)),
        }
    }

    /// Sammelt referenzierte Nummern/IDs, die es nicht gibt.
    ///
    /// `count` ist die Spuranzahl des Typs, `ids` sind die IDs dieses Typs in der Datei.
    pub fn missing_refs(&self, count: u32, ids: &[u64], out: &mut Vec<MissingRef>) {
        match self {
            Expr::All => {}
            Expr::Crit(Criterion::Number(specs)) => {
                for s in specs {
                    if !s.exists(count) {
                        out.push(MissingRef::Number(*s));
                    }
                }
            }
            Expr::Crit(Criterion::Id(list)) => {
                for id in list {
                    if !ids.contains(id) {
                        out.push(MissingRef::Id(*id));
                    }
                }
            }
            Expr::Crit(_) => {}
            Expr::Not(e) => e.missing_refs(count, ids, out),
            Expr::And(v) | Expr::Or(v) => v.iter().for_each(|e| e.missing_refs(count, ids, out)),
        }
    }
}

fn eval_crit(c: &Criterion, t: &EvalTrack<'_>) -> bool {
    match c {
        Criterion::Number(specs) => specs.iter().any(|s| s.matches(t.track.type_index, t.count)),
        Criterion::Id(ids) => ids.contains(&t.track.id),
        Criterion::Type(k) => t.track.kind == *k,
        Criterion::Language(langs) => langs.contains(t.lang),
        Criterion::Codec(list) => {
            let codec = t.track.codec.to_lowercase();
            let id = t.track.codec_id.as_deref().unwrap_or("").to_lowercase();
            list.iter().any(|c| {
                let c = c.to_lowercase();
                codec.contains(&c) || id.contains(&c)
            })
        }
        Criterion::Channels(op, n) => t.track.channels.is_some_and(|ch| op.apply(ch, *n)),
        Criterion::Name(re) => re.0.is_match(t.name.unwrap_or("")),
        Criterion::Flag(kind) => match kind {
            FlagKind::Default => t.track.flags.default,
            FlagKind::Forced => t.track.flags.forced,
            FlagKind::HearingImpaired => t.track.flags.hearing_impaired,
            FlagKind::Commentary => t.track.flags.commentary,
        },
    }
}

// ---------------------------------------------------------------------------------------------
// Ausgabe (kanonische deutsche Kurzsyntax)

fn write_set<T: fmt::Display>(f: &mut fmt::Formatter<'_>, key: &str, items: &[T]) -> fmt::Result {
    if items.len() == 1 {
        write!(f, "{key} = {}", items[0])
    } else {
        write!(f, "{key} in {{")?;
        for (i, it) in items.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{it}")?;
        }
        f.write_str("}")
    }
}

fn write_regex(f: &mut fmt::Formatter<'_>, op: &str, re: &NameRegex) -> fmt::Result {
    write!(f, "Name {op} /{}/", re.0.as_str().replace('/', r"\/"))
}

impl fmt::Display for Criterion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Criterion::Number(specs) => {
                f.write_str("Nr. ")?;
                let s: Vec<String> = specs.iter().map(|s| s.to_string()).collect();
                f.write_str(&s.join(","))
            }
            Criterion::Id(ids) => {
                let s: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
                write!(f, "ID {}", s.join(","))
            }
            Criterion::Type(k) => write!(f, "Typ {}", k.short()),
            Criterion::Language(l) => write_set(f, "Sprache", l),
            Criterion::Codec(c) => write_set(f, "Codec", c),
            Criterion::Channels(op, n) => write!(f, "Kanäle {} {n}", op.symbol()),
            Criterion::Name(re) => write_regex(f, "~", re),
            Criterion::Flag(k) => write!(f, "Flag {}", k.keyword()),
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::All => f.write_str("alle"),
            Expr::Crit(c) => write!(f, "{c}"),
            Expr::Not(inner) => match inner.as_ref() {
                Expr::Crit(Criterion::Name(re)) => write_regex(f, "!~", re),
                e @ (Expr::And(_) | Expr::Or(_)) => write!(f, "NICHT ({e})"),
                e => write!(f, "NICHT {e}"),
            },
            Expr::And(v) => {
                for (i, e) in v.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" UND ")?;
                    }
                    match e {
                        Expr::Or(_) => write!(f, "({e})")?,
                        _ => write!(f, "{e}")?,
                    }
                }
                Ok(())
            }
            Expr::Or(v) => {
                for (i, e) in v.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ODER ")?;
                    }
                    write!(f, "{e}")?;
                }
                Ok(())
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Lexer

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String),
    Num(u64),
    Regex(String),
    Comma,
    Dash,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Tilde,
    NotTilde,
    Cmp(CmpOp),
    Bang,
    AndOp,
    OrOp,
    In,
}

struct Lexed {
    toks: Vec<(Tok, usize)>,
}

fn lex(input: &str) -> Result<Lexed, ParseError> {
    let err = |message: &str, position: usize| ParseError {
        input: input.to_string(),
        message: message.to_string(),
        position,
    };
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i].1;
        let next = chars.get(i + 1).map(|&(_, c)| c);
        let char_pos = i;
        match c {
            c if c.is_whitespace() => {
                i += 1;
                continue;
            }
            ',' => toks.push((Tok::Comma, char_pos)),
            '-' => toks.push((Tok::Dash, char_pos)),
            '(' => toks.push((Tok::LParen, char_pos)),
            ')' => toks.push((Tok::RParen, char_pos)),
            '{' => toks.push((Tok::LBrace, char_pos)),
            '}' => toks.push((Tok::RBrace, char_pos)),
            '∈' => toks.push((Tok::In, char_pos)),
            '~' => toks.push((Tok::Tilde, char_pos)),
            '#' => toks.push((Tok::Word("nr".into()), char_pos)),
            '*' => toks.push((Tok::Word("alle".into()), char_pos)),
            '!' if next == Some('~') => {
                toks.push((Tok::NotTilde, char_pos));
                i += 1;
            }
            '!' if next == Some('=') => {
                toks.push((Tok::Cmp(CmpOp::Ne), char_pos));
                i += 1;
            }
            '!' => toks.push((Tok::Bang, char_pos)),
            '=' => {
                if next == Some('=') {
                    i += 1;
                }
                toks.push((Tok::Cmp(CmpOp::Eq), char_pos));
            }
            '<' | '>' => {
                let eq = next == Some('=');
                let op = match (c, eq) {
                    ('<', false) => CmpOp::Lt,
                    ('<', true) => CmpOp::Le,
                    ('>', false) => CmpOp::Gt,
                    _ => CmpOp::Ge,
                };
                if eq {
                    i += 1;
                }
                toks.push((Tok::Cmp(op), char_pos));
            }
            '&' => {
                if next == Some('&') {
                    i += 1;
                }
                toks.push((Tok::AndOp, char_pos));
            }
            '|' => {
                if next == Some('|') {
                    i += 1;
                }
                toks.push((Tok::OrOp, char_pos));
            }
            '/' => {
                let mut re = String::new();
                let mut j = i + 1;
                let mut closed = false;
                while j < chars.len() {
                    let ch = chars[j].1;
                    if ch == '\\' && chars.get(j + 1).map(|&(_, c)| c) == Some('/') {
                        re.push('/');
                        j += 2;
                        continue;
                    }
                    if ch == '/' {
                        closed = true;
                        break;
                    }
                    re.push(ch);
                    j += 1;
                }
                if !closed {
                    return Err(err("Regex ohne schließendes /", char_pos));
                }
                toks.push((Tok::Regex(re), char_pos));
                i = j;
            }
            c if c.is_ascii_digit() => {
                let mut j = i;
                while j < chars.len() && chars[j].1.is_ascii_digit() {
                    j += 1;
                }
                let text: String = chars[i..j].iter().map(|&(_, c)| c).collect();
                let n = text
                    .parse::<u64>()
                    .map_err(|_| err("Zahl zu groß", char_pos))?;
                toks.push((Tok::Num(n), char_pos));
                i = j;
                continue;
            }
            c if c.is_alphabetic() || c == '_' => {
                let mut j = i;
                while j < chars.len() {
                    let ch = chars[j].1;
                    if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                        j += 1;
                    } else {
                        // Ein Punkt beendet das Wort („Nr.1“ → „Nr.“ + 1).
                        if ch == '.' {
                            j += 1;
                        }
                        break;
                    }
                }
                let word: String = chars[i..j].iter().map(|&(_, c)| c).collect();
                let tok = match word.to_lowercase().as_str() {
                    "und" | "and" => Tok::AndOp,
                    "oder" | "or" => Tok::OrOp,
                    "in" => Tok::In,
                    _ => Tok::Word(word),
                };
                // „und“ ist auch der Sprachcode für „undefiniert“: In Mengen/nach „=“ wird es
                // vom Parser als Sprache behandelt.
                toks.push((tok, char_pos));
                i = j;
                continue;
            }
            _ => {
                return Err(err(&format!("unerwartetes Zeichen „{c}“"), char_pos));
            }
        }
        i += 1;
    }
    Ok(Lexed { toks })
}

// ---------------------------------------------------------------------------------------------
// Parser

struct Parser<'a> {
    input: &'a str,
    toks: Vec<(Tok, usize)>,
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|(t, _)| t)
    }

    fn here(&self) -> usize {
        self.toks
            .get(self.pos)
            .map(|&(_, p)| p)
            .unwrap_or_else(|| self.input.chars().count())
    }

    fn bump(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).map(|(t, _)| t.clone());
        self.pos += 1;
        t
    }

    fn err<T>(&self, message: impl Into<String>) -> Result<T, ParseError> {
        Err(ParseError {
            input: self.input.to_string(),
            message: message.into(),
            position: self.here(),
        })
    }

    fn expect(&mut self, tok: Tok, what: &str) -> Result<(), ParseError> {
        if self.peek() == Some(&tok) {
            self.pos += 1;
            Ok(())
        } else {
            self.err(format!("{what} erwartet"))
        }
    }

    fn expr(&mut self) -> Result<Expr, ParseError> {
        let mut items = vec![self.and_expr()?];
        while self.peek() == Some(&Tok::OrOp) {
            self.pos += 1;
            items.push(self.and_expr()?);
        }
        Ok(if items.len() == 1 {
            items.remove(0)
        } else {
            Expr::Or(items)
        })
    }

    fn and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut items = vec![self.unary()?];
        while self.peek() == Some(&Tok::AndOp) {
            self.pos += 1;
            items.push(self.unary()?);
        }
        Ok(if items.len() == 1 {
            items.remove(0)
        } else {
            Expr::And(items)
        })
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Some(Tok::Bang) => {
                self.pos += 1;
                Ok(Expr::Not(Box::new(self.unary()?)))
            }
            Some(Tok::Word(w)) if matches!(w.to_lowercase().as_str(), "nicht" | "not") => {
                self.pos += 1;
                Ok(Expr::Not(Box::new(self.unary()?)))
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        match self.bump() {
            Some(Tok::LParen) => {
                let e = self.expr()?;
                self.expect(Tok::RParen, "„)“")?;
                Ok(e)
            }
            Some(Tok::Word(w)) => self.criterion(&w),
            Some(_) => {
                self.pos -= 1;
                self.err("Kriterium erwartet (Nr., ID, Typ, Sprache, Codec, Kanäle, Name, Flag)")
            }
            None => self.err("Ausdruck unvollständig"),
        }
    }

    fn criterion(&mut self, word: &str) -> Result<Expr, ParseError> {
        let key = word.to_lowercase();
        let key = key.trim_end_matches('.');
        match key {
            "alle" | "all" => Ok(Expr::All),
            "nr" | "nummer" | "no" | "number" => {
                Ok(Expr::Crit(Criterion::Number(self.num_list()?)))
            }
            "id" => {
                let mut ids = vec![self.num()?];
                while self.peek() == Some(&Tok::Comma) {
                    self.pos += 1;
                    ids.push(self.num()?);
                }
                Ok(Expr::Crit(Criterion::Id(ids)))
            }
            "typ" | "type" => {
                let w = self.word("Spurtyp")?;
                let kind = match w.to_lowercase().as_str() {
                    "audio" => TrackType::Audio,
                    "video" => TrackType::Video,
                    "untertitel" | "subtitles" | "subtitle" | "subs" => TrackType::Subtitles,
                    _ => return self.err(format!("unbekannter Spurtyp „{w}“")),
                };
                Ok(Expr::Crit(Criterion::Type(kind)))
            }
            "sprache" | "lang" | "language" => {
                let (negate, items) = self.set_operand()?;
                let langs = items
                    .iter()
                    .map(|s| match s.to_lowercase().as_str() {
                        "undefiniert" | "undefined" | "und" | "keine" | "none" => Lang::Undefined,
                        _ => Lang::parse(s),
                    })
                    .collect();
                Ok(wrap_not(negate, Expr::Crit(Criterion::Language(langs))))
            }
            "codec" => {
                let (negate, items) = self.set_operand()?;
                Ok(wrap_not(negate, Expr::Crit(Criterion::Codec(items))))
            }
            "kanäle" | "kanaele" | "kanal" | "channels" | "ch" => {
                let op = match self.bump() {
                    Some(Tok::Cmp(op)) => op,
                    _ => {
                        self.pos -= 1;
                        return self.err("Vergleich erwartet (=, !=, <, <=, >, >=)");
                    }
                };
                let n = self.num()?;
                let n = u32::try_from(n).or_else(|_| self.err("Zahl zu groß"))?;
                Ok(Expr::Crit(Criterion::Channels(op, n)))
            }
            "name" | "titel" => {
                let negate = match self.bump() {
                    Some(Tok::Tilde) => false,
                    Some(Tok::NotTilde) => true,
                    _ => {
                        self.pos -= 1;
                        return self.err("„~“ oder „!~“ erwartet");
                    }
                };
                let Some(Tok::Regex(src)) = self.bump() else {
                    self.pos -= 1;
                    return self.err("Regex in /…/ erwartet");
                };
                let re = Regex::new(&src).or_else(|e| self.err(format!("ungültige Regex: {e}")))?;
                Ok(wrap_not(negate, Expr::Crit(Criterion::Name(NameRegex(re)))))
            }
            "flag" => {
                let w = self.word("Flag-Name")?;
                match FlagKind::parse(&w) {
                    Some(k) => Ok(Expr::Crit(Criterion::Flag(k))),
                    None => self.err(format!(
                        "unbekanntes Flag „{w}“ (default, forced, hörgeschädigt, kommentar)"
                    )),
                }
            }
            _ => {
                self.pos -= 1;
                self.err(format!("unbekanntes Kriterium „{word}“"))
            }
        }
    }

    fn num(&mut self) -> Result<u64, ParseError> {
        match self.bump() {
            Some(Tok::Num(n)) => Ok(n),
            _ => {
                self.pos -= 1;
                self.err("Zahl erwartet")
            }
        }
    }

    fn word(&mut self, what: &str) -> Result<String, ParseError> {
        match self.bump() {
            Some(Tok::Word(w)) => Ok(w),
            // „und“ wird als Operator gelext, ist hier aber ein Wert.
            Some(Tok::AndOp) => Ok("und".into()),
            _ => {
                self.pos -= 1;
                self.err(format!("{what} erwartet"))
            }
        }
    }

    fn num_list(&mut self) -> Result<Vec<NumSpec>, ParseError> {
        let mut out = Vec::new();
        loop {
            match self.bump() {
                Some(Tok::Num(a)) => {
                    let a = u32::try_from(a).or_else(|_| self.err("Zahl zu groß"))?;
                    if self.peek() == Some(&Tok::Dash) {
                        self.pos += 1;
                        let b = self.num()?;
                        let b = u32::try_from(b).or_else(|_| self.err("Zahl zu groß"))?;
                        if b < a {
                            return self.err("Bereich rückwärts");
                        }
                        out.push(NumSpec::Range(a, b));
                    } else {
                        out.push(NumSpec::Index(a));
                    }
                }
                Some(Tok::Word(w)) if matches!(w.to_lowercase().as_str(), "letzte" | "last") => {
                    out.push(NumSpec::Last)
                }
                _ => {
                    self.pos -= 1;
                    return self.err("Nummer, Bereich oder „letzte“ erwartet");
                }
            }
            if self.peek() == Some(&Tok::Comma) {
                self.pos += 1;
            } else {
                break;
            }
        }
        Ok(out)
    }

    /// `= x`, `!= x`, `in {a, b}`, `∈ {a, b}`, `!= {a,b}`.
    fn set_operand(&mut self) -> Result<(bool, Vec<String>), ParseError> {
        let negate = match self.bump() {
            Some(Tok::Cmp(CmpOp::Eq)) | Some(Tok::In) => false,
            Some(Tok::Cmp(CmpOp::Ne)) => true,
            _ => {
                self.pos -= 1;
                return self.err("„=“, „!=“ oder „in“ erwartet");
            }
        };
        if self.peek() == Some(&Tok::LBrace) {
            self.pos += 1;
            let mut items = vec![self.word("Wert")?];
            while self.peek() == Some(&Tok::Comma) {
                self.pos += 1;
                items.push(self.word("Wert")?);
            }
            self.expect(Tok::RBrace, "„}“")?;
            Ok((negate, items))
        } else {
            Ok((negate, vec![self.word("Wert")?]))
        }
    }
}

fn wrap_not(negate: bool, e: Expr) -> Expr {
    if negate { Expr::Not(Box::new(e)) } else { e }
}

impl FromStr for Expr {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Expr, ParseError> {
        let lexed = lex(input)?;
        if lexed.toks.is_empty() {
            return Err(ParseError {
                input: input.to_string(),
                message: "leerer Ausdruck".into(),
                position: 0,
            });
        }
        let mut p = Parser {
            input,
            toks: lexed.toks,
            pos: 0,
        };
        let e = p.expr()?;
        if p.pos < p.toks.len() {
            return p.err("unerwartete Fortsetzung");
        }
        Ok(e)
    }
}

impl Serialize for Expr {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Expr {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------------------------
// Regelsätze

/// Regel für ein Flag (Standard/Forced).
#[derive(Debug, Clone, Default, PartialEq)]
pub enum FlagRule {
    /// Flags bleiben wie sie sind.
    #[default]
    Unchanged,
    /// Flag bei allen Spuren des Typs entfernen („keine“).
    None,
    /// Flag bei Treffern setzen, bei allen übrigen Spuren des Typs entfernen.
    Match(Expr),
}

impl FlagRule {
    pub fn is_unchanged(&self) -> bool {
        matches!(self, FlagRule::Unchanged)
    }
}

impl fmt::Display for FlagRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FlagRule::Unchanged => f.write_str("unverändert"),
            FlagRule::None => f.write_str("keine"),
            FlagRule::Match(e) => write!(f, "{e}"),
        }
    }
}

impl FromStr for FlagRule {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, ParseError> {
        match s.trim().to_lowercase().as_str() {
            "" | "unverändert" | "unchanged" => Ok(FlagRule::Unchanged),
            "keine" | "none" => Ok(FlagRule::None),
            _ => Ok(FlagRule::Match(s.parse()?)),
        }
    }
}

impl Serialize for FlagRule {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for FlagRule {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// „Sprache setzen“: Treffer von `when` bekommen die Sprache `lang`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LangAssign {
    pub when: Expr,
    pub lang: String,
}

/// „Name setzen“: Treffer von `when` bekommen den Namen `name` (leer = Name entfernen).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NameAssign {
    pub when: Expr,
    pub name: String,
}

/// Regeln für einen Spurtyp.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TypeRules {
    /// Welche Spuren bleiben. `None` = alle behalten.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep: Option<Expr>,
    /// Standard-Flag. Bei mehreren Treffern bekommt nur die erste behaltene Spur das Flag.
    #[serde(skip_serializing_if = "FlagRule::is_unchanged")]
    pub default: FlagRule,
    /// Forced-Flag. Alle Treffer bekommen das Flag.
    #[serde(skip_serializing_if = "FlagRule::is_unchanged")]
    pub forced: FlagRule,
    /// Wird vor allen anderen Regeln angewendet; bei mehreren Treffern gewinnt die letzte.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub set_language: Vec<LangAssign>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub set_name: Vec<NameAssign>,
}

impl TypeRules {
    pub fn is_empty(&self) -> bool {
        *self == TypeRules::default()
    }

    /// Alle Ausdrücke dieses Regelsatzes (für die Prüfung fehlender Nummern).
    pub fn exprs(&self) -> Vec<&Expr> {
        let mut v: Vec<&Expr> = Vec::new();
        v.extend(self.keep.iter());
        if let FlagRule::Match(e) = &self.default {
            v.push(e);
        }
        if let FlagRule::Match(e) = &self.forced {
            v.push(e);
        }
        v.extend(self.set_language.iter().map(|a| &a.when));
        v.extend(self.set_name.iter().map(|a| &a.when));
        v
    }
}

/// Vollständiger Regelsatz.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuleSet {
    #[serde(skip_serializing_if = "TypeRules::is_empty")]
    pub audio: TypeRules,
    #[serde(skip_serializing_if = "TypeRules::is_empty", alias = "untertitel")]
    pub subtitles: TypeRules,
}

impl RuleSet {
    pub fn for_type(&self, kind: TrackType) -> Option<&TypeRules> {
        match kind {
            TrackType::Audio => Some(&self.audio),
            TrackType::Subtitles => Some(&self.subtitles),
            _ => None,
        }
    }

    pub fn for_type_mut(&mut self, kind: TrackType) -> Option<&mut TypeRules> {
        match kind {
            TrackType::Audio => Some(&mut self.audio),
            TrackType::Subtitles => Some(&mut self.subtitles),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Expr {
        s.parse().unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn parses_handover_examples() {
        let e = p("Nr. 1 ODER Sprache ∈ {ger, jpn}");
        assert_eq!(
            e,
            Expr::Or(vec![
                Expr::number(1),
                Expr::Crit(Criterion::Language(vec![
                    Lang::Code("ger".into()),
                    Lang::Code("jpn".into())
                ])),
            ])
        );
        let e = p("Nr. 1,2  UND  Name !~ /(?i)kommentar|commentary/");
        let Expr::And(items) = &e else {
            panic!("{e:?}")
        };
        assert_eq!(
            items[0],
            Expr::Crit(Criterion::Number(vec![
                NumSpec::Index(1),
                NumSpec::Index(2)
            ]))
        );
        assert!(
            matches!(&items[1], Expr::Not(inner) if matches!(**inner, Expr::Crit(Criterion::Name(_))))
        );
        p("Nr. 2  ODER  Name ~ /(?i)forced|signs|schilder/");
    }

    #[test]
    fn precedence_and_not() {
        // NICHT > UND > ODER
        let e = p("Nr. 1 ODER Nr. 2 UND NICHT Flag forced");
        let Expr::Or(v) = e else { panic!() };
        assert_eq!(v.len(), 2);
        assert!(matches!(&v[1], Expr::And(a) if matches!(a[1], Expr::Not(_))));
        let e = p("(Nr. 1 OR #2) and !Flag kommentar");
        assert!(matches!(e, Expr::And(_)));
    }

    #[test]
    fn number_lists() {
        assert_eq!(
            p("Nr. 1,3-4,letzte"),
            Expr::Crit(Criterion::Number(vec![
                NumSpec::Index(1),
                NumSpec::Range(3, 4),
                NumSpec::Last
            ]))
        );
        assert!("Nr. 4-2".parse::<Expr>().is_err());
        assert_eq!(p("ID 5,7"), Expr::Crit(Criterion::Id(vec![5, 7])));
        assert_eq!(p("Nr.1"), Expr::number(1));
    }

    #[test]
    fn language_values() {
        assert_eq!(
            p("Sprache = und"),
            Expr::Crit(Criterion::Language(vec![Lang::Undefined]))
        );
        assert_eq!(
            p("lang in {undefiniert, deu}"),
            Expr::Crit(Criterion::Language(vec![
                Lang::Undefined,
                Lang::Code("ger".into())
            ]))
        );
        assert!(matches!(p("Sprache != ger"), Expr::Not(_)));
    }

    #[test]
    fn misc_criteria() {
        assert_eq!(
            p("Kanäle >= 6"),
            Expr::Crit(Criterion::Channels(CmpOp::Ge, 6))
        );
        assert_eq!(
            p("Codec in {ac3, dts}"),
            Expr::Crit(Criterion::Codec(vec!["ac3".into(), "dts".into()]))
        );
        assert_eq!(
            p("Flag hi"),
            Expr::Crit(Criterion::Flag(FlagKind::HearingImpaired))
        );
        assert_eq!(
            p("Typ untertitel"),
            Expr::Crit(Criterion::Type(TrackType::Subtitles))
        );
        assert_eq!(p("alle"), Expr::All);
    }

    #[test]
    fn errors_have_positions() {
        for bad in [
            "",
            "Nr.",
            "Sprache ger",
            "Name ~ /(/",
            "Name ~ /abc",
            "Foo 1",
            "Nr. 1 ODER",
            "Nr. 1)",
            "Kanäle 6",
            "Flag xyz",
            "Nr. 1 $",
        ] {
            let err = bad.parse::<Expr>().expect_err(bad);
            assert!(err.position <= bad.chars().count(), "{bad}: {err}");
        }
    }

    #[test]
    fn display_roundtrip() {
        for src in [
            "Nr. 1 ODER Sprache in {ger, jpn}",
            "Nr. 1,2 UND Name !~ /(?i)kommentar|commentary/",
            "(Nr. 1 ODER Nr. 2) UND NICHT Flag forced",
            "NICHT (Sprache = undefiniert ODER Kanäle < 2)",
            "Name ~ /a\\/b/",
            "ID 3 UND Codec = aac UND Typ audio",
            "Nr. 2-4,letzte",
            "alle",
        ] {
            let e = p(src);
            let shown = e.to_string();
            assert_eq!(p(&shown), e, "{src} -> {shown}");
        }
        assert_eq!(
            p("nr 1 or lang = de").to_string(),
            "Nr. 1 ODER Sprache = ger"
        );
    }

    #[test]
    fn regex_slash_escape() {
        let Expr::Crit(Criterion::Name(re)) = p(r"Name ~ /a\/b/") else {
            panic!()
        };
        assert!(re.0.is_match("a/b"));
    }

    #[test]
    fn flag_rule_parsing() {
        assert_eq!("keine".parse::<FlagRule>(), Ok(FlagRule::None));
        assert_eq!("".parse::<FlagRule>(), Ok(FlagRule::Unchanged));
        assert_eq!(
            "Nr. 1".parse::<FlagRule>(),
            Ok(FlagRule::Match(Expr::number(1)))
        );
    }

    #[test]
    fn missing_refs() {
        let e = p("Nr. 1 ODER Nr. 3 ODER Nr. 2-4 ODER Nr. letzte ODER ID 9");
        let mut out = Vec::new();
        e.missing_refs(2, &[1, 2], &mut out);
        assert_eq!(
            out,
            vec![
                MissingRef::Number(NumSpec::Index(3)),
                MissingRef::Number(NumSpec::Range(2, 4)),
                MissingRef::Id(9)
            ]
        );
        let mut out = Vec::new();
        p("Nr. letzte").missing_refs(0, &[], &mut out);
        assert_eq!(out, vec![MissingRef::Number(NumSpec::Last)]);
    }
}
