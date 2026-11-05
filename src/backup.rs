//! Backup dos dados para uma pasta escolhida pelo usuário, de preferência uma que o Google Drive,
//! o OneDrive ou o Dropbox já sincronizam: o próprio programa de nuvem cuida do envio, sem login,
//! senha ou chave de API dentro do Navi, e funciona offline (o envio acontece quando a internet voltar).
//!
//! Cada backup é um .zip comum, por dia, que pode ser aberto no Explorer. O arquivo nasce com um nome
//! temporário e só ganha o nome final depois de conferido: a nuvem nunca recebe um backup pela metade.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Local, NaiveDateTime};
use eframe::egui::{self, Color32, RichText};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::model::{Settings, Store, data_dir};

/// Quantos backups diários ficam na pasta.
const KEEP: usize = 30;
/// Com as tarefas mudando, novo backup no mesmo dia só depois deste intervalo (e sempre ao sair).
const MIN_GAP_MINUTES: i64 = 30;
/// Com que frequência o app confere se está na hora de um backup.
pub const CHECK_SECS: f64 = 600.0;
/// Espera depois de abrir o app: o Google Drive pode ainda estar montando a unidade dele.
const FIRST_CHECK_SECS: f64 = 30.0;
/// Falhas seguidas antes de avisar: uma unidade de rede ainda não montada não merece alarme.
const WARN_AFTER: u32 = 3;
const PREFIX: &str = "Navi_backup_";
const SAFETY_PREFIX: &str = "Navi_antes_de_restaurar_";
const FOLDER_NAME: &str = "Navi Backup";
const TASKS: &str = "tasks.json";
const STAGING: &str = ".restaurar";

/// Só o que é dado do usuário: os JSON e os ícones guardados. Arquivos temporários ficam de fora.
fn is_data_file(name: &str) -> bool {
    !name.starts_with('.') && name != "toast.png" && (name.ends_with(".json") || name.ends_with(".png"))
}

/// Arquivos a guardar, como (nome no zip, caminho no disco).
fn collect(data: &Path) -> io::Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    for (prefix, dir) in [("", data.to_path_buf()), ("cache/", data.join("cache"))] {
        if !dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type()?.is_file() && is_data_file(&name) {
                out.push((format!("{prefix}{name}"), entry.path()));
            }
        }
    }
    Ok(out)
}

