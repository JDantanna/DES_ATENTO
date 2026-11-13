use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::appcache::{self, Cache};
use crate::appname;

mod engagement;
mod probe;

// `Evidence` e `State::label` são para a interface mostrar o motivo; o rastreador em si só usa `counting()`.
#[allow(unused_imports)]
pub use engagement::{Evidence, State};

const SAVE_EVERY: u32 = 30;
/// Com a entrada parada, os sinais passivos (tela cheia, áudio, controle) são lidos a cada tantos segundos.
const PASSIVE_EVERY_MS: u32 = 5_000;

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
    /// O tempo não está sendo contado (`!state.counting()`).
    idle: bool,
    /// Em uso, só aberto ou ninguém no computador.
    state: State,
    /// Ícones carregados, pela chave (caminho do executável, `site:dominio` ou `custom:...`).
    icons: HashMap<String, Arc<Icon>>,
    /// O que fica guardado em disco: nomes, apelidos e de onde vem o ícone de cada programa.
    cache: Cache,
    cache_dirty: bool,
    /// Nada mais é gravado em disco: os arquivos estão sendo substituídos por um backup.
    frozen: bool,
}

impl Shared {
    fn icon(&self, name: &str) -> Option<Arc<Icon>> {
        self.cache.icon_of.get(name).and_then(|p| self.icons.get(p)).cloned()
    }

    fn row(&self, name: &str, secs: u64) -> AppRow {
        AppRow { name: name.to_owned(), secs, icon: self.icon(name) }
    }

    /// Passa o programa a se chamar `to` para sempre: o histórico, a sessão e o ícone vão junto.
    /// Se já existe um programa com esse nome, os tempos se somam.
    fn rename(&mut self, from: &str, to: &str) {
        for v in self.cache.aliases.values_mut() {
            if v == from {
                *v = to.to_owned();
            }
        }
        self.cache.aliases.insert(from.to_owned(), to.to_owned());
        self.cache.aliases.retain(|k, v| k != v);
        for apps in self.usage.days.values_mut() {
            if let Some(secs) = apps.remove(from) {
                *apps.entry(to.to_owned()).or_default() += secs;
            }
        }
        if let Some(secs) = self.session.remove(from) {
            *self.session.entry(to.to_owned()).or_default() += secs;
        }
        if let Some(key) = self.cache.icon_of.remove(from) {
            self.cache.icon_of.entry(to.to_owned()).or_insert(key);
        }
        if let Some(prev) = self.cache.custom_icons.remove(from) {
            self.cache.custom_icons.insert(to.to_owned(), prev);
        }
        if self.current.as_deref() == Some(from) {
            self.current = Some(to.to_owned());
        }
        self.cache_dirty = true;
    }
}

pub struct AppRow {
    pub name: String,
    pub secs: u64,
    pub icon: Option<Arc<Icon>>,
}

pub struct Now {
    /// Programa na frente (`None` quando não há ninguém no computador).
    pub current: Option<String>,
    /// O tempo não está sendo contado: o mesmo que `!state.counting()`.
    pub idle: bool,
    pub icon: Option<Arc<Icon>>,
    /// Em uso (e por quê), só aberto, ou sem atividade. `state.label()` dá o texto para a tela.
    #[allow(dead_code)]
    pub state: State,
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
        let cache = Cache::load();
        // Ícones guardados em execuções anteriores: lidos do disco uma vez, sem consultar o sistema nem a internet.
        let saved_keys: HashSet<String> = cache.icon_of.values().chain(cache.custom_icons.values()).cloned().collect();
        let shared = Arc::new(Mutex::new(Shared { usage, cache, ..Default::default() }));
        let worker = Arc::clone(&shared);
        std::thread::spawn(move || {
            let me = std::env::current_exe()
                .ok()
                .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()));
            let saved: Vec<_> =
                saved_keys.into_iter().filter_map(|k| appcache::load_icon(&k).map(|i| (k, Arc::new(i)))).collect();
            worker.lock().unwrap().icons.extend(saved);

