//! Sinais passivos de uso, lidos do Windows: programa em tela cheia, áudio saindo do programa
//! e controle de jogo. São consultados só quando o teclado e o mouse já estão parados há um
//! tempo (ver `engagement`), então custam nada enquanto a pessoa trabalha.

use std::collections::HashMap;
use std::ffi::c_void;

use windows_sys::Win32::Foundation::{ERROR_DEVICE_NOT_CONNECTED, RECT};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
use windows_sys::Win32::Media::Audio::{AudioSessionStateActive, DEVICE_STATE_ACTIVE, MMDeviceEnumerator, eConsole, eRender};
use windows_sys::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows_sys::Win32::UI::Input::XboxController::{XINPUT_STATE, XInputGetState};
use windows_sys::Win32::UI::Shell::{
    QUNS_APP, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;
use windows_sys::core::{GUID, HRESULT};

use super::engagement::Passive;
use super::sys;

const IID_IMMDEVICE_ENUMERATOR: GUID = GUID::from_u128(0xa95664d2_9614_4f35_a746_de8db63617e6);
const IID_IAUDIO_SESSION_MANAGER2: GUID = GUID::from_u128(0x77aa99a0_1bd6_484f_8bc7_2c654c9a9b6f);
const IID_IAUDIO_SESSION_CONTROL2: GUID = GUID::from_u128(0xbfb7ff88_7239_4fc9_8fa2_07c950be9c6d);
/// Depois de um controle aparecer desconectado, só é consultado de novo passado este tempo
/// (perguntar por um controle ausente é lento).
const PAD_RETRY_MS: u32 = 60_000;

/// Prepara a thread para usar COM (as sessões de áudio). Chamar uma vez, na thread do rastreador.
pub(super) fn init() {
    // S_FALSE (já iniciada) também serve; qualquer falha só desliga o sinal de áudio.
    let hr = unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) };
    trace(&format!("CoInitializeEx {hr:#x}"));
}

/// Nos testes, conta cada passo das sondas no stdout (o teste pai mostra isso se o processo filho cair).
fn trace(step: &str) {
    if cfg!(test) {
        println!("sonda: {step}");
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }
}

/// A janela `hwnd` está em tela cheia (jogo, vídeo, apresentação)? Precisa das duas coisas: o Windows
/// dizer que há um app de tela cheia (o critério que segura as notificações, mas que vale para o sistema
/// todo) e a janela da frente cobrir o monitor inteiro (uma janela só maximizada para na barra de tarefas).
pub(super) fn fullscreen(hwnd: usize) -> bool {
    let mut state = 0;
    unsafe {
        if SHQueryUserNotificationState(&mut state) < 0
            || !matches!(state, QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE | QUNS_APP)
        {
            return false;
        }
        let hwnd = hwnd as *mut c_void;
        let mut window: RECT = std::mem::zeroed();
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if GetWindowRect(hwnd, &mut window) == 0 || GetMonitorInfoW(monitor, &mut info) == 0 {
            return false;
        }
        let m = info.rcMonitor;
        window.left <= m.left && window.top <= m.top && window.right >= m.right && window.bottom >= m.bottom
    }
}

/// Objeto COM cru: a tabela de métodos é lida pela posição, porque o `windows-sys` não traz as
/// interfaces de áudio. Solta o objeto ao sair de escopo.
struct Com(*mut c_void);

impl Com {
    /// Método na posição `index` da tabela virtual (0 a 2 são os de `IUnknown`).
    unsafe fn method<F: Copy>(&self, index: usize) -> F {
        unsafe {
            let vtbl = *(self.0 as *const *const usize);
            std::mem::transmute_copy::<usize, F>(&*vtbl.add(index))
        }
    }

    /// Chama um método que devolve um objeto novo pelo último argumento.
    unsafe fn child(&self, index: usize, call: impl FnOnce(usize, *mut *mut c_void) -> HRESULT) -> Option<Com> {
        let mut out: *mut c_void = std::ptr::null_mut();
        let hr = call(unsafe { self.method::<usize>(index) }, &mut out);
        (hr >= 0 && !out.is_null()).then_some(Com(out))
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        unsafe {
            let release: unsafe extern "system" fn(*mut c_void) -> u32 = self.method(2);
            release(self.0);
        }
    }
}

