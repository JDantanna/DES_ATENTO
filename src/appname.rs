//! Nome de exibição do programa em primeiro plano. Em navegadores, o nome do site
//! ou do aplicativo web (WhatsApp Web, YouTube...) no lugar do nome do navegador.

/// (exe em minúsculas, nome do navegador, trecho que aparece no título da janela)
const BROWSERS: [(&str, &str, &str); 7] = [
    ("chrome", "Google Chrome", "chrome"),
    ("msedge", "Microsoft Edge", "edge"),
    ("firefox", "Mozilla Firefox", "firefox"),
    ("brave", "Brave", "brave"),
    ("opera", "Opera", "opera"),
    ("vivaldi", "Vivaldi", "vivaldi"),
    ("arc", "Arc", "arc"),
];

/// (trecho do título em minúsculas, nome do site). A ordem importa: o primeiro que casar vence.
const SITES: [(&str, &str); 36] = [
    ("whatsapp", "WhatsApp Web"),
    ("youtube", "YouTube"),
    ("gmail", "Gmail"),
    ("google agenda", "Google Agenda"),
    ("google calendar", "Google Agenda"),
    ("google drive", "Google Drive"),
    ("meu drive", "Google Drive"),
    ("google docs", "Google Docs"),
    ("google planilhas", "Google Planilhas"),
    ("google sheets", "Google Planilhas"),
    ("google apresentações", "Google Apresentações"),
    ("google meet", "Google Meet"),
    ("outlook", "Outlook"),
    ("microsoft teams", "Microsoft Teams"),
    ("notion", "Notion"),
    ("github", "GitHub"),
    ("stack overflow", "Stack Overflow"),
    ("chatgpt", "ChatGPT"),
    ("claude", "Claude"),
    ("netflix", "Netflix"),
    ("spotify", "Spotify"),
    ("instagram", "Instagram"),
    ("facebook", "Facebook"),
    ("linkedin", "LinkedIn"),
    ("telegram", "Telegram Web"),
    ("discord", "Discord"),
    ("figma", "Figma"),
    ("canva", "Canva"),
    ("trello", "Trello"),
    ("slack", "Slack"),
    ("twitch", "Twitch"),
    ("reddit", "Reddit"),
    ("mercado livre", "Mercado Livre"),
    ("amazon", "Amazon"),
    ("wikipédia", "Wikipédia"),
    ("/ x", "X"),
];

fn clean(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, '\u{200b}' | '\u{200e}' | '\u{200f}' | '\u{feff}')).collect()
}

/// Remove o contador de notificações do começo: "(3) WhatsApp" -> "WhatsApp".
fn strip_counter(s: &str) -> &str {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix('(') {
        if let Some((n, tail)) = rest.split_once(')') {
            if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) {
                return tail.trim_start();
            }
        }
    }
    t
}

/// Navegadores e o hospedeiro de apps da Store são nomeados pelo título da janela.
pub fn needs_product(exe: &str) -> bool {
    let lower = exe.to_lowercase();
    lower != "applicationframehost" && !BROWSERS.iter().any(|b| b.0 == lower)
}

/// Nome que aparece nos relatórios. `product` é o nome do produto lido do executável.
pub fn display_name(exe: &str, title: &str, product: Option<&str>) -> String {
    let lower = exe.to_lowercase();
    if let Some((_, pretty, key)) = BROWSERS.iter().find(|b| b.0 == lower) {
        return browser_name(pretty, key, title);
    }
    // Aplicativos da Microsoft Store rodam dentro deste processo hospedeiro.
    if lower == "applicationframehost" {
        let t = clean(title);
        if !t.trim().is_empty() {
            return t.trim().to_owned();
        }
    }
    match product {
        Some(p) if lower.contains("shipping") => prettify(p),
        Some(p) => p.to_owned(),
        None => prettify(exe),
    }
}

/// Nome do produto do executável (recurso de versão do arquivo): "Valor Mortis", "Task Manager"...
pub fn product_name(path: &str) -> Option<String> {
    let (product, desc) = crate::tracker::version_names(path)?;
    pick_product(product.as_deref(), desc.as_deref())
}

