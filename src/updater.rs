//! Atualização via GitHub Releases. Usa o `curl.exe` do Windows para não
//! trazer uma pilha HTTP/TLS inteira para dentro do executável.

use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

/// Repositório "usuario/nome" que publica as releases. Troque pelo seu.
const REPO: &str = "JDantanna/DES_ATENTO";
const ASSET: &str = "Des_Atento_Setup.exe";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Default, PartialEq)]
pub enum State {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available { version: String, url: String },
    Downloading,
    Error(String),
}

#[derive(Clone, Default)]
pub struct Updater {
    state: Arc<Mutex<State>>,
}

#[cfg(windows)]
fn curl() -> Command {
    use std::os::windows::process::CommandExt;
    let mut c = Command::new("curl.exe");
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    c
}

fn parse(v: &str) -> Vec<u32> {
    v.trim_start_matches(['v', 'V']).split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

fn check_blocking() -> Result<State, String> {
    let out = curl()
        .args(["-sSL", "--max-time", "15", "-H", "User-Agent: Des_Atento", "-H", "Accept: application/vnd.github+json"])
        .arg(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .output()
        .map_err(|e| format!("curl indisponível: {e}"))?;
    if !out.status.success() {
        return Err("Sem conexão com o GitHub.".into());
    }
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|_| "Resposta inválida do GitHub.".to_owned())?;
    let tag = json["tag_name"].as_str().ok_or("Nenhuma release publicada ainda.")?;
    if parse(tag) <= parse(VERSION) {
        return Ok(State::UpToDate);
    }
    let url = json["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"] == ASSET))
        .and_then(|x| x["browser_download_url"].as_str())
        .ok_or_else(|| format!("A release {tag} não tem o arquivo {ASSET}."))?;
    Ok(State::Available { version: tag.trim_start_matches(['v', 'V']).to_owned(), url: url.to_owned() })
}

impl Updater {
    pub fn state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    fn set(&self, s: State) {
        *self.state.lock().unwrap() = s;
    }

    pub fn check(&self, ctx: eframe::egui::Context) {
        self.set(State::Checking);
        let this = self.clone();
        std::thread::spawn(move || {
            this.set(check_blocking().unwrap_or_else(State::Error));
            ctx.request_repaint();
        });
    }

    /// Baixa o instalador, executa em modo silencioso e encerra o app;
    /// o instalador fecha/reabre o Des_Atento sozinho.
    pub fn install(&self, url: String, ctx: eframe::egui::Context) {
        self.set(State::Downloading);
        let this = self.clone();
        std::thread::spawn(move || {
            let dest = std::env::temp_dir().join(ASSET);
            let ok = curl()
                .args(["-sSL", "--fail", "--max-time", "300", "-o"])
                .arg(&dest)
                .arg(&url)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ok {
                this.set(State::Error("Falha ao baixar a atualização.".into()));
                ctx.request_repaint();
                return;
            }
            let started = Command::new(&dest)
                .args(["/SILENT", "/SUPPRESSMSGBOXES", "/CLOSEAPPLICATIONS", "/NORESTART"])
                .stdin(Stdio::null())
                .spawn()
                .is_ok();
            if started {
                // Salva o uso antes de sair; o instalador reabre o app ao terminar.
                ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
            } else {
                this.set(State::Error("Não foi possível iniciar o instalador.".into()));
                ctx.request_repaint();
            }
        });
    }
}
