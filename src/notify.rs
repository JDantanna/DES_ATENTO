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
