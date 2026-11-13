//! Bandeja do sistema. Ao "ir para a bandeja" a janela some da barra de tarefas e sai
//! da tela (continua existindo e desenhando, então o widget flutuante segue atualizando);
//! clicar no ícone da bandeja traz tudo de volta.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GWL_EXSTYLE, GetWindowLongPtrW, GetWindowPlacement, IsIconic, SW_HIDE, SW_SHOW, SW_SHOWMAXIMIZED,
    SW_SHOWNOACTIVATE, SW_SHOWNORMAL, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetWindowLongPtrW,
    SetWindowPlacement, SetWindowPos, ShowWindow, WINDOWPLACEMENT, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

const OPEN_ID: &str = "navi-open";
const QUIT_ID: &str = "navi-quit";

static WANT_RESTORE: AtomicBool = AtomicBool::new(false);
static WANT_QUIT: AtomicBool = AtomicBool::new(false);
static SAVED: Mutex<Option<Saved>> = Mutex::new(None);

/// Canto de onde a janela escondida fica, longe de qualquer monitor.
const OFFSCREEN: i32 = -32000;

/// Onde a janela estava antes de ir para a bandeja (posição normal, em coordenadas da área de trabalho).
struct Saved {
    rect: RECT,
    maximized: bool,
}

/// Pedidos feitos pelo ícone da bandeja, a tratar no laço da interface.
pub enum Request {
    Restore,
    Quit,
}

pub fn take_request() -> Option<Request> {
    if WANT_QUIT.swap(false, Ordering::Relaxed) {
        Some(Request::Quit)
    } else if WANT_RESTORE.swap(false, Ordering::Relaxed) {
        Some(Request::Restore)
    } else {
        None
    }
}

pub struct Tray {
    _icon: TrayIcon,
}

fn main_window() -> HWND {
    let title: Vec<u16> = "Navi".encode_utf16().chain(Some(0)).collect();
    unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) }
}

/// Esconde a janela principal e põe o ícone na bandeja.
pub fn enter(ctx: &egui::Context, rgba: &[u8]) -> Option<Tray> {
    // Os testes não mexem na janela do Navi que estiver aberta na máquina de quem os roda.
    if cfg!(test) {
        return None;
    }
    let hwnd = main_window();
    if hwnd.is_null() {
        return None;
    }
    let menu = Menu::new();
    menu.append(&MenuItem::with_id(OPEN_ID, "Abrir o Navi", true, None)).ok()?;
    menu.append(&MenuItem::with_id(QUIT_ID, "Sair", true, None)).ok()?;
    let icon = TrayIconBuilder::new()
        .with_icon(Icon::from_rgba(rgba.to_vec(), 64, 64).ok()?)
        .with_tooltip("Navi")
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .ok()?;

    let c = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
            WANT_RESTORE.store(true, Ordering::Relaxed);
            c.request_repaint();
        }
    }));
    let c = ctx.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        match e.id.0.as_str() {
            OPEN_ID => WANT_RESTORE.store(true, Ordering::Relaxed),
            QUIT_ID => WANT_QUIT.store(true, Ordering::Relaxed),
            _ => return,
        }
        c.request_repaint();
    }));

    unsafe {
        // O lugar da janela vem sempre de `GetWindowPlacement`, que serve para minimizada ou não e usa as
        // mesmas coordenadas (da área de trabalho) com que `leave` a devolve. `GetWindowRect` não serve:
        // minimizada, a janela diz estar em (-32000, -32000).
        let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
        placement.length = size_of::<WINDOWPLACEMENT>() as u32;
        GetWindowPlacement(hwnd, &mut placement);
        let was_iconic = IsIconic(hwnd) != 0;
        let normal = placement.rcNormalPosition;
        let mut saved = SAVED.lock().unwrap();
        if saved.is_none() {
            *saved = Some(Saved { rect: normal, maximized: placement.showCmd == SW_SHOWMAXIMIZED as u32 });
        }
        // Sem botão na barra de tarefas e fora da tela, mas ainda "visível" para o sistema.
        ShowWindow(hwnd, SW_HIDE);
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (ex | WS_EX_TOOLWINDOW as isize) & !(WS_EX_APPWINDOW as isize));
        if was_iconic {
            // Uma janela minimizada não pode ficar assim na bandeja (apareceria como uma barrinha no canto da
            // tela), e ao ser restaurada ela iria para a posição normal dela, que é na tela. Então a posição
            // normal passa a ser fora da tela, e a janela é restaurada sem ser ativada (para o teclado não ir
            // parar numa janela que ninguém vê). O lugar de verdade fica guardado em `SAVED`.
            placement.rcNormalPosition = RECT {
                left: OFFSCREEN,
                top: OFFSCREEN,
                right: OFFSCREEN + (normal.right - normal.left),
                bottom: OFFSCREEN + (normal.bottom - normal.top),
            };
            placement.flags = 0;
            placement.showCmd = SW_SHOWNOACTIVATE as u32;
            SetWindowPlacement(hwnd, &placement);
        } else {
            SetWindowPos(hwnd, std::ptr::null_mut(), OFFSCREEN, OFFSCREEN, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
    }
    Some(Tray { _icon: icon })
}

/// Traz a janela de volta ao lugar de antes (e ao tamanho, maximizada ou não).
pub fn leave() {
    let hwnd = main_window();
    if hwnd.is_null() {
        return;
    }
    unsafe {
        ShowWindow(hwnd, SW_HIDE);
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (ex & !(WS_EX_TOOLWINDOW as isize)) | WS_EX_APPWINDOW as isize);
        match SAVED.lock().unwrap().take() {
            Some(saved) => {
                // As coordenadas de `SetWindowPlacement` são as mesmas de `GetWindowPlacement`, que guardou o lugar.
                let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
                placement.length = size_of::<WINDOWPLACEMENT>() as u32;
                GetWindowPlacement(hwnd, &mut placement);
                placement.rcNormalPosition = saved.rect;
                placement.flags = 0;
                let show = if saved.maximized { SW_SHOWMAXIMIZED } else { SW_SHOWNORMAL };
                placement.showCmd = show as u32;
                SetWindowPlacement(hwnd, &placement);
            }
            None => {
                ShowWindow(hwnd, SW_SHOW);
            }
        }
        SetForegroundWindow(hwnd);
    }
}