type NewObject = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT;
type QueryInterface = unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT;
type DefaultEndpoint = unsafe extern "system" fn(*mut c_void, i32, i32, *mut *mut c_void) -> HRESULT;
type EnumEndpoints = unsafe extern "system" fn(*mut c_void, i32, u32, *mut *mut c_void) -> HRESULT;
type Activate = unsafe extern "system" fn(*mut c_void, *const GUID, u32, *const c_void, *mut *mut c_void) -> HRESULT;
type GetSession = unsafe extern "system" fn(*mut c_void, i32, *mut *mut c_void) -> HRESULT;
type GetI32 = unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT;
type GetU32 = unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT;

/// Sessões de áudio do Windows: diz se algum processo de um executável está tocando som.
#[derive(Default)]
pub(super) struct Audio {
    /// Executável (minúsculo, sem extensão) de cada processo já visto. O Windows reaproveita
    /// números de processo, então a tabela é zerada de tempos em tempos.
    exe_of: HashMap<u32, Option<String>>,
}

impl Audio {
    /// Há som saindo de um processo chamado `exe` (comparação sem maiúsculas)? O navegador toca
    /// por um processo filho, de número diferente da janela: por isso a comparação é pelo nome.
    pub(super) fn playing(&mut self, exe: &str) -> bool {
        if self.exe_of.len() > 128 {
            self.exe_of.clear();
        }
        let exe = exe.to_lowercase();
        unsafe {
            let mut raw: *mut c_void = std::ptr::null_mut();
            let hr = CoCreateInstance(&MMDeviceEnumerator, std::ptr::null_mut(), CLSCTX_ALL, &IID_IMMDEVICE_ENUMERATOR, &mut raw);
            trace(&format!("CoCreateInstance {hr:#x} {raw:?}"));
            if hr < 0 || raw.is_null() {
                return false;
            }
            let enumerator = Com(raw);
            // IMMDeviceEnumerator::EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE): sem nenhuma saída de áudio
            // ativa (máquina sem placa de som, serviço de áudio parado) não há o que ouvir, e pedir o dispositivo
            // padrão nessa situação derruba o processo em alguns Windows (visto no servidor de testes).
            let Some(outputs) = enumerator.child(3, |f, out| {
                let f: EnumEndpoints = std::mem::transmute(f);
                f(enumerator.0, eRender, DEVICE_STATE_ACTIVE, out)
            }) else {
                trace("sem lista de saídas de áudio");
                return false;
            };
            let mut outputs_count = 0u32;
            let get_outputs: GetU32 = outputs.method(3);
            if get_outputs(outputs.0, &mut outputs_count) < 0 || outputs_count == 0 {
                trace("nenhuma saída de áudio ativa");
                return false;
            }
            trace(&format!("{outputs_count} saídas de áudio ativas"));
            // IMMDeviceEnumerator::GetDefaultAudioEndpoint
            let Some(device) = enumerator.child(4, |f, out| {
                let f: DefaultEndpoint = std::mem::transmute(f);
                f(enumerator.0, eRender, eConsole, out)
            }) else {
                trace("sem dispositivo de áudio padrão");
                return false;
            };
            trace("dispositivo pronto");
            // IMMDevice::Activate(IAudioSessionManager2)
            let Some(manager) = device.child(3, |f, out| {
                let f: Activate = std::mem::transmute(f);
                f(device.0, &IID_IAUDIO_SESSION_MANAGER2, CLSCTX_ALL, std::ptr::null(), out)
            }) else {
                trace("sem gerenciador de sessões");
                return false;
            };
            trace("gerenciador pronto");
            // IAudioSessionManager2::GetSessionEnumerator (depois dos 2 métodos de IAudioSessionManager)
            let Some(sessions) = manager.child(5, |f, out| {
                let f: NewObject = std::mem::transmute(f);
                f(manager.0, out)
            }) else {
                trace("sem enumerador de sessões");
                return false;
            };
            trace("enumerador de sessões pronto");
            let mut count = 0;
            let get_count: GetI32 = sessions.method(3);
            if get_count(sessions.0, &mut count) < 0 {
                return false;
            }
            trace(&format!("{count} sessões"));
            for i in 0..count {
                // IAudioSessionEnumerator::GetSession
                let Some(session) = sessions.child(4, |f, out| {
                    let f: GetSession = std::mem::transmute(f);
                    f(sessions.0, i, out)
                }) else {
                    continue;
                };
                // IAudioSessionControl::GetState: só sessões tocando agora.
                let mut state = 0;
                let get_state: GetI32 = session.method(3);
                if get_state(session.0, &mut state) < 0 || state != AudioSessionStateActive {
                    continue;
                }
                // IUnknown::QueryInterface(IAudioSessionControl2), para saber o processo.
                let Some(control2) = session.child(0, |f, out| {
                    let f: QueryInterface = std::mem::transmute(f);
                    f(session.0, &IID_IAUDIO_SESSION_CONTROL2, out)
                }) else {
                    continue;
                };
                // IAudioSessionControl2::GetProcessId: 3 de IUnknown + 9 de IAudioSessionControl + 2.
                let mut pid = 0u32;
                let get_pid: GetU32 = control2.method(14);
                if get_pid(control2.0, &mut pid) < 0 || pid == 0 {
                    continue;
                }
                trace(&format!("sessão {i} ativa, processo {pid}"));
                let found = self.exe_of.entry(pid).or_insert_with(|| sys::process_exe(pid).map(|e| e.to_lowercase()));
                if found.as_deref() == Some(exe.as_str()) {
                    return true;
                }
            }
        }
        false
    }
}

