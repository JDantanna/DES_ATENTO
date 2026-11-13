//! Atualização via GitHub Releases. Usa o `curl.exe` do Windows para não
//! trazer uma pilha HTTP/TLS inteira para dentro do executável.
//!
//! A versão mais nova vem do redirecionamento público de `/releases/latest` (o `Location` aponta para
//! `/releases/tag/vX.Y.Z`), e o instalador é baixado do endereço fixo da release. Nada passa pela API do GitHub
//! (`api.github.com`), que limita a 60 consultas por hora por IP: esgotada, o botão de atualizar parava de funcionar.

use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

/// Repositório "usuario/nome" que publica as releases. Troque pelo seu.
const REPO: &str = "JDantanna/DES_ATENTO";
const ASSET: &str = "Navi_Setup.exe";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Um instalador menor que isto veio quebrado (uma página de erro, um download cortado).
const MIN_INSTALLER_BYTES: u64 = 100_000;

#[derive(Clone, Default, PartialEq)]
pub enum State {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available { version: String, url: String },
    Downloading,
    /// O instalador já está rodando: o app precisa fechar, sem passar pela pergunta de sair.
    Closing,
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

/// O que o redirecionamento de `/releases/latest` diz.
#[derive(Debug, PartialEq)]
enum Latest {
    /// A versão da release mais nova, sem o "v" (`0.9.7`).
    Version(String),
    /// O repositório não tem nenhuma release (o GitHub manda para a lista de releases).
    NoRelease,
    /// Qualquer outro endereço.
    Unexpected,
}

/// Lê o `Location` do redirecionamento: `https://github.com/dono/repo/releases/tag/v0.9.7`, ou só o caminho
/// (`/dono/repo/releases/tag/v0.9.7`); com `v` ou sem, com barra no fim ou não.
fn parse_location(location: &str) -> Latest {
    let path = location.split(['?', '#']).next().unwrap_or("").trim().trim_end_matches('/');
    if let Some((_, tag)) = path.rsplit_once("/releases/tag/") {
        let version = tag.trim_start_matches(['v', 'V']);
        let valid = !version.is_empty() && version.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
        return if valid { Latest::Version(version.to_owned()) } else { Latest::Unexpected };
    }
    if path.ends_with("/releases") { Latest::NoRelease } else { Latest::Unexpected }
}

/// Lê os cabeçalhos da resposta de `/releases/latest` (a primeira linha traz o código HTTP).
fn parse_headers(headers: &str) -> Result<Latest, String> {
    let status: u16 = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| "O GitHub respondeu algo que não entendi. Tente de novo em instantes.".to_owned())?;
    let location = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim().eq_ignore_ascii_case("location").then(|| value.trim().to_owned())
    });
    match (status, location) {
        (301 | 302 | 303 | 307 | 308, Some(location)) => Ok(parse_location(&location)),
        (429, _) => Err("O GitHub pediu para esperar um pouco antes de verificar de novo. Tente em alguns minutos.".to_owned()),
        (500..=599, _) => Err(format!("O GitHub está com problemas agora (erro {status}). Tente mais tarde.")),
        _ => Err(format!("O GitHub respondeu algo inesperado (código {status}). Tente de novo em instantes.")),
    }
}

/// Mensagem para um `curl` que terminou com erro, de acordo com o código dele.
fn curl_error(code: Option<i32>, doing: &str) -> String {
    match code {
        // 6: nome não resolvido; 7: não conectou; 28: tempo esgotado; 35, 51, 58, 60: TLS; 56: conexão caiu.
        Some(6 | 7 | 28 | 35 | 56) => format!("Sem conexão com a internet para {doing}."),
        Some(22) => "O instalador desta versão ainda não está no GitHub (pode estar sendo publicado). Tente em alguns minutos.".to_owned(),
        Some(code) => format!("Não foi possível {doing} (erro {code} do curl)."),
        None => format!("Não foi possível {doing}: o curl foi interrompido."),
    }
}

