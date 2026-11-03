use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{Datelike, Local, NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};

/// Pasta de dados do app (`%APPDATA%\Navi`). Migra a antiga `Des_Atento` na primeira execução.
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let new = base.join("Navi");
    let old = base.join("Des_Atento");
    if !new.exists() && old.exists() {
        let _ = std::fs::rename(&old, &new);
    }
    new
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Subtask {
    pub title: String,
    pub done: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
pub enum Repeat {
    #[default]
    None,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl Repeat {
    pub const ALL: [Repeat; 5] = [Repeat::None, Repeat::Daily, Repeat::Weekly, Repeat::Monthly, Repeat::Yearly];

    pub fn label(self) -> &'static str {
        match self {
            Repeat::None => "Não repete",
            Repeat::Daily => "Diariamente",
            Repeat::Weekly => "Semanalmente",
            Repeat::Monthly => "Mensalmente",
            Repeat::Yearly => "Anualmente",
        }
    }

    pub fn unit(self, n: u32) -> &'static str {
        match (self, n == 1) {
            (Repeat::Daily, true) => "dia",
            (Repeat::Daily, false) => "dias",
            (Repeat::Weekly, true) => "semana",
            (Repeat::Weekly, false) => "semanas",
            (Repeat::Monthly, true) => "mês",
            (Repeat::Monthly, false) => "meses",
            (Repeat::Yearly, true) => "ano",
            (Repeat::Yearly, false) => "anos",
            (Repeat::None, _) => "",
        }
    }
}

fn days_in_month(y: i32, m: u32) -> u32 {
    let next = if m == 12 { NaiveDate::from_ymd_opt(y + 1, 1, 1) } else { NaiveDate::from_ymd_opt(y, m + 1, 1) };
    next.and_then(|n| n.pred_opt()).map_or(28, |d| d.day())
}

/// Soma meses mantendo o dia-âncora; em meses curtos usa o último dia (30 -> 28 em fevereiro).
fn add_months(d: NaiveDate, months: i32, anchor: u32) -> NaiveDate {
    let total = d.year() * 12 + d.month0() as i32 + months;
    let (y, m) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
    NaiveDate::from_ymd_opt(y, m, anchor.min(days_in_month(y, m))).unwrap_or(d)
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Task {
    pub id: u64,
    pub title: String,
    pub notes: String,
    pub done: bool,
    pub subtasks: Vec<Subtask>,
    pub date: Option<NaiveDate>,
    pub time: Option<NaiveTime>,
    /// Minutos de antecedência do aviso (0 = no horário). `None` = sem aviso.
    pub notify: Option<u32>,
    pub tags: Vec<String>,
    pub color: [u8; 3],
    #[serde(default)]
    pub repeat: Repeat,
    /// Repete a cada N dias/semanas/meses/anos.
    #[serde(default = "one")]
    pub every: u32,
    /// Dia do mês de referência da recorrência mensal/anual (o dia 30 volta ao 30 depois de fevereiro).
    #[serde(default)]
    pub anchor_day: u32,
}

fn one() -> u32 {
    1
}

impl Task {
    /// Guarda o dia da data atual como referência da recorrência.
    pub fn set_anchor(&mut self) {
        if let Some(d) = self.date {
            self.anchor_day = d.day();
        }
    }

    fn anchor(&self, d: NaiveDate) -> u32 {
        if self.anchor_day == 0 { d.day() } else { self.anchor_day }
    }

    /// A data seguinte à ocorrência `d`.
    fn next_after(&self, d: NaiveDate) -> NaiveDate {
        let n = self.every.max(1) as i64;
        match self.repeat {
            Repeat::None => d,
            Repeat::Daily => d + chrono::Duration::days(n),
            Repeat::Weekly => d + chrono::Duration::days(7 * n),
            Repeat::Monthly => add_months(d, n as i32, self.anchor(d)),
            Repeat::Yearly => add_months(d, 12 * n as i32, self.anchor(d)),
        }
    }

    /// Ao concluir uma tarefa recorrente, ela volta pendente na próxima data futura.
    /// Retorna `true` se a tarefa foi reagendada.
    pub fn roll_over(&mut self) -> bool {
        if self.repeat == Repeat::None || !self.done {
            return false;
        }
        let today = Local::now().date_naive();
        let mut next = self.next_after(self.date.unwrap_or(today));
        for _ in 0..2000 {
            if next >= today {
                break;
            }
            next = self.next_after(next);
        }
        self.date = Some(next);
        self.done = false;
        self.subtasks.iter_mut().for_each(|s| s.done = false);
        true
    }

    /// A tarefa cai no dia `d`? Considera a data marcada e as repetições futuras.
    pub fn occurs_on(&self, d: NaiveDate) -> bool {
        let Some(start) = self.date else { return false };
        if d == start {
            return true;
        }
        if self.repeat == Repeat::None || d < start {
            return false;
        }
        let n = self.every.max(1) as i64;
        let anchor = self.anchor(start);
        let months = (d.year() - start.year()) as i64 * 12 + d.month0() as i64 - start.month0() as i64;
        let on_day = d.day() == anchor.min(days_in_month(d.year(), d.month()));
        match self.repeat {
            Repeat::None => false,
            Repeat::Daily => (d - start).num_days() % n == 0,
            Repeat::Weekly => (d - start).num_days() % (7 * n) == 0,
            Repeat::Monthly => months % n == 0 && on_day,
            Repeat::Yearly => months % (12 * n) == 0 && on_day,
        }
    }

    pub fn subtasks_done(&self) -> (usize, usize) {
        (self.subtasks.iter().filter(|s| s.done).count(), self.subtasks.len())
    }
}

pub type TagColors = BTreeMap<String, [u8; 3]>;

const TAG_PALETTE: [[u8; 3]; 8] =
    [[110, 142, 251], [236, 112, 99], [88, 196, 141], [240, 178, 70], [176, 124, 230], [80, 190, 210], [230, 120, 170], [160, 170, 90]];

/// Cor da tag: a escolhida pelo usuário ou, na falta dela, uma da paleta (sempre a mesma para o mesmo nome).
pub fn tag_rgb(colors: &TagColors, tag: &str) -> [u8; 3] {
    colors.get(tag).copied().unwrap_or_else(|| {
        let h = tag.bytes().fold(7usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize));
        TAG_PALETTE[h % TAG_PALETTE.len()]
    })
}