/// Controles de jogo (XInput): um botão ou analógico mexido muda o número do pacote.
#[derive(Default)]
pub(super) struct Pad {
    packet: [Option<u32>; 4],
    /// Instante a partir do qual cada entrada pode ser consultada de novo.
    retry_at: [u32; 4],
}

impl Pad {
    /// Algum controle foi mexido desde a última consulta?
    pub(super) fn moved(&mut self, now: u32) -> bool {
        let mut moved = false;
        for i in 0..4 {
            if (now.wrapping_sub(self.retry_at[i]) as i32) < 0 {
                continue;
            }
            let mut state: XINPUT_STATE = unsafe { std::mem::zeroed() };
            let result = unsafe { XInputGetState(i as u32, &mut state) };
            if result != 0 {
                if result == ERROR_DEVICE_NOT_CONNECTED {
                    self.retry_at[i] = now.wrapping_add(PAD_RETRY_MS);
                }
                self.packet[i] = None;
                continue;
            }
            if self.packet[i].is_some_and(|p| p != state.dwPacketNumber) {
                moved = true;
            }
            self.packet[i] = Some(state.dwPacketNumber);
        }
        moved
    }
}

/// Lê os três sinais de uma vez, para a janela `hwnd` do programa `exe` que está na frente.
pub(super) fn passive(hwnd: usize, exe: &str, audio: &mut Audio, pad: &mut Pad, now: u32) -> Passive {
    Passive { fullscreen: fullscreen(hwnd), audio: audio.playing(exe), controller: pad.moved(now) }
}

/// Cada sonda roda num processo filho (o próprio executável de testes, filtrado para um teste só): se o Windows
/// derrubar o processo, o teste pai conta isso como falha com o nome da sonda, em vez de levar a suíte inteira.
#[cfg(test)]
mod tests {
    use super::*;

    const CHILD: &str = "NAVI_SONDA_FILHO";

    fn in_child() -> bool {
        std::env::var_os(CHILD).is_some()
    }

    fn run_in_child(test: &str) {
        let exe = std::env::current_exe().expect("caminho do executável de testes");
        let out = std::process::Command::new(exe)
            .args(["--exact", &format!("tracker::probe::tests::{test}"), "--test-threads=1", "--nocapture"])
            .env(CHILD, "1")
            .output()
            .expect("rodar o processo filho");
        assert!(
            out.status.success(),
            "{test}: o processo filho terminou com {:?}\n{}\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn sonda_janela_da_frente() {
        if !in_child() {
            return run_in_child("sonda_janela_da_frente");
        }
        let fg = sys::foreground();
        println!("frente: {:?}", fg.as_ref().map(|f| (&f.exe, &f.title)));
        println!("tick {} entrada {:?}", sys::tick(), sys::last_input_tick());
        println!("meu exe: {:?}", sys::process_exe(std::process::id()));
    }

    #[test]
    fn sonda_tela_cheia() {
        if !in_child() {
            return run_in_child("sonda_tela_cheia");
        }
        let hwnd = sys::foreground().map_or(0, |f| f.hwnd);
        println!("tela cheia: {}", fullscreen(hwnd));
    }

    #[test]
    fn sonda_audio() {
        if !in_child() {
            return run_in_child("sonda_audio");
        }
        init();
        let mut audio = Audio::default();
        println!("áudio: {}", audio.playing("chrome"));
        println!("áudio de novo: {}", audio.playing("navi"));
    }

    #[test]
    fn sonda_controle() {
        if !in_child() {
            return run_in_child("sonda_controle");
        }
        let mut pad = Pad::default();
        let now = sys::tick();
        println!("controle: {} {}", pad.moved(now), pad.moved(now.wrapping_add(5_000)));
    }
}
