//! Importação/exportação iCalendar (.ics): compatível com Google Calendar,
//! Outlook, Apple Calendar e a maioria dos apps de tarefas.

use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

use crate::model::Store;

pub struct Imported {
    pub title: String,
    pub notes: String,
    pub date: Option<NaiveDate>,
    pub time: Option<NaiveTime>,
    pub notify: Option<u32>,
    pub tags: Vec<String>,
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace("\r\n", "\\n").replace('\n', "\\n")
}

fn unesc(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn export(store: &Store) -> String {
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let mut o = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Des_Atento//PT\r\nCALSCALE:GREGORIAN\r\n");
    for t in store.tasks.iter().filter(|t| t.date.is_some()) {
        let d = t.date.unwrap();
        o.push_str("BEGIN:VEVENT\r\n");
        o.push_str(&format!("UID:des-atento-{}@local\r\nDTSTAMP:{stamp}\r\n", t.id));
        match t.time {
            Some(h) => {
                let start = d.and_time(h);
                let end = start + chrono::Duration::hours(1);
                o.push_str(&format!("DTSTART:{}\r\nDTEND:{}\r\n", start.format("%Y%m%dT%H%M%S"), end.format("%Y%m%dT%H%M%S")));
            }
            None => o.push_str(&format!("DTSTART;VALUE=DATE:{}\r\n", d.format("%Y%m%d"))),
        }
        o.push_str(&format!("SUMMARY:{}\r\n", esc(&t.title)));
        let mut desc = t.notes.clone();
        for s in &t.subtasks {
            if !desc.is_empty() {
                desc.push('\n');
            }
            desc.push_str(&format!("[{}] {}", if s.done { "x" } else { " " }, s.title));
        }
        if !desc.is_empty() {
            o.push_str(&format!("DESCRIPTION:{}\r\n", esc(&desc)));
        }
        if !t.tags.is_empty() {
            o.push_str(&format!("CATEGORIES:{}\r\n", t.tags.iter().map(|x| esc(x)).collect::<Vec<_>>().join(",")));
        }
        if t.done {
            o.push_str("STATUS:CONFIRMED\r\nX-DES-ATENTO-DONE:1\r\n");
        }
        if let (Some(m), Some(_)) = (t.notify, t.time) {
            o.push_str(&format!("BEGIN:VALARM\r\nACTION:DISPLAY\r\nDESCRIPTION:{}\r\nTRIGGER:-PT{m}M\r\nEND:VALARM\r\n", esc(&t.title)));
        }
        o.push_str("END:VEVENT\r\n");
    }
    o.push_str("END:VCALENDAR\r\n");
    o
}

fn parse_when(params: &str, value: &str) -> Option<(NaiveDate, Option<NaiveTime>)> {
    let v = value.trim();
    if params.contains("VALUE=DATE") && !params.contains("DATE-TIME") || v.len() == 8 {
        return NaiveDate::parse_from_str(v.get(..8)?, "%Y%m%d").ok().map(|d| (d, None));
    }
    let utc = v.ends_with('Z');
    let ndt = NaiveDateTime::parse_from_str(v.trim_end_matches('Z'), "%Y%m%dT%H%M%S").ok()?;
    let ndt = if utc {
        Utc.from_utc_datetime(&ndt).with_timezone(&Local).naive_local()
    } else {
        ndt
    };
    Some((ndt.date(), Some(ndt.time())))
}

fn parse_trigger(v: &str) -> Option<u32> {
    let v = v.trim().strip_prefix("-PT")?;
    let (n, unit) = v.split_at(v.len().checked_sub(1)?);
    let n: u32 = n.parse().ok()?;
    match unit {
        "M" => Some(n),
        "H" => Some(n * 60),
        _ => None,
    }
}

pub fn parse(text: &str) -> Vec<Imported> {
    // Desdobra linhas continuadas (começam com espaço/tab).
    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines() {
        if (raw.starts_with(' ') || raw.starts_with('\t')) && !lines.is_empty() {
            lines.last_mut().unwrap().push_str(&raw[1..]);
        } else {
            lines.push(raw.to_owned());
        }
    }

    let mut out = Vec::new();
    let mut cur: Option<Imported> = None;
    let mut in_alarm = false;
    for l in lines {
        let l = l.trim_end();
        match l {
            "BEGIN:VEVENT" | "BEGIN:VTODO" => {
                cur = Some(Imported { title: String::new(), notes: String::new(), date: None, time: None, notify: None, tags: vec![] });
            }
            "END:VEVENT" | "END:VTODO" => {
                if let Some(c) = cur.take().filter(|c| !c.title.trim().is_empty()) {
                    out.push(c);
                }
            }
            "BEGIN:VALARM" => in_alarm = true,
            "END:VALARM" => in_alarm = false,
            _ => {
                let Some(c) = cur.as_mut() else { continue };
                let Some((head, value)) = l.split_once(':') else { continue };
                let (name, params) = head.split_once(';').unwrap_or((head, ""));
                match name.to_ascii_uppercase().as_str() {
                    "SUMMARY" => c.title = unesc(value),
                    "DESCRIPTION" if !in_alarm => c.notes = unesc(value),
                    "DTSTART" | "DUE" if c.date.is_none() => {
                        if let Some((d, t)) = parse_when(params, value) {
                            c.date = Some(d);
                            c.time = t;
                        }
                    }
                    "CATEGORIES" => c.tags.extend(value.split(',').map(|s| unesc(s).trim().to_owned()).filter(|s| !s.is_empty())),
                    "TRIGGER" if in_alarm => c.notify = parse_trigger(value),
                    _ => {}
                }
            }
        }
    }
    out
}

/// Adiciona ao `store` os itens que ainda não existem (mesmo título, data e hora).
pub fn import_into(store: &mut Store, items: Vec<Imported>) -> usize {
    let mut added = 0;
    for it in items {
        if store.tasks.iter().any(|t| t.title == it.title && t.date == it.date && t.time == it.time) {
            continue;
        }
        let id = store.add(it.title, it.date);
        if let Some(t) = store.tasks.iter_mut().find(|t| t.id == id) {
            t.notes = it.notes;
            t.time = it.time;
            t.notify = it.notify;
            t.tags = it.tags;
        }
        added += 1;
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut s = Store::default();
        let d = NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
        let id = s.add("Reunião; com, vírgula".into(), Some(d));
        let t = s.tasks.iter_mut().find(|t| t.id == id).unwrap();
        t.time = NaiveTime::from_hms_opt(14, 30, 0);
        t.notify = Some(15);
        t.notes = "linha1\nlinha2".into();
        t.tags = vec!["trabalho".into()];
        let got = parse(&export(&s));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].title, "Reunião; com, vírgula");
        assert_eq!(got[0].date, Some(d));
        assert_eq!(got[0].time, NaiveTime::from_hms_opt(14, 30, 0));
        assert_eq!(got[0].notify, Some(15));
        assert_eq!(got[0].notes, "linha1\nlinha2");
        assert_eq!(got[0].tags, vec!["trabalho".to_string()]);
    }

    #[test]
    fn google_utc_and_allday() {
        let ics = "BEGIN:VEVENT\r\nSUMMARY:Dia todo\r\nDTSTART;VALUE=DATE:20261105\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:Com hora\r\nDTSTART:20261105T120000Z\r\nEND:VEVENT\r\n";
        let got = parse(ics);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].time, None);
        assert!(got[1].time.is_some());
    }
}