            let mut ticks = 0u32;
            // Cada consulta (nome do produto, ícone do exe, ícone do site) é feita uma única vez por programa.
            let mut icon_tried: HashSet<String> = HashSet::new();
            let mut names_tried: HashSet<String> = HashSet::new();
            let mut sites_tried: HashSet<&'static str> = HashSet::new();
            // Uso de verdade: entrada recente na janela da frente, ou tela cheia, áudio e controle.
            probe::init();
            let mut watch = engagement::Watch::default();
            let mut audio = probe::Audio::default();
            let mut pad = probe::Pad::default();
            let mut passive = engagement::Passive::default();
            // Quando e para qual janela os sinais passivos foram lidos pela última vez.
            let mut passive_at: Option<(u32, usize)> = None;
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let now = sys::tick();
                let last_input = sys::last_input_tick().unwrap_or(now);
                let fg = sys::foreground();
                // Os sinais passivos só são lidos com o teclado e o mouse parados, espaçados, e de novo
                // assim que outra janela vem para a frente (o som da anterior não vale para a nova).
                // Nos testes, o laço não sonda a máquina de quem os roda (as sondas têm testes próprios).
                match fg.as_ref().filter(|_| !cfg!(test) && engagement::wants_passive(now, last_input)) {
                    Some(f) => {
                        if passive_at.is_none_or(|(at, hwnd)| hwnd != f.hwnd || now.wrapping_sub(at) >= PASSIVE_EVERY_MS) {
                            passive = probe::passive(f.hwnd, &f.exe, &mut audio, &mut pad, now);
                            passive_at = Some((now, f.hwnd));
                        }
                    }
                    None => {
                        passive = engagement::Passive::default();
                        passive_at = None;
                    }
                }
                let state = watch.observe(now, fg.as_ref().map(|f| f.hwnd), last_input, passive);
                let site = fg.as_ref().and_then(|f| appname::site_domain(&f.exe, &f.title));
                let name = fg.as_ref().map(|f| {
                    let product = if appname::needs_product(&f.exe) {
                        let known = worker.lock().unwrap().cache.names.get(&f.path).cloned();
                        known.or_else(|| {
                            if !names_tried.insert(f.path.clone()) {
                                return None;
                            }
                            // Ler o recurso de versão é lento: fica fora da trava.
                            let p = appname::product_name(&f.path)?;
                            let mut s = worker.lock().unwrap();
                            s.cache.names.insert(f.path.clone(), p.clone());
                            s.cache_dirty = true;
                            Some(p)
                        })
                    } else {
                        None
                    };
                    let detected = appname::display_name(&f.exe, &f.title, product.as_deref());
                    // O nome que o usuário escolheu vale sempre.
                    let alias = worker.lock().unwrap().cache.aliases.get(&detected).cloned();
                    alias.unwrap_or(detected)
                });

                let mut fresh: Vec<(String, Arc<Icon>)> = Vec::new();
                if let Some(f) = fg.as_ref().filter(|f| icon_tried.insert(f.path.clone())) {
                    let icon = appcache::load_icon(&f.path).or_else(|| {
                        let icon = sys::exe_icon(&f.path)?;
                        appcache::save_icon(&icon);
                        Some(icon)
                    });
                    fresh.extend(icon.map(|i| (f.path.clone(), Arc::new(i))));
                }
                if let Some(domain) = site.filter(|d| sites_tried.insert(*d)) {
                    let key = format!("site:{domain}");
                    match appcache::load_icon(&key) {
                        Some(i) => fresh.push((key, Arc::new(i))),
                        // Primeira vez deste site: busca o ícone em segundo plano, uma vez, e guarda em disco.
                        None => {
                            let w = Arc::clone(&worker);
                            std::thread::spawn(move || {
                                if let Some(i) = appcache::fetch_site_icon(domain) {
                                    w.lock().unwrap().icons.insert(i.key.clone(), Arc::new(i));
                                }
                            });
                        }
                    }
                }