fn tidy_product(s: &str) -> String {
    // Tira ®, ™ e © do texto do recurso de versão.
    let s: String = s.chars().filter(|c| !matches!(c, '\u{ae}' | '\u{2122}' | '\u{a9}')).collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Prefere o ProductName; cai para a descrição quando ele é genérico (nome do Windows, de motor de jogo).
fn pick_product(product: Option<&str>, desc: Option<&str>) -> Option<String> {
    const GENERIC: [&str; 6] =
        ["operating system", "sistema operacional", "microsoft windows", "unreal engine", "unity", "electron"];
    let good = |s: &String| {
        let low = s.to_lowercase();
        !s.is_empty() && !GENERIC.iter().any(|g| low.contains(g))
    };
    let (product, desc) = (product.map(tidy_product), desc.map(tidy_product));
    product.filter(good).or(desc.filter(good))
}

/// Deixa legível um nome de executável: tira sufixos de build e separa palavras.
/// "ValorMortis-Win64-Shipping" -> "Valor Mortis".
pub fn prettify(exe: &str) -> String {
    let mut s = exe.to_owned();
    for suffix in ["-Win64-Shipping", "-Win32-Shipping", "-WinGDK-Shipping", "_Win64_Shipping", "-Shipping", "-Win64", "_x64"] {
        if let Some(i) = s.to_lowercase().rfind(&suffix.to_lowercase()) {
            s.truncate(i);
        }
    }
    let s = s.replace(['_', '-'], " ");
    let mut out = String::with_capacity(s.len() + 4);
    let mut prev: Option<char> = None;
    for c in s.chars() {
        if c.is_uppercase() && prev.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit()) {
            out.push(' ');
        }
        out.push(c);
        prev = Some(c);
    }
    let out = out.trim().to_owned();
    if out.is_empty() { exe.to_owned() } else { out }
}

fn browser_name(pretty: &str, key: &str, title: &str) -> String {
    let title = clean(title);
    let pieces: Vec<&str> = title.split(" - ").flat_map(|p| p.split(" – ")).flat_map(|p| p.split(" — ")).collect();
    // Tira do fim o nome do navegador e o perfil ("... - Pessoal - Microsoft Edge").
    let browser_at = pieces.iter().rposition(|p| p.to_lowercase().contains(key));
    let (page, is_app_window) = match browser_at {
        Some(i) => (&pieces[..i], false),
        None => (&pieces[..], true),
    };
    let page_title = strip_counter(&page.join(" - ")).to_owned();
    let low = page_title.to_lowercase();
    if let Some((_, site)) = SITES.iter().find(|(k, _)| low.contains(k)) {
        return (*site).to_owned();
    }
    // Janela de aplicativo instalado (PWA): o título já é o nome do app.
    if is_app_window {
        let t = page_title.trim();
        return if t.is_empty() { pretty.to_owned() } else { t.to_owned() };
    }
    // Páginas costumam terminar com o nome do site: "Notícia - Folha".
    if page.len() > 1 {
        if let Some(last) = page.last().map(|l| l.trim()).filter(|l| !l.is_empty() && l.chars().count() <= 22) {
            return last.to_owned();
        }
    }
    pretty.to_owned()
}

#[cfg(test)]
mod tests {
    use super::{display_name, pick_product, prettify};

    fn d(exe: &str, title: &str) -> String {
        display_name(exe, title, None)
    }

    #[test]
    fn sites_e_navegadores() {
        assert_eq!(d("chrome", "(3) WhatsApp - Google Chrome"), "WhatsApp Web");
        assert_eq!(d("msedge", "Início - YouTube e mais 2 páginas - Pessoal – Microsoft\u{200b} Edge"), "YouTube");
        assert_eq!(d("chrome", "Notícia boa - Folha - Google Chrome"), "Folha");
        assert_eq!(d("chrome", "Qualquer coisa - Google Chrome"), "Google Chrome");
        assert_eq!(d("chrome", "WhatsApp"), "WhatsApp Web");
        assert_eq!(d("Code", "x"), "Code");
    }

    #[test]
    fn nome_do_produto() {
        assert_eq!(prettify("ValorMortis-Win64-Shipping"), "Valor Mortis");
        assert_eq!(display_name("ValorMortis-Win64-Shipping", "", Some("ValorMortis")), "Valor Mortis");
        assert_eq!(display_name("Taskmgr", "", Some("Task Manager")), "Task Manager");
        assert_eq!(
            pick_product(Some("Microsoft\u{ae} Windows\u{ae} Operating System"), Some("Gerenciador de Tarefas")).as_deref(),
            Some("Gerenciador de Tarefas")
        );
        assert_eq!(pick_product(Some("Unreal Engine"), Some("Valor Mortis")).as_deref(), Some("Valor Mortis"));
    }

    #[test]
    fn le_o_nome_real_dos_programas() {
        for exe in ["C:\\Windows\\explorer.exe", "C:\\Windows\\System32\\taskmgr.exe", "C:\\Windows\\System32\\notepad.exe"] {
            let name = super::product_name(exe);
            println!("{exe} -> {name:?}");
            assert!(name.is_some());
        }
    }
}