/// Lê tudo de volta: o CRC de cada arquivo é conferido na leitura.
fn verify(path: &Path) -> Result<(), String> {
    let mut archive = ZipArchive::new(File::open(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        io::copy(&mut entry, &mut io::sink()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Grava `dir/name` com os dados de `data`. Se algo falhar, o backup anterior de mesmo nome continua lá.
fn write_zip(dir: &Path, name: &str, data: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Não foi possível acessar a pasta: {e}"))?;
    let tmp = dir.join(format!("{name}.tmp"));
    let result = (|| -> Result<(), String> {
        let file = File::create(&tmp).map_err(|e| format!("Não foi possível gravar na pasta: {e}"))?;
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (entry, path) in collect(data).map_err(|e| e.to_string())? {
            let bytes = std::fs::read(&path).map_err(|e| format!("{entry}: {e}"))?;
            zip.start_file(entry, options).map_err(|e| e.to_string())?;
            zip.write_all(&bytes).map_err(|e| e.to_string())?;
        }
        let file = zip.finish().map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        verify(&tmp)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    let path = dir.join(name);
    std::fs::rename(&tmp, &path).map_err(|e| format!("Não foi possível concluir o arquivo: {e}"))?;
    Ok(path)
}

/// Apaga os backups diários mais antigos, deixando só os últimos `KEEP`.
fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(PREFIX) && n.ends_with(".zip"))
        .collect();
    // O nome traz a data em AAAA-MM-DD: a ordem alfabética é a cronológica.
    names.sort_unstable_by(|a, b| b.cmp(a));
    for old in names.iter().skip(KEEP) {
        let _ = std::fs::remove_file(dir.join(old));
    }
}

/// O backup do dia: um arquivo por data, substituído se houver outro no mesmo dia.
pub fn create(dir: &Path, data: &Path, now: NaiveDateTime) -> Result<PathBuf, String> {
    let path = write_zip(dir, &format!("{PREFIX}{}.zip", now.format("%Y-%m-%d")), data)?;
    prune(dir);
    Ok(path)
}

/// Está na hora de um backup automático?
pub fn due(last: Option<NaiveDateTime>, changed: Option<NaiveDateTime>, now: NaiveDateTime) -> bool {
    let Some(last) = last else { return true };
    if last.date() != now.date() {
        return true;
    }
    changed_since(Some(last), changed) && now - last >= Duration::minutes(MIN_GAP_MINUTES)
}

/// As tarefas mudaram depois do último backup?
pub fn changed_since(last: Option<NaiveDateTime>, changed: Option<NaiveDateTime>) -> bool {
    match (last, changed) {
        (Some(last), Some(changed)) => changed > last,
        (None, _) => true,
        (_, None) => false,
    }
}

/// Quando as tarefas foram gravadas pela última vez.
pub fn tasks_changed_at() -> Option<NaiveDateTime> {
    Store::modified().map(|t| DateTime::<Local>::from(t).naive_local())
}

/// Backup já extraído e conferido, pronto para substituir os dados quando o app fechar.
pub struct Prepared {
    pub source: PathBuf,
    pub tasks: usize,
    staging: PathBuf,
}

/// Abre o backup e o extrai para uma pasta à parte, sem tocar nos dados atuais.
/// Um arquivo que não seja um backup do Navi é recusado aqui.
pub fn prepare(source: &Path, data: &Path) -> Result<Prepared, String> {
    let invalid = |e: String| format!("Este arquivo não é um backup válido do Navi ({e}).");
    let mut archive = ZipArchive::new(File::open(source).map_err(|e| invalid(e.to_string()))?).map_err(|e| invalid(e.to_string()))?;
    let staging = data.join(STAGING);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(staging.join("cache")).map_err(|e| e.to_string())?;
    let mut tasks = None;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| invalid(e.to_string()))?;
        let name = entry.name().map_err(|e| invalid(e.to_string()))?.replace('\\', "/");
        // Só os nomes que o próprio backup cria: nada de subpastas nem de "..", que escapariam da pasta de dados.
        let (folder, file) = match name.split_once('/') {
            Some(("cache", file)) => (Some("cache"), file),
            None => (None, name.as_str()),
            _ => continue,
        };
        if file.contains(['/', ':']) || file.contains("..") || !is_data_file(file) {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| invalid(e.to_string()))?;
        if name == TASKS {
            let store: Store = serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
            tasks = Some(store.tasks.len());
        }
        let target = folder.map_or_else(|| staging.join(file), |f| staging.join(f).join(file));
        std::fs::write(target, bytes).map_err(|e| e.to_string())?;
    }
    let Some(tasks) = tasks else {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(invalid("não há tarefas nele".to_owned()));
    };
    Ok(Prepared { source: source.to_path_buf(), tasks, staging })
}

/// Coloca os arquivos extraídos no lugar. Roda com o app fechando, quando nada mais grava neles.
pub fn commit(prepared: &Prepared, data: &Path) -> io::Result<()> {
    std::fs::create_dir_all(data.join("cache"))?;
    for (sub, dest) in [(prepared.staging.clone(), data.to_path_buf()), (prepared.staging.join("cache"), data.join("cache"))] {
        for entry in std::fs::read_dir(&sub)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                std::fs::rename(entry.path(), dest.join(entry.file_name()))?;
            }
        }
    }
    std::fs::remove_dir_all(&prepared.staging)
}

