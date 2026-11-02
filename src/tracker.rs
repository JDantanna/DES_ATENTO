use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

const IDLE_SECS: u32 = 300;
const SAVE_EVERY: u32 = 30;

/// Segundos por programa, agrupados por dia (`AAAA-MM-DD`).
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Usage {
    pub days: BTreeMap<String, BTreeMap<String, u64>>,
}

#[derive(Default)]
pub struct Shared {
    pub usage: Usage,
    pub current: Option<String>,
    pub idle: bool,
}

#[derive(Clone)]
pub struct Tracker {
    shared: Arc<Mutex<Shared>>,
}

fn path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("Des_Atento").join("usage.json")
}

fn save(usage: &Usage) {
    let p = path();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string(usage) {
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

impl Tracker {
    pub fn start() -> Self {
        let usage: Usage = std::fs::read_to_string(path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let shared = Arc::new(Mutex::new(Shared { usage, ..Default::default() }));
        let worker = Arc::clone(&shared);
        std::thread::spawn(move || {
            let me = std::env::current_exe()
                .ok()
                .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()));
            let mut ticks = 0u32;
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let idle = sys::idle_secs() >= IDLE_SECS;
                let app = if idle { None } else { sys::foreground_app() };
                let day = chrono::Local::now().format("%Y-%m-%d").to_string();
                let mut s = worker.lock().unwrap();
                s.idle = idle;
                s.current = app.clone();
                if let Some(name) = app.filter(|n| Some(n) != me.as_ref()) {
                    *s.usage.days.entry(day).or_default().entry(name).or_default() += 1;
                }
                ticks += 1;
                if ticks % SAVE_EVERY == 0 {
                    save(&s.usage);
                }
            }
        });
        Self { shared }
    }

    pub fn save_now(&self) {
        save(&self.shared.lock().unwrap().usage);
    }

    /// Copia o estado atual para desenhar sem segurar o lock.
    pub fn snapshot(&self) -> (Usage, Option<String>, bool) {
        let s = self.shared.lock().unwrap();
        (s.usage.clone(), s.current.clone(), s.idle)
    }
}

mod sys {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    pub fn idle_secs() -> u32 {
        unsafe {
            let mut info = LASTINPUTINFO { cbSize: size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
            if GetLastInputInfo(&mut info) == 0 {
                return 0;
            }
            GetTickCount().wrapping_sub(info.dwTime) / 1000
        }
    }

    /// Nome do executável (sem extensão) da janela em primeiro plano.
    pub fn foreground_app() -> Option<String> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 {
                return None;
            }
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut buf = [0u16; 520];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
            CloseHandle(h);
            if ok == 0 {
                return None;
            }
            let full = String::from_utf16_lossy(&buf[..len as usize]);
            let file = full.rsplit('\\').next()?;
            Some(file.strip_suffix(".exe").or_else(|| file.strip_suffix(".EXE")).unwrap_or(file).to_owned())
        }
    }
}