#[derive(Serialize, Deserialize, Default)]
pub struct Store {
    next_id: u64,
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub tag_colors: TagColors,
    #[serde(skip)]
    pub dirty: bool,
}

impl Store {
    pub fn add(&mut self, title: String, date: Option<NaiveDate>) -> u64 {
        self.next_id += 1;
        self.tasks.push(Task {
            id: self.next_id,
            title,
            notes: String::new(),
            done: false,
            subtasks: Vec::new(),
            date,
            time: None,
            notify: None,
            tags: Vec::new(),
            color: [110, 142, 251],
            repeat: Repeat::None,
            every: 1,
            anchor_day: 0,
        });
        self.dirty = true;
        self.next_id
    }

    pub fn remove(&mut self, id: u64) {
        self.tasks.retain(|t| t.id != id);
        self.dirty = true;
    }

    pub fn all_tags(&self) -> Vec<String> {
        let mut v: Vec<String> = self.tasks.iter().flat_map(|t| t.tags.iter().cloned()).collect();
        v.sort_by_key(|t| t.to_lowercase());
        v.dedup();
        v
    }

    /// Quantas tarefas usam a tag.
    pub fn tag_count(&self, tag: &str) -> usize {
        self.tasks.iter().filter(|t| t.tags.iter().any(|x| x == tag)).count()
    }

    pub fn set_tag_color(&mut self, tag: &str, rgb: [u8; 3]) {
        self.tag_colors.insert(tag.to_owned(), rgb);
        self.dirty = true;
    }

    /// Renomeia a tag em todas as tarefas; se o novo nome já existe, as duas se juntam.
    pub fn rename_tag(&mut self, from: &str, to: &str) {
        let to = to.trim().trim_start_matches('#').trim();
        if to.is_empty() || to == from {
            return;
        }
        for t in &mut self.tasks {
            if let Some(p) = t.tags.iter().position(|x| x == from) {
                if t.tags.iter().any(|x| x == to) {
                    t.tags.remove(p);
                } else {
                    t.tags[p] = to.to_owned();
                }
            }
        }
        let color = tag_rgb(&self.tag_colors, from);
        self.tag_colors.remove(from);
        self.tag_colors.entry(to.to_owned()).or_insert(color);
        self.dirty = true;
    }