/// Pastas de programas de nuvem encontradas nesta máquina.
pub fn cloud_folders() -> Vec<(&'static str, PathBuf)> {
    let mut found = Vec::new();
    // O Google Drive para computador monta uma unidade própria, normalmente G:.
    'drive: for letter in 'C'..='Z' {
        for name in ["Meu Drive", "My Drive"] {
            let path = PathBuf::from(format!("{letter}:\\{name}"));
            if path.is_dir() {
                found.push(("Google Drive", path));
                break 'drive;
            }
        }
    }
    for var in ["OneDriveConsumer", "OneDrive"] {
        if let Some(path) = std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_dir()) {
            found.push(("OneDrive", path));
            break;
        }
    }
    if let Some(path) = std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Dropbox")).filter(|p| p.is_dir()) {
        found.push(("Dropbox", path));
    }
    found
}

fn when_label(at: NaiveDateTime, now: NaiveDateTime) -> String {
    match (now.date() - at.date()).num_days() {
        0 => format!("hoje às {}", at.format("%H:%M")),
        1 => format!("ontem às {}", at.format("%H:%M")),
        _ => at.format("%d/%m/%Y às %H:%M").to_string(),
    }
}

/// O que a janela pediu ao app.
pub enum Request {
    BackupNow,
    Restore(Prepared),
}

/// Situação para o ícone da barra superior.
#[derive(PartialEq, Clone, Copy)]
pub enum Health {
    Ok,
    Off,
    Failing,
}

#[derive(Default)]
struct Job {
    running: bool,
    done: Option<Result<NaiveDateTime, String>>,
}

/// Backups em segundo plano (a unidade da nuvem pode ser lenta) e a janela de configuração.
pub struct Backup {
    job: Arc<Mutex<Job>>,
    pub open: bool,
    /// Instante, no relógio da interface, da próxima conferência.
    next_check: f64,
    failures: u32,
    warned: bool,
    error: Option<String>,
    clouds: Option<Vec<(&'static str, PathBuf)>>,
    confirm: Option<Prepared>,
}

impl Backup {
    pub fn new() -> Self {
        Self {
            job: Arc::default(),
            open: false,
            next_check: FIRST_CHECK_SECS,
            failures: 0,
            warned: false,
            error: None,
            clouds: None,
            confirm: None,
        }
    }

    pub fn health(&self, settings: &Settings) -> Health {
        match (&settings.backup_dir, &self.error) {
            (None, _) => Health::Off,
            (_, Some(_)) => Health::Failing,
            _ => Health::Ok,
        }
    }

    pub fn running(&self) -> bool {
        self.job.lock().unwrap().running
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
    }

    /// Hora de conferir? Já deixa marcada a próxima vez.
    pub fn check_now(&mut self, time: f64) -> bool {
        if time < self.next_check {
            return false;
        }
        self.next_check = time + CHECK_SECS;
        true
    }

    /// Quanto falta para a próxima conferência: até lá o app pode ficar parado.
    pub fn until_check(&self, time: f64) -> std::time::Duration {
        std::time::Duration::from_secs_f64((self.next_check - time).max(1.0))
    }

