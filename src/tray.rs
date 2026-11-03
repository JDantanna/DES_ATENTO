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
    FindWindowW, GWL_EXSTYLE, GetWindowLongPtrW, GetWindowRect, SW_HIDE, SW_SHOW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_EX_APPWINDOW,
    WS_EX_TOOLWINDOW,
};

const OPEN_ID: &str = "navi-open";
const QUIT_ID: &str = "navi-quit";

static WANT_RESTORE: AtomicBool = AtomicBool::new(false);
static WANT_QUIT: AtomicBool = AtomicBool::new(false);
static SAVED: Mutex<Option<[i32; 4]>> = Mutex::new(None);

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
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(hwnd, &mut r);
        let mut saved = SAVED.lock().unwrap();
        if saved.is_none() {
            *saved = Some([r.left, r.top, r.right - r.left, r.bottom - r.top]);
        }
        // Sem botão na barra de tarefas e fora da tela, mas ainda "visível" para o sistema.
        ShowWindow(hwnd, SW_HIDE);
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (ex | WS_EX_TOOLWINDOW as isize) & !(WS_EX_APPWINDOW as isize));
        SetWindowPos(hwnd, std::ptr::null_mut(), -32000, -32000, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    Some(Tray { _icon: icon })
}

/// Traz a janela de volta ao lugar de antes.
pub fn leave() {
    let hwnd = main_window();
    if hwnd.is_null() {
        return;
    }
    unsafe {
        ShowWindow(hwnd, SW_HIDE);
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (ex & !(WS_EX_TOOLWINDOW as isize)) | WS_EX_APPWINDOW as isize);
        if let Some([x, y, _, _]) = SAVED.lock().unwrap().take() {
            SetWindowPos(hwnd, std::ptr::null_mut(), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
    }
}