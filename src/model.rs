use std::path::PathBuf;

use chrono::{NaiveDate, NaiveTime};
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
}

impl Task {
    pub fn subtasks_done(&self) -> (usize, usize) {
        (self.subtasks.iter().filter(|s| s.done).count(), self.subtasks.len())
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct Store {
    next_id: u64,
    pub tasks: Vec<Task>,
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

    fn path() -> PathBuf {
        data_dir().join("tasks.json")
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
        let mut v: Vec<&Task> = self.tasks.iter().filter(|t| !t.done && t.date == Some(today)).collect();
        v.sort_by_key(|t| t.time);
        v.iter()
            .map(|t| match t.time {
                Some(h) => format!("{}  {}", h.format("%H:%M"), t.title),
                None => t.title.clone(),
            })
            .collect()
    }
}
