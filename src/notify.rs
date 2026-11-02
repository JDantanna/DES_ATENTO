//! Avisos de tarefas como notificações do Windows.
//!
//! Uma thread lê `tasks.json` a cada 20 s (o app salva a cada mudança), então
//! os avisos funcionam mesmo com a janela minimizada.

use std::collections::HashSet;
use std::process::Command;
use std::time::Duration;

use chrono::{Duration as Dur, Local};

use crate::model::Store;

/// Janela em que um aviso ainda é disparado depois da hora (cobre suspensão/atrasos).
const GRACE_MIN: i64 = 3;

const TOAST: &str = r#"
$t = [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime]
$x = $t::GetTemplateContent('ToastText02')
$n = $x.GetElementsByTagName('text')
$n.Item(0).AppendChild($x.CreateTextNode($env:DA_TITLE)) | Out-Null
$n.Item(1).AppendChild($x.CreateTextNode($env:DA_BODY)) | Out-Null
$app = '{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe'
$toast = [Windows.UI.Notifications.ToastNotification, Windows.UI.Notifications, ContentType = WindowsRuntime]::new($x)
$t::CreateToastNotifier($app).Show($toast)
"#;

#[cfg(windows)]
pub fn toast(title: &str, body: &str) {
    use std::os::windows::process::CommandExt;
    let _ = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", TOAST])
        .env("DA_TITLE", title)
        .env("DA_BODY", body)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn();
}

pub fn start() {
    std::thread::spawn(|| {
        let mut fired: HashSet<(u64, i64)> = HashSet::new();
        loop {
            let now = Local::now().naive_local();
            let store = Store::load();
            for t in store.tasks.iter().filter(|t| !t.done) {
                let (Some(date), Some(time), Some(min)) = (t.date, t.time, t.notify) else {
                    continue;
                };
                let due = date.and_time(time) - Dur::minutes(min as i64);
                if now >= due
                    && now < due + Dur::minutes(GRACE_MIN)
                    && fired.insert((t.id, due.and_utc().timestamp()))
                {
                    let body = if min == 0 {
                        format!("Agora, às {}", time.format("%H:%M"))
                    } else {
                        format!("Em {min} min, às {}", time.format("%H:%M"))
                    };
                    toast(&t.title, &body);
                }
            }
            std::thread::sleep(Duration::from_secs(20));
        }
    });
}