    /// Faz o backup em outra thread; a tela nunca espera a unidade da nuvem.
    pub fn start(&self, dir: PathBuf, ctx: &egui::Context) {
        {
            let mut job = self.job.lock().unwrap();
            if job.running {
                return;
            }
            job.running = true;
        }
        let job = Arc::clone(&self.job);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let now = Local::now().naive_local();
            let result = create(&dir, &data_dir(), now).map(|_| now);
            let mut job = job.lock().unwrap();
            job.running = false;
            job.done = Some(result);
            ctx.request_repaint();
        });
    }

    /// Recolhe o resultado do último backup, se já terminou. Retorna a data de um backup bem-sucedido.
    pub fn take_finished(&mut self) -> Option<NaiveDateTime> {
        let done = self.job.lock().unwrap().done.take()?;
        match done {
            Ok(at) => {
                self.error = None;
                self.failures = 0;
                self.warned = false;
                Some(at)
            }
            Err(e) => {
                self.failures += 1;
                // Backup que falha em silêncio é o pior dos mundos: com o app na bandeja, o aviso precisa sair da janela.
                if self.failures >= WARN_AFTER && !self.warned {
                    self.warned = true;
                    crate::notify::toast("O backup do Navi não está funcionando", &e);
                }
                self.error = Some(e);
                None
            }
        }
    }

    /// Janela de backup. Retorna o que precisa do app para acontecer.
    pub fn window(&mut self, ctx: &egui::Context, settings: &mut Settings) -> Option<Request> {
        if !self.open {
            self.clouds = None;
            return None;
        }
        let clouds = self.clouds.get_or_insert_with(cloud_folders).clone();
        let running = self.running();
        let now = Local::now().naive_local();
        let mut request = None;
        let mut open = true;
        let mut choose = None;
        let mut pick_folder = false;
        let mut pick_file = false;
        let mut turn_off = false;
        egui::Window::new("Backup")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(430.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                if let Some(prepared) = &self.confirm {
                    let name = prepared.source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    ui.label(RichText::new("Restaurar este backup?").strong());
                    ui.label(format!("{name}, com {} tarefas.", prepared.tasks));
                    ui.weak("Os dados de agora serão substituídos, mas antes ganham uma cópia de segurança na mesma pasta. O Navi reinicia sozinho.");
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Restaurar e reiniciar").clicked() {
                            request = self.confirm.take().map(Request::Restore);
                        } else if ui.button("Cancelar").clicked() {
                            self.confirm = None;
                        }
                    });
                    return;
                }

                let (color, status) = match (&settings.backup_dir, &self.error, settings.last_backup) {
                    (None, ..) => (Color32::from_rgb(240, 180, 90), "Backup desligado. Escolha uma pasta para ativar.".to_owned()),
                    _ if running => (crate::theme::ACCENT, "Fazendo backup...".to_owned()),
                    (_, Some(e), _) => (Color32::LIGHT_RED, format!("A última tentativa falhou. {e}")),
                    (_, None, Some(at)) => (Color32::from_rgb(90, 200, 130), format!("Último backup: {}.", when_label(at, now))),
                    (_, None, None) => (crate::theme::ACCENT, "Primeiro backup em instantes.".to_owned()),
                };
                ui.label(RichText::new(status).color(color));
                ui.add_space(4.0);

                if let Some(dir) = &settings.backup_dir {
                    ui.label(RichText::new("Pasta").size(12.5).color(Color32::from_gray(150)));
                    ui.add(egui::Label::new(dir.to_string_lossy().into_owned()).wrap());
                }
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for (name, path) in &clouds {
                        let target = path.join(FOLDER_NAME);
                        let current = settings.backup_dir.as_ref() == Some(&target);
                        if ui.selectable_label(current, *name).on_hover_text(target.to_string_lossy().into_owned()).clicked() && !current {
                            choose = Some(target);
                        }
                    }
                    pick_folder = ui.button("Outra pasta...").clicked();
                });
                if settings.backup_dir.is_none() {
                    ui.add_space(2.0);
                    if clouds.is_empty() {
                        ui.weak("Nenhum programa de nuvem encontrado. Instale o Google Drive para computador ou escolha uma pasta sincronizada.");
                    } else {
                        ui.weak("Uma pasta de nuvem manda o backup para fora deste computador sozinha, sem login no Navi.");
                    }
                }

                if settings.backup_dir.is_some() {
                    ui.add_space(4.0);
                    ui.weak(format!("Um arquivo por dia, automático, guardando os últimos {KEEP} dias."));
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(!running, egui::Button::new("Fazer backup agora")).clicked() {
                            request = Some(Request::BackupNow);
                        }
                        if ui.button("Abrir pasta").clicked() {
                            if let Some(dir) = &settings.backup_dir {
                                let _ = std::process::Command::new("explorer.exe").arg(dir).spawn();
                            }
                        }
                        turn_off = ui.add(egui::Button::new("Desligar").frame(false)).clicked();
                    });
                }
                ui.add_space(4.0);
                ui.separator();
                pick_file = ui.button("Restaurar um backup...").clicked();
            });
        self.open = open;
        if !open {
            self.confirm = None;
        }
        if pick_folder {
            let mut dialog = rfd::FileDialog::new().set_title("Pasta do backup");
            if let Some(dir) = &settings.backup_dir {
                dialog = dialog.set_directory(dir);
            }
            choose = dialog.pick_folder().or(choose);
        }
        if let Some(dir) = choose {
            settings.backup_dir = Some(dir);
            settings.last_backup = None;
            settings.save();
            self.error = None;
            self.failures = 0;
            // Ao ativar, já faz o primeiro backup e mostra se a pasta funciona.
            request = Some(Request::BackupNow);
        }
        if turn_off {
            settings.backup_dir = None;
            settings.last_backup = None;
            settings.save();
            self.error = None;
        }
        if pick_file {
            let mut dialog = rfd::FileDialog::new().add_filter("Backup do Navi", &["zip"]).set_title("Restaurar um backup");
            if let Some(dir) = &settings.backup_dir {
                dialog = dialog.set_directory(dir);
            }
            if let Some(path) = dialog.pick_file() {
                match prepare(&path, &data_dir()) {
                    Ok(prepared) => self.confirm = Some(prepared),
                    Err(e) => self.error = Some(e),
                }
            }
        }
        request
    }
}

