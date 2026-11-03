use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::appname;

const IDLE_SECS: u32 = 300;
const SAVE_EVERY: u32 = 30;

/// Segundos por programa, agrupados por dia (`AAAA-MM-DD`).
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Usage {
    pub days: BTreeMap<String, BTreeMap<String, u64>>,
}

impl Usage {
    /// Nomes antigos de jogos ("Jogo-Win64-Shipping") viram o nome legível, juntando os tempos.
    fn tidy(&mut self) {
        for apps in self.days.values_mut() {
            if !apps.keys().any(|k| k.contains("Shipping")) {
                continue;
            }
            let mut fixed: BTreeMap<String, u64> = BTreeMap::new();
            for (name, secs) in std::mem::take(apps) {
                let name = if name.contains("Shipping") { appname::prettify(&name) } else { name };
                *fixed.entry(name).or_default() += secs;
            }
            *apps = fixed;
        }
    }
}

/// Ícone RGBA do executável (`key` é o caminho do .exe, usado para cache de textura).
pub struct Icon {
    pub key: String,
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

#[derive(Default)]
struct Shared {
    usage: Usage,
    /// Segundos por programa desde que o app foi aberto.
    session: BTreeMap<String, u64>,
    current: Option<String>,
    idle: bool,
    /// Ícones por caminho do executável, e de qual executável vem cada nome exibido.
    icons: HashMap<String, Arc<Icon>>,
    icon_of: HashMap<String, String>,
}

impl Shared {
    fn icon(&self, name: &str) -> Option<Arc<Icon>> {
        self.icon_of.get(name).and_then(|p| self.icons.get(p)).cloned()
    }

    fn row(&self, name: &str, secs: u64) -> AppRow {
        AppRow { name: name.to_owned(), secs, icon: self.icon(name) }
    }
}

pub struct AppRow {
    pub name: String,
    pub secs: u64,
    pub icon: Option<Arc<Icon>>,
}

pub struct Now {
    pub current: Option<String>,
    pub idle: bool,
    pub icon: Option<Arc<Icon>>,
}

pub struct Report {
    pub now: Now,
    pub per_day: Vec<(NaiveDate, u64)>,
    pub apps: Vec<AppRow>,
}

pub struct WidgetData {
    pub now: Now,
    pub app_secs: u64,
    pub total: u64,
    pub session: Vec<AppRow>,
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

fn day_key(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

impl Tracker {
    pub fn start() -> Self {
        let mut usage: Usage =
            std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        usage.tidy();
        let shared = Arc::new(Mutex::new(Shared { usage, ..Default::default() }));
        let worker = Arc::clone(&shared);
        std::thread::spawn(move || {
            let me = std::env::current_exe()
                .ok()
                .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()));
            let mut ticks = 0u32;
            // Caches só desta thread: consultar ícone e nome do produto é caro e feito uma vez por executável.
            let mut icon_tried: HashSet<String> = HashSet::new();
            let mut products: HashMap<String, Option<String>> = HashMap::new();
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let idle = sys::idle_secs() >= IDLE_SECS;
                let fg = if idle { None } else { sys::foreground() };
                let name = fg.as_ref().map(|f| {
                    let product = appname::needs_product(&f.exe)
                        .then(|| products.entry(f.path.clone()).or_insert_with(|| appname::product_name(&f.path)).clone())
                        .flatten();
                    appname::display_name(&f.exe, &f.title, product.as_deref())
                });
                let new_icon = fg
                    .as_ref()
                    .filter(|f| icon_tried.insert(f.path.clone()))
                    .and_then(|f| sys::exe_icon(&f.path).map(|i| (f.path.clone(), Arc::new(i))));
                let mut s = worker.lock().unwrap();
                s.icons.extend(new_icon);
                s.idle = idle;
                s.current = name.clone();
                if let (Some(f), Some(name)) = (fg, name) {
                    s.icon_of.entry(name.clone()).or_insert_with(|| f.path.clone());
                    if Some(&f.exe) != me.as_ref() {
                        *s.session.entry(name.clone()).or_default() += 1;
                        *s.usage.days.entry(day_key(Local::now().date_naive())).or_default().entry(name).or_default() += 1;
                    }
                }
                ticks += 1;
                if ticks % SAVE_EVERY == 0 {
                    let usage = s.usage.clone();
                    drop(s);
                    save(&usage);
                }
            }
        });
        Self { shared }
    }

    pub fn save_now(&self) {
        save(&self.shared.lock().unwrap().usage);
    }

    fn now(s: &Shared) -> Now {
        let current = s.current.clone();
        let icon = current.as_deref().filter(|_| !s.idle).and_then(|c| s.icon(c));
        Now { current, idle: s.idle, icon }
    }

