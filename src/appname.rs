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

pub fn display_name(exe: &str, title: &str) -> String {
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
    exe.to_owned()
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
    use super::display_name as d;

    #[test]
    fn sites_e_navegadores() {
        assert_eq!(d("chrome", "(3) WhatsApp - Google Chrome"), "WhatsApp Web");
        assert_eq!(d("msedge", "Início - YouTube e mais 2 páginas - Pessoal – Microsoft\u{200b} Edge"), "YouTube");
        assert_eq!(d("chrome", "Notícia boa - Folha - Google Chrome"), "Folha");
        assert_eq!(d("chrome", "Qualquer coisa - Google Chrome"), "Google Chrome");
        assert_eq!(d("chrome", "WhatsApp"), "WhatsApp Web");
        assert_eq!(d("Code", "x"), "Code");
    }
}
