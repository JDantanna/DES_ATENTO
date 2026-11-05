//! Cache em disco do que o Monitoramento descobre sobre cada programa: o nome real, de qual ícone ele usa
//! e os próprios ícones (PNG). Assim nada precisa ser procurado de novo a cada abertura.
//! Ícones de sites vêm da internet uma única vez por site e ficam guardados.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::tracker::Icon;

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct Cache {
    /// Caminho do executável -> nome do produto.
    pub names: HashMap<String, String>,
    /// Nome exibido -> chave do ícone (caminho do executável, `site:dominio` ou `custom:...`).
    pub icon_of: HashMap<String, String>,
    /// Nome detectado -> nome que o usuário escolheu.
    pub aliases: HashMap<String, String>,
    /// Programas com ícone escolhido pelo usuário: nome exibido -> chave do ícone que havia antes.
    pub custom_icons: HashMap<String, String>,
}

fn dir() -> PathBuf {
    crate::model::data_dir().join("cache")
}

impl Cache {
    fn path() -> PathBuf {
        dir().join("apps.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(dir());
        if let Ok(json) = serde_json::to_string(self) {
            let tmp = Self::path().with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, Self::path());
            }
        }
    }
}

/// Arquivo do ícone: o nome é um hash da chave (caminhos têm caracteres que não servem de nome).
fn icon_file(key: &str) -> PathBuf {
    let hash = key.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
    dir().join(format!("{hash:016x}.png"))
}

pub fn load_icon(key: &str) -> Option<Icon> {
    let img = image::open(icon_file(key)).ok()?.into_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    Some(Icon { key: key.to_owned(), w, h, rgba: img.into_raw() })
}

pub fn save_icon(icon: &Icon) {
    let _ = std::fs::create_dir_all(dir());
    let _ = image::save_buffer(icon_file(&icon.key), &icon.rgba, icon.w as u32, icon.h as u32, image::ColorType::Rgba8);
}

/// Lê uma imagem escolhida pelo usuário (PNG, JPG, ICO ou BMP) como ícone de até 64 px.
pub fn icon_from_file(path: &std::path::Path, key: String) -> Result<Icon, String> {
    let img = image::open(path).map_err(|e| e.to_string())?.thumbnail(64, 64).into_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    Ok(Icon { key, w, h, rgba: img.into_raw() })
}

pub fn remove_icon(key: &str) {
    let _ = std::fs::remove_file(icon_file(key));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icone_volta_igual_do_disco() {
        let key = "teste:ida-e-volta";
        let rgba: Vec<u8> = (0..4 * 4 * 4).map(|i| (i * 3) as u8).collect();
        save_icon(&Icon { key: key.to_owned(), w: 4, h: 4, rgba: rgba.clone() });
        let back = load_icon(key).expect("ícone salvo");
        assert_eq!((back.w, back.h, back.rgba), (4, 4, rgba));
        let _ = std::fs::remove_file(icon_file(key));
    }
}

/// Baixa o ícone do site (uma requisição pequena, feita uma vez e guardada). Usa o `curl` que já vem no Windows.
pub fn fetch_site_icon(domain: &str) -> Option<Icon> {
    use std::os::windows::process::CommandExt;
    let key = format!("site:{domain}");
    let tmp = icon_file(&key).with_extension("dl");
    let _ = std::fs::create_dir_all(dir());
    let ok = std::process::Command::new("curl.exe")
        .args(["-s", "-f", "-L", "--max-time", "8", "-o"])
        .arg(&tmp)
        .arg(format!("https://www.google.com/s2/favicons?domain={domain}&sz=64"))
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .status()
        .is_ok_and(|s| s.success());
    let bytes = if ok { std::fs::read(&tmp).ok() } else { None };
    let _ = std::fs::remove_file(&tmp);
    let img = image::load_from_memory(&bytes?).ok()?.into_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    let icon = Icon { key, w, h, rgba: img.into_raw() };
    save_icon(&icon);
    Some(icon)
}