    /// Relatório dos últimos `days` dias; só o que a tela mostra é copiado (nada do histórico inteiro).
    pub fn report(&self, days: i64) -> Report {
        let s = self.shared.lock().unwrap();
        let today = Local::now().date_naive();
        let mut per_app: HashMap<&str, u64> = HashMap::new();
        let mut per_day = Vec::with_capacity(days as usize);
        for i in (0..days).rev() {
            let d = today - chrono::Duration::days(i);
            let mut total = 0;
            if let Some(apps) = s.usage.days.get(&day_key(d)) {
                for (a, secs) in apps {
                    *per_app.entry(a.as_str()).or_default() += secs;
                    total += secs;
                }
            }
            per_day.push((d, total));
        }
        let mut apps: Vec<AppRow> = per_app.into_iter().map(|(n, secs)| s.row(n, secs)).collect();
        apps.sort_by(|a, b| b.secs.cmp(&a.secs).then_with(|| a.name.cmp(&b.name)));
        Report { now: Self::now(&s), per_day, apps }
    }

    pub fn widget(&self) -> WidgetData {
        let s = self.shared.lock().unwrap();
        let today = s.usage.days.get(&day_key(Local::now().date_naive()));
        let app_secs = s.current.as_deref().and_then(|c| today.and_then(|t| t.get(c))).copied().unwrap_or(0);
        let total = today.map(|t| t.values().sum()).unwrap_or(0);
        let mut session: Vec<AppRow> = s.session.iter().map(|(n, secs)| s.row(n, *secs)).collect();
        session.sort_by(|a, b| b.secs.cmp(&a.secs));
        session.truncate(40);
        WidgetData { now: Self::now(&s), app_secs, total, session }
    }
}
pub(crate) use sys::version_names;

mod sys {
    use std::ffi::c_void;

    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Graphics::Gdi::{
        BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW,
        ReleaseDC,
    };
    use windows_sys::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
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
        /// Nome do executável sem extensão.
        pub exe: String,
        pub path: String,
        pub title: String,
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
            Some(Foreground { exe, path, title: window_title(hwnd) })
        }
    }

    /// Ícone grande (32x32) do executável.
    pub fn exe_icon(path: &str) -> Option<Icon> {
        let key = path.to_owned();
        unsafe {
            let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
            let mut large: HICON = std::ptr::null_mut();
            if ExtractIconExW(wide.as_ptr(), 0, &mut large, std::ptr::null_mut(), 1) == 0 || large.is_null() {
                return None;
            }
            let icon = icon_pixels(large, key);
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

    unsafe fn icon_pixels(icon: HICON, key: String) -> Option<Icon> {
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
                Some(Icon { key, w: w as usize, h: h as usize, rgba: px })
            })();
            DeleteObject(info.hbmColor);
            DeleteObject(info.hbmMask);
            result
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// Lê um campo de texto do recurso de versão do executável.
    unsafe fn version_string(block: &[u8], lang: u16, codepage: u16, field: &str) -> Option<String> {
        unsafe {
            let q = wide(&format!("\\StringFileInfo\\{lang:04x}{codepage:04x}\\{field}"));
            let mut p: *mut c_void = std::ptr::null_mut();
            let mut len = 0u32;
            if VerQueryValueW(block.as_ptr() as *const c_void, q.as_ptr(), &mut p, &mut len) == 0 || p.is_null() || len == 0 {
                return None;
            }
            let chars = std::slice::from_raw_parts(p as *const u16, len as usize);
            let text = String::from_utf16_lossy(chars);
            let text = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
            (!text.is_empty()).then(|| text.to_owned())
        }
    }

    /// (ProductName, FileDescription) do executável, se o recurso de versão existir.
    pub fn version_names(path: &str) -> Option<(Option<String>, Option<String>)> {
        unsafe {
            let w = wide(path);
            let size = GetFileVersionInfoSizeW(w.as_ptr(), std::ptr::null_mut());
            if size == 0 {
                return None;
            }
            let mut block = vec![0u8; size as usize];
            if GetFileVersionInfoW(w.as_ptr(), 0, size, block.as_mut_ptr() as *mut c_void) == 0 {
                return None;
            }
            let mut langs: Vec<(u16, u16)> = Vec::new();
            let q = wide("\\VarFileInfo\\Translation");
            let mut p: *mut c_void = std::ptr::null_mut();
            let mut len = 0u32;
            if VerQueryValueW(block.as_ptr() as *const c_void, q.as_ptr(), &mut p, &mut len) != 0 && !p.is_null() {
                let pairs = std::slice::from_raw_parts(p as *const u16, len as usize / 2);
                langs.extend(pairs.chunks_exact(2).map(|c| (c[0], c[1])));
            }
            langs.push((0x0409, 0x04B0));
            langs.push((0x0409, 0x04E4));
            for (lang, cp) in langs {
                let product = version_string(&block, lang, cp, "ProductName");
                let desc = version_string(&block, lang, cp, "FileDescription");
                if product.is_some() || desc.is_some() {
                    return Some((product, desc));
                }
            }
            None
        }
    }
}