    pub fn delete_tag(&mut self, tag: &str) {
        self.tasks.iter_mut().for_each(|t| t.tags.retain(|x| x != tag));
        self.tag_colors.remove(tag);
        self.dirty = true;
    }

    fn path() -> PathBuf {
        data_dir().join("tasks.json")
    }

    /// Data da última gravação do arquivo de tarefas.
    pub fn modified() -> Option<std::time::SystemTime> {
        std::fs::metadata(Self::path()).and_then(|m| m.modified()).ok()
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&mut self) {
        self.dirty = false;
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }

    /// Títulos das tarefas pendentes de hoje, em ordem de horário.
    pub fn today_titles(&self, today: NaiveDate) -> Vec<String> {
        let mut v: Vec<&Task> = self.tasks.iter().filter(|t| !t.done && t.occurs_on(today)).collect();
        v.sort_by_key(|t| t.time);
        v.iter()
            .map(|t| match t.time {
                Some(h) => format!("{}  {}", h.format("%H:%M"), t.title),
                None => t.title.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rent() -> Task {
        let mut s = Store::default();
        let id = s.add("Aluguel".into(), NaiveDate::from_ymd_opt(2026, 1, 30));
        let mut t = s.tasks.remove(s.tasks.iter().position(|t| t.id == id).unwrap());
        t.repeat = Repeat::Monthly;
        t.set_anchor();
        t
    }

    #[test]
    fn mensal_no_dia_30() {
        let t = rent();
        assert!(t.occurs_on(NaiveDate::from_ymd_opt(2026, 3, 30).unwrap()));
        assert!(t.occurs_on(NaiveDate::from_ymd_opt(2026, 2, 28).unwrap()));
        assert!(!t.occurs_on(NaiveDate::from_ymd_opt(2026, 3, 29).unwrap()));
        assert!(!t.occurs_on(NaiveDate::from_ymd_opt(2025, 12, 30).unwrap()));
    }

    #[test]
    fn renomear_e_remover_tags() {
        let mut s = Store::default();
        let a = s.add("A".into(), None);
        let b = s.add("B".into(), None);
        s.tasks[0].tags = vec!["casa".into(), "casa2".into()];
        s.tasks[1].tags = vec!["casa".into()];
        s.set_tag_color("casa", [1, 2, 3]);
        s.rename_tag("casa", "lar");
        assert_eq!(s.tasks[0].tags, ["lar", "casa2"]);
        assert_eq!(s.tag_colors.get("lar"), Some(&[1, 2, 3]));
        assert!(!s.tag_colors.contains_key("casa"));
        // Renomear para um nome que já existe junta as duas, sem duplicar.
        s.rename_tag("casa2", "lar");
        assert_eq!(s.tasks[0].tags, ["lar"]);
        assert_eq!(s.tag_count("lar"), 2);
        s.delete_tag("lar");
        assert!(s.all_tags().is_empty());
        let _ = (a, b);
    }

    #[test]
    fn proxima_data_volta_ao_30() {
        let t = rent();
        let feb = t.next_after(NaiveDate::from_ymd_opt(2026, 1, 30).unwrap());
        assert_eq!(feb, NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
        assert_eq!(t.next_after(feb), NaiveDate::from_ymd_opt(2026, 3, 30).unwrap());
    }
}

/// Preferências do app (`settings.json`).
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(default)]
pub struct Settings {
    /// Escolha do botão de minimizar: `Some(true)` bandeja, `Some(false)` só minimizar, `None` perguntar.
    pub minimize_to_tray: Option<bool>,
    /// Escala da interface (1.0 = 100%).
    pub scale: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { minimize_to_tray: None, scale: 1.0 }
    }
}

pub const SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.7..=2.0;

impl Settings {
    fn path() -> PathBuf {
        data_dir().join("settings.json")
    }

    pub fn load() -> Self {
        let mut s: Self =
            std::fs::read_to_string(Self::path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        s.scale = if s.scale.is_finite() { s.scale.clamp(*SCALE_RANGE.start(), *SCALE_RANGE.end()) } else { 1.0 };
        s
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(data_dir());
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::path(), json);
        }
    }
}