fn check_blocking() -> Result<State, String> {
    let out = curl()
        .args(["-sI", "--max-time", "15", "-H", "User-Agent: Navi"])
        .arg(format!("https://github.com/{REPO}/releases/latest"))
        .output()
        .map_err(|e| format!("O curl não está disponível neste Windows: {e}"))?;
    if !out.status.success() {
        return Err(curl_error(out.status.code(), "verificar atualizações"));
    }
    let version = match parse_headers(&String::from_utf8_lossy(&out.stdout))? {
        Latest::Version(version) => version,
        Latest::NoRelease => return Err("Nenhuma release publicada ainda.".to_owned()),
        Latest::Unexpected => return Err("O GitHub respondeu algo que não entendi (endereço da release inesperado).".to_owned()),
    };
    if parse(&version) <= parse(VERSION) {
        return Ok(State::UpToDate);
    }
    let url = format!("https://github.com/{REPO}/releases/download/v{version}/{ASSET}");
    Ok(State::Available { version, url })
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
    /// o instalador fecha/reabre o Navi sozinho.
    pub fn install(&self, url: String, ctx: eframe::egui::Context) {
        self.set(State::Downloading);
        let this = self.clone();
        std::thread::spawn(move || {
            let dest = std::env::temp_dir().join(ASSET);
            let _ = std::fs::remove_file(&dest);
            let downloaded = curl().args(["-sSL", "--fail", "--max-time", "300", "-o"]).arg(&dest).arg(&url).status();
            let problem = match downloaded {
                Err(e) => Some(format!("O curl não está disponível neste Windows: {e}")),
                Ok(status) if !status.success() => Some(curl_error(status.code(), "baixar a atualização")),
                Ok(_) if !std::fs::metadata(&dest).is_ok_and(|m| m.len() >= MIN_INSTALLER_BYTES) => {
                    Some("O arquivo baixado veio incompleto. Tente de novo.".to_owned())
                }
                Ok(_) => None,
            };
            if let Some(problem) = problem {
                this.set(State::Error(problem));
                ctx.request_repaint();
                return;
            }
            let started = Command::new(&dest)
                .args(["/SILENT", "/SUPPRESSMSGBOXES", "/CLOSEAPPLICATIONS", "/NORESTART"])
                .stdin(Stdio::null())
                .spawn()
                .is_ok();
            // O instalador reabre o app ao terminar.
            this.set(if started { State::Closing } else { State::Error("Não foi possível iniciar o instalador baixado.".into()) });
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(v: &str) -> Latest {
        Latest::Version(v.to_owned())
    }

    #[test]
    fn le_a_versao_do_redirecionamento() {
        let tag = "https://github.com/JDantanna/DES_ATENTO/releases/tag";
        assert_eq!(parse_location(&format!("{tag}/v0.9.7")), version("0.9.7"));
        assert_eq!(parse_location(&format!("{tag}/0.9.7")), version("0.9.7"), "sem o v");
        assert_eq!(parse_location(&format!("{tag}/V1.10.0")), version("1.10.0"), "v maiúsculo");
        assert_eq!(parse_location(&format!("{tag}/v0.9.7/")), version("0.9.7"), "barra no fim");
        assert_eq!(parse_location("/JDantanna/DES_ATENTO/releases/tag/v0.9.7"), version("0.9.7"), "só o caminho");
        assert_eq!(parse_location(&format!("  {tag}/v0.9.7?x=1 ")), version("0.9.7"), "espaço e consulta");
    }

    #[test]
    fn caminho_invalido_nao_vira_versao() {
        for bad in [
            "",
            "https://github.com/login",
            "https://github.com/JDantanna/DES_ATENTO",
            "https://github.com/JDantanna/DES_ATENTO/releases/tag/",
            "https://github.com/JDantanna/DES_ATENTO/releases/tag/v",
            "https://github.com/JDantanna/DES_ATENTO/releases/tag/nightly",
            "https://github.com/JDantanna/DES_ATENTO/releases/tag/v0.9.",
            "https://github.com/JDantanna/DES_ATENTO/releases/tag/v0..9",
            "https://github.com/JDantanna/DES_ATENTO/releases/tag/v0.9.7-beta",
        ] {
            assert_eq!(parse_location(bad), Latest::Unexpected, "{bad:?}");
        }
        // Sem nenhuma release, o GitHub manda para a lista.
        assert_eq!(parse_location("https://github.com/JDantanna/DES_ATENTO/releases"), Latest::NoRelease);
    }

    #[test]
    fn le_os_cabecalhos_do_curl() {
        let ok = "HTTP/2 302 \r\nserver: GitHub.com\r\nLocation: https://github.com/JDantanna/DES_ATENTO/releases/tag/v0.9.7\r\n\r\n";
        assert_eq!(parse_headers(ok), Ok(version("0.9.7")));
        let lower = "HTTP/1.1 302 Found\nlocation: /JDantanna/DES_ATENTO/releases/tag/v0.9.7\n";
        assert_eq!(parse_headers(lower), Ok(version("0.9.7")), "nome do cabeçalho em minúsculas");
        // Sem Location, ou com um código que não é de redirecionamento, é "resposta inesperada", não "sem release".
        assert!(parse_headers("HTTP/2 200 \r\n\r\n").unwrap_err().contains("200"));
        assert!(parse_headers("HTTP/2 302 \r\n\r\n").unwrap_err().contains("302"));
        assert!(parse_headers("HTTP/2 503 \r\n\r\n").unwrap_err().contains("problemas"));
        assert!(parse_headers("HTTP/2 429 \r\n\r\n").unwrap_err().contains("esperar"));
        assert!(parse_headers("").is_err());
        assert!(parse_headers("<html>").is_err());
    }

    #[test]
    fn mensagens_de_erro_do_curl() {
        assert!(curl_error(Some(6), "baixar a atualização").starts_with("Sem conexão"));
        assert!(curl_error(Some(28), "verificar atualizações").starts_with("Sem conexão"));
        assert!(curl_error(Some(22), "baixar a atualização").contains("ainda não está no GitHub"));
        assert!(curl_error(Some(3), "baixar a atualização").contains("erro 3"));
        assert!(curl_error(None, "baixar a atualização").contains("interrompido"));
    }

    #[test]
    fn so_oferece_versao_mais_nova() {
        assert!(parse("0.9.7") > parse("0.9.6"));
        assert!(parse("0.10.0") > parse("0.9.9"));
        assert!(parse("1.0.0") > parse("0.99.99"));
        assert!(parse("v0.9.7") == parse("0.9.7"), "o v não conta");
    }
}
