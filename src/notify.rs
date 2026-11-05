//! Avisos de tarefas como notificações do Windows.
//!
//! Uma thread lê `tasks.json` a cada 20 s (o app salva a cada mudança), então
//! os avisos funcionam mesmo com a janela minimizada.

use std::collections::HashSet;
use std::process::Command;
use std::time::Duration;

use chrono::{Duration as Dur, Local, NaiveDateTime, NaiveTime};

use crate::model::{Store, Task};

/// Janela em que um aviso ainda é disparado depois da hora (cobre suspensão/atrasos).
const GRACE_MIN: i64 = 3;
/// Hora do aviso do dia anterior para tarefas sem hora marcada.
const DAY_BEFORE_HOUR: u32 = 9;

/// Identidade do app nas notificações: registra um AppUserModelID próprio ("Navi" + ícone),
/// em vez de aparecer como "Windows PowerShell".
const TOAST: &str = r#"
$id = 'Navi.App'
$k = "HKCU:\Software\Classes\AppUserModelId\$id"
New-Item $k -Force | Out-Null
New-ItemProperty $k -Name DisplayName -Value 'Navi' -Force | Out-Null
New-ItemProperty $k -Name IconUri -Value $env:DA_ICON -Force | Out-Null
$t = [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime]
$x = $t::GetTemplateContent('ToastImageAndText02')
$x.GetElementsByTagName('image').Item(0).SetAttribute('src', $env:DA_ICON)
$n = $x.GetElementsByTagName('text')
$n.Item(0).AppendChild($x.CreateTextNode($env:DA_TITLE)) | Out-Null
$n.Item(1).AppendChild($x.CreateTextNode($env:DA_BODY)) | Out-Null
$au = $x.CreateElement('audio'); $au.SetAttribute('silent','true'); $x.DocumentElement.AppendChild($au) | Out-Null
$toast = [Windows.UI.Notifications.ToastNotification, Windows.UI.Notifications, ContentType = WindowsRuntime]::new($x)
$t::CreateToastNotifier($id).Show($toast)
"#;

/// Grava o ícone do aviso em disco (o Windows só lê imagens de arquivo).
fn icon_path() -> std::path::PathBuf {
    let path = crate::model::data_dir().join("toast.png");
    let bytes = include_bytes!("../assets/toast.png");
    if std::fs::metadata(&path).map(|m| m.len() != bytes.len() as u64).unwrap_or(true) {
        let _ = std::fs::create_dir_all(crate::model::data_dir());
        let _ = std::fs::write(&path, bytes);
    }
    path
}

#[cfg(windows)]
pub fn toast(title: &str, body: &str) {
    use std::os::windows::process::CommandExt;
    let _ = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", TOAST])
        .env("DA_TITLE", title)
        .env("DA_BODY", body)
        .env("DA_ICON", icon_path())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn();
}

/// Avisos programados de uma tarefa: quando disparar e o texto.
fn notices(t: &Task) -> Vec<(NaiveDateTime, String)> {
    let mut out = Vec::new();
    let Some(date) = t.date else { return out };
    if let (Some(time), Some(min)) = (t.time, t.notify) {
        let body = if min == 0 {
            format!("Agora, às {}", time.format("%H:%M"))
        } else {
            format!("Em {min} min, às {}", time.format("%H:%M"))
        };
        out.push((date.and_time(time) - Dur::minutes(min as i64), body));
    }
    if t.notify_day_before {
        let at = t.time.or(NaiveTime::from_hms_opt(DAY_BEFORE_HOUR, 0, 0));
        if let Some(at) = at {
            let body = match t.time {
                Some(h) => format!("Amanhã, às {}", h.format("%H:%M")),
                None => "Amanhã".to_owned(),
            };
            out.push(((date - Dur::days(1)).and_time(at), body));
        }
    }
    out
}

pub fn start() {
    std::thread::spawn(|| {
        let mut fired: HashSet<(u64, i64)> = HashSet::new();
        let mut store = Store::load();
        let mut stamp = Store::modified();
        loop {
            let now = Local::now().naive_local();
            // Só relê o arquivo quando o app salvou algo novo.
            if Store::modified() != stamp {
                stamp = Store::modified();
                store = Store::load();
            }
            for t in store.tasks.iter().filter(|t| !t.done) {
                for (due, body) in notices(t) {
                    if now >= due
                        && now < due + Dur::minutes(GRACE_MIN)
                        && fired.insert((t.id, due.and_utc().timestamp()))
                    {
                        toast(&t.title, &body);
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(20));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn aviso_no_dia_anterior() {
        let mut s = Store::default();
        let id = s.add("Prova".into(), NaiveDate::from_ymd_opt(2026, 5, 10));
        let t = s.tasks.iter_mut().find(|t| t.id == id).unwrap();
        assert!(notices(t).is_empty());

        t.notify_day_before = true;
        let n = notices(t);
        assert_eq!(n[0].0, NaiveDate::from_ymd_opt(2026, 5, 9).unwrap().and_hms_opt(9, 0, 0).unwrap());
        assert_eq!(n[0].1, "Amanhã");

        // Com hora marcada, vale a mesma hora e o aviso normal continua existindo.
        t.time = NaiveTime::from_hms_opt(15, 30, 0);
        t.notify = Some(0);
        let n = notices(t);
        assert_eq!(n.len(), 2);
        assert_eq!(n[0].0, NaiveDate::from_ymd_opt(2026, 5, 10).unwrap().and_hms_opt(15, 30, 0).unwrap());
        assert_eq!(n[1].0, NaiveDate::from_ymd_opt(2026, 5, 9).unwrap().and_hms_opt(15, 30, 0).unwrap());
        assert_eq!(n[1].1, "Amanhã, às 15:30");
    }
}
