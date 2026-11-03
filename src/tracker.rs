use std::collections::{BTreeMap, HashMap};
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
    /// Segundos por programa desde que o app foi aberto.
    pub session: BTreeMap<String, u64>,
    pub current: Option<String>,
    pub idle: bool,
    pub icons: HashMap<String, Arc<Icon>>,
}

/// Ícone RGBA do executável.
pub struct Icon {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

pub struct Snapshot {
    pub usage: Usage,
    pub session: BTreeMap<String, u64>,
    pub current: Option<String>,
    pub idle: bool,
    pub icons: HashMap<String, Arc<Icon>>,
}

#[derive(Clone)]
pub struct Tracker {
    shared: Arc<Mutex<Shared>>,
}

fn path() -> PathBuf {
    crate::model::data_dir().join("usage.json")
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
                let fg = if idle { None } else { sys::foreground() };
                let app = fg.as_ref().map(|f| f.name.clone());
                let day = chrono::Local::now().format("%Y-%m-%d").to_string();
                let mut s = worker.lock().unwrap();
                s.idle = idle;
                s.current = app;
                if let Some(f) = fg.filter(|f| Some(&f.exe) != me.as_ref()) {
                    let name = f.name;
                    if !s.icons.contains_key(&name) {
                        if let Some(icon) = sys::exe_icon(&f.path) {
                            s.icons.insert(name.clone(), Arc::new(icon));
                        }
                    }
                    *s.session.entry(name.clone()).or_default() += 1;
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
    pub fn snapshot(&self) -> Snapshot {
        let s = self.shared.lock().unwrap();
        Snapshot {
            usage: s.usage.clone(),
            session: s.session.clone(),
            current: s.current.clone(),
            idle: s.idle,
            icons: s.icons.clone(),
        }
    }
}

mod sys {
    use std::ffi::c_void;

    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Graphics::Gdi::{
        BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW,
        ReleaseDC,
    };
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows_sys::Win32::UI::Shell::ExtractIconExW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DestroyIcon, GetForegroundWindow, GetIconInfo, GetWindowTextW, GetWindowThreadProcessId, HICON, ICONINFO,
    };

    use super::Icon;

    pub struct Foreground {
        /// Nome mostrado (programa, ou site quando for um navegador).
        pub name: String,
        /// Nome do executável sem extensão.
        pub exe: String,
        pub path: String,
    }

    pub fn idle_secs() -> u32 {
        unsafe {
            let mut info = LASTINPUTINFO { cbSize: size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
            if GetLastInputInfo(&mut info) == 0 {
                return 0;
            }
            GetTickCount().wrapping_sub(info.dwTime) / 1000
        }
    }

    fn window_title(hwnd: *mut c_void) -> String {
        let mut buf = [0u16; 512];
        let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    /// Janela em primeiro plano: executável, caminho e nome de exibição.
    pub fn foreground() -> Option<Foreground> {
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
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            let file = path.rsplit('\\').next()?;
            let exe = file.rsplit_once('.').map_or(file, |(stem, _)| stem).to_owned();
            let name = crate::appname::display_name(&exe, &window_title(hwnd));
            Some(Foreground { name, exe, path })
        }
    }

    /// Ícone grande (32x32) do executável.
    pub fn exe_icon(path: &str) -> Option<Icon> {
        unsafe {
            let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
            let mut large: HICON = std::ptr::null_mut();
            if ExtractIconExW(wide.as_ptr(), 0, &mut large, std::ptr::null_mut(), 1) == 0 || large.is_null() {
                return None;
            }
            let icon = icon_pixels(large);
            DestroyIcon(large);
            icon
        }
    }

    unsafe fn bitmap_bgra(bmp: *mut c_void, w: i32, h: i32) -> Option<Vec<u8>> {
        unsafe {
            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let mut buf = vec![0u8; (w * h * 4) as usize];
            let dc = GetDC(std::ptr::null_mut());
            let lines = GetDIBits(dc, bmp, 0, h as u32, buf.as_mut_ptr() as *mut c_void, &mut bi, DIB_RGB_COLORS);
            ReleaseDC(std::ptr::null_mut(), dc);
            (lines != 0).then_some(buf)
        }
    }

    unsafe fn icon_pixels(icon: HICON) -> Option<Icon> {
        unsafe {
            let mut info: ICONINFO = std::mem::zeroed();
            if GetIconInfo(icon, &mut info) == 0 {
                return None;
            }
            let result = (|| {
                if info.hbmColor.is_null() {
                    return None;
                }
                let mut bm: BITMAP = std::mem::zeroed();
                GetObjectW(info.hbmColor, size_of::<BITMAP>() as i32, &mut bm as *mut _ as *mut c_void);
                let (w, h) = (bm.bmWidth, bm.bmHeight);
                if w <= 0 || h <= 0 {
                    return None;
                }
                let mut px = bitmap_bgra(info.hbmColor, w, h)?;
                // Ícones antigos não têm canal alfa: usa a máscara.
                if px.chunks_exact(4).all(|p| p[3] == 0) {
                    let mask = bitmap_bgra(info.hbmMask, w, h)?;
                    for (p, m) in px.chunks_exact_mut(4).zip(mask.chunks_exact(4)) {
                        p[3] = if m[0] == 0 { 255 } else { 0 };
                    }
                }
                for p in px.chunks_exact_mut(4) {
                    p.swap(0, 2);
                }
                Some(Icon { w: w as usize, h: h as usize, rgba: px })
            })();
            DeleteObject(info.hbmColor);
            DeleteObject(info.hbmMask);
            result
        }
    }
}