                let mut s = worker.lock().unwrap();
                s.icons.extend(fresh);
                s.state = state;
                s.idle = !state.counting();
                // Sem ninguém no computador, não há "programa atual"; só aberto, há (a tela diz que não conta).
                s.current = if state == State::Away { None } else { name.clone() };
                if let (Some(f), Some(name), true) = (fg, name, state.counting()) {
                    // Site com ícone próprio usa o dele; senão, o do programa (o navegador).
                    // O ícone escolhido pelo usuário não é substituído.
                    if !s.cache.custom_icons.contains_key(&name) {
                        let key = site
                            .map(|d| format!("site:{d}"))
                            .filter(|k| s.icons.contains_key(k))
                            .unwrap_or_else(|| f.path.clone());
                        if s.cache.icon_of.get(&name) != Some(&key) {
                            s.cache.icon_of.insert(name.clone(), key);
                            s.cache_dirty = true;
                        }
                    }
                    if Some(&f.exe) != me.as_ref() {
                        *s.session.entry(name.clone()).or_default() += 1;
                        *s.usage.days.entry(day_key(Local::now().date_naive())).or_default().entry(name).or_default() += 1;
                    }
                }
                ticks += 1;
                if ticks.is_multiple_of(SAVE_EVERY) && !s.frozen {
                    let usage = s.usage.clone();
                    let cache = std::mem::take(&mut s.cache_dirty).then(|| s.cache.clone());
                    drop(s);
                    save(&usage);
                    if let Some(cache) = cache {
                        cache.save();
                    }
                }
            }
        });
        Self { shared }
    }

    pub fn save_now(&self) {
        let s = self.shared.lock().unwrap();
        save(&s.usage);
        s.cache.save();
    }

    /// Para de gravar em disco: o app está fechando para restaurar um backup.
    pub fn freeze(&self) {
        self.shared.lock().unwrap().frozen = true;
    }

    /// Renomeia o programa para sempre e grava na hora.
    pub fn rename_app(&self, from: &str, to: &str) {
        let mut s = self.shared.lock().unwrap();
        s.rename(from, to);
        s.cache_dirty = false;
        save(&s.usage);
        s.cache.save();
    }

    pub fn has_custom_icon(&self, name: &str) -> bool {
        self.shared.lock().unwrap().cache.custom_icons.contains_key(name)
    }

    /// Usa a imagem do arquivo como ícone do programa, no lugar do que foi detectado.
    pub fn set_custom_icon(&self, name: &str, path: &std::path::Path) -> Result<(), String> {
        // A chave muda a cada troca: a textura da tela é guardada por ela.
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
        let icon = appcache::icon_from_file(path, format!("custom:{name}:{stamp}"))?;
        appcache::save_icon(&icon);
        let mut s = self.shared.lock().unwrap();
        let old = s.cache.icon_of.insert(name.to_owned(), icon.key.clone());
        // `old` era o ícone detectado na primeira troca; nas seguintes, a imagem anterior do usuário.
        match s.cache.custom_icons.contains_key(name) {
            true => {
                if let Some(old) = old {
                    appcache::remove_icon(&old);
                    s.icons.remove(&old);
                }
            }
            false => {
                s.cache.custom_icons.insert(name.to_owned(), old.unwrap_or_default());
            }
        }
        s.icons.insert(icon.key.clone(), Arc::new(icon));
        s.cache.save();
        Ok(())
    }

    /// Volta ao ícone detectado do programa.
    pub fn reset_icon(&self, name: &str) {
        let mut s = self.shared.lock().unwrap();
        let Some(prev) = s.cache.custom_icons.remove(name) else { return };
        if let Some(old) = s.cache.icon_of.remove(name) {
            appcache::remove_icon(&old);
            s.icons.remove(&old);
        }
        if !prev.is_empty() {
            s.cache.icon_of.insert(name.to_owned(), prev);
        }
        s.cache.save();
    }

    fn now(s: &Shared) -> Now {
        let current = s.current.clone();
        let icon = current.as_deref().and_then(|c| s.icon(c));
        Now { current, idle: s.idle, icon, state: s.state }
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
        session.sort_by_key(|r| std::cmp::Reverse(r.secs));
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
        /// Identificador da janela, para notar quando outra vem para a frente.
        pub hwnd: usize,
        /// Nome do executável sem extensão.
        pub exe: String,
        pub path: String,
        pub title: String,
    }

    /// Relógio do Windows em milissegundos (dá a volta a cada 49 dias; comparar com `wrapping_sub`).
    pub fn tick() -> u32 {
        unsafe { GetTickCount() }
    }

    /// Instante, nesse relógio, da última tecla ou clique em qualquer janela.
    pub fn last_input_tick() -> Option<u32> {
        unsafe {
            let mut info = LASTINPUTINFO { cbSize: size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
            (GetLastInputInfo(&mut info) != 0).then_some(info.dwTime)
        }
    }

    /// Caminho completo do executável do processo.
    fn process_path(pid: u32) -> Option<String> {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut buf = [0u16; 520];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
            CloseHandle(h);
            (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
        }
    }

    fn exe_stem(path: &str) -> Option<String> {
        let file = path.rsplit('\\').next()?;
        Some(file.rsplit_once('.').map_or(file, |(stem, _)| stem).to_owned())
    }

    /// Nome do executável (sem pasta nem extensão) do processo.
    pub fn process_exe(pid: u32) -> Option<String> {
        exe_stem(&process_path(pid)?)
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
            let path = process_path(pid)?;
            let exe = exe_stem(&path)?;
            Some(Foreground { hwnd: hwnd as usize, exe, path, title: window_title(hwnd) })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renomear_leva_o_historico_e_vale_para_sempre() {
        let mut s = Shared::default();
        s.usage.days.entry("2026-05-10".into()).or_default().insert("Foo".into(), 60);
        s.usage.days.entry("2026-05-10".into()).or_default().insert("Bar".into(), 40);
        s.session.insert("Foo".into(), 5);
        s.cache.icon_of.insert("Foo".into(), "foo.exe".into());
        s.current = Some("Foo".into());
        assert!(!s.state.counting(), "antes da primeira leitura, nada é contado");

        s.rename("Foo", "Meu app");
        assert_eq!(s.cache.aliases["Foo"], "Meu app");
        assert_eq!(s.usage.days["2026-05-10"]["Meu app"], 60);
        assert_eq!(s.cache.icon_of["Meu app"], "foo.exe");
        assert_eq!(s.current.as_deref(), Some("Meu app"));

        // Mudar de novo: o nome detectado passa a apontar para o novo, e tempos de nomes iguais se somam.
        s.rename("Meu app", "Bar");
        assert_eq!(s.cache.aliases["Foo"], "Bar");
        assert_eq!(s.usage.days["2026-05-10"]["Bar"], 100);
        assert!(!s.usage.days["2026-05-10"].contains_key("Foo"));
    }
}