/// Cópia dos dados atuais antes de restaurar, ao lado do backup que será usado.
pub fn safety_copy(prepared: &Prepared, settings: &Settings, data: &Path, now: NaiveDateTime) -> Result<PathBuf, String> {
    let dir = settings.backup_dir.clone().or_else(|| prepared.source.parent().map(Path::to_path_buf)).ok_or("Pasta de backup indefinida.")?;
    write_zip(&dir, &format!("{SAFETY_PREFIX}{}.zip", now.format("%Y-%m-%d_%H%M%S")), data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(day: u32, h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap().and_hms_opt(h, m, 0).unwrap()
    }

    /// Pasta temporária só deste teste, com alguns dados de exemplo.
    fn sandbox(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("navi_backup_teste_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        let (data, dest) = (root.join("dados"), root.join("nuvem"));
        std::fs::create_dir_all(data.join("cache")).unwrap();
        let mut store = Store::default();
        store.add("Primeira".into(), None);
        std::fs::write(data.join(TASKS), serde_json::to_string(&store).unwrap()).unwrap();
        std::fs::write(data.join("usage.json"), "{\"days\":{}}").unwrap();
        std::fs::write(data.join("toast.png"), "fora do backup").unwrap();
        std::fs::write(data.join("tasks.json.tmp"), "temporário").unwrap();
        std::fs::write(data.join("cache").join("abc.png"), [1u8, 2, 3]).unwrap();
        (root, data, dest)
    }

    #[test]
    fn ida_e_volta_do_backup() {
        let (root, data, dest) = sandbox("ida_e_volta");
        let path = create(&dest, &data, at(5, 10, 0)).unwrap();
        assert!(path.ends_with("Navi_backup_2026-11-05.zip"));
        assert!(!dest.join("Navi_backup_2026-11-05.zip.tmp").exists());
        verify(&path).unwrap();

        // Os dados mudam ou se perdem; o backup traz tudo de volta.
        std::fs::write(data.join(TASKS), "{\"next_id\":9,\"tasks\":[]}").unwrap();
        std::fs::remove_file(data.join("cache").join("abc.png")).unwrap();
        let prepared = prepare(&path, &data).unwrap();
        assert_eq!(prepared.tasks, 1);
        // Preparar não mexe nos dados atuais.
        assert!(std::fs::read_to_string(data.join(TASKS)).unwrap().contains("\"next_id\":9"));
        commit(&prepared, &data).unwrap();
        assert!(std::fs::read_to_string(data.join(TASKS)).unwrap().contains("Primeira"));
        assert_eq!(std::fs::read(data.join("cache").join("abc.png")).unwrap(), [1, 2, 3]);
        assert!(!data.join(STAGING).exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn so_entra_o_que_e_dado() {
        let (root, data, _) = sandbox("so_dados");
        let mut names: Vec<String> = collect(&data).unwrap().into_iter().map(|c| c.0).collect();
        names.sort();
        assert_eq!(names, ["cache/abc.png", "tasks.json", "usage.json"]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn guarda_so_os_ultimos_dias() {
        let (root, data, dest) = sandbox("poda");
        std::fs::create_dir_all(&dest).unwrap();
        for day in 1..=(KEEP + 5) {
            std::fs::write(dest.join(format!("{PREFIX}2026-01-{day:02}.zip")), "x").unwrap();
        }
        std::fs::write(dest.join(format!("{SAFETY_PREFIX}2026-01-01_000000.zip")), "x").unwrap();
        std::fs::write(dest.join("outro arquivo.zip"), "x").unwrap();
        create(&dest, &data, at(5, 10, 0)).unwrap();
        let left: Vec<String> = std::fs::read_dir(&dest).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(left.iter().filter(|n| n.starts_with(PREFIX)).count(), KEEP);
        assert!(left.iter().any(|n| n == "Navi_backup_2026-11-05.zip"));
        assert!(!left.iter().any(|n| n == "Navi_backup_2026-01-01.zip"));
        // O que não é backup diário nunca é apagado.
        assert!(left.iter().any(|n| n.starts_with(SAFETY_PREFIX)));
        assert!(left.iter().any(|n| n == "outro arquivo.zip"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn recusa_o_que_nao_e_backup_e_nao_escapa_da_pasta() {
        let (root, data, dest) = sandbox("recusa");
        std::fs::create_dir_all(&dest).unwrap();
        let not_zip = dest.join("foto.zip");
        std::fs::write(&not_zip, "não sou um zip").unwrap();
        assert!(prepare(&not_zip, &data).is_err());

        // Zip sem tarefas e com um nome que tenta sair da pasta de dados.
        let hostile = dest.join("hostil.zip");
        let mut zip = ZipWriter::new(File::create(&hostile).unwrap());
        zip.start_file("../fora.json", SimpleFileOptions::default()).unwrap();
        zip.write_all(b"{}").unwrap();
        zip.finish().unwrap();
        assert!(prepare(&hostile, &data).is_err());
        assert!(!data.parent().unwrap().join("fora.json").exists());

        // Tarefas ilegíveis também são recusadas, e os dados atuais continuam como estavam.
        let broken = dest.join("quebrado.zip");
        let mut zip = ZipWriter::new(File::create(&broken).unwrap());
        zip.start_file(TASKS, SimpleFileOptions::default()).unwrap();
        zip.write_all(b"isto nao e json").unwrap();
        zip.finish().unwrap();
        assert!(prepare(&broken, &data).is_err());
        assert!(std::fs::read_to_string(data.join(TASKS)).unwrap().contains("Primeira"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn falha_na_pasta_preserva_o_backup_anterior() {
        let (root, data, dest) = sandbox("falha");
        let path = create(&dest, &data, at(5, 10, 0)).unwrap();
        let before = std::fs::read(&path).unwrap();
        // Um arquivo no caminho impede criar a pasta de destino.
        let not_dir = root.join("arquivo");
        std::fs::write(&not_dir, "x").unwrap();
        assert!(create(&not_dir.join("sub"), &data, at(5, 11, 0)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn quando_fazer_backup() {
        let now = at(5, 12, 0);
        assert!(due(None, None, now), "nunca fez");
        assert!(due(Some(at(4, 23, 0)), None, now), "ainda não fez hoje");
        assert!(!due(Some(at(5, 9, 0)), Some(at(5, 8, 0)), now), "nada mudou depois do backup");
        assert!(due(Some(at(5, 9, 0)), Some(at(5, 10, 0)), now), "mudou e já faz tempo");
        assert!(!due(Some(at(5, 11, 50)), Some(at(5, 11, 55)), now), "mudou, mas o último foi há pouco");
        assert!(changed_since(Some(at(5, 11, 50)), Some(at(5, 11, 55))), "ao sair, vale qualquer mudança");
        assert!(!changed_since(Some(at(5, 11, 50)), Some(at(5, 11, 40))));
    }
}
