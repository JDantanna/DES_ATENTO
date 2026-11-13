use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};

/// Pasta de dados do app (`%APPDATA%\Navi`). Migra a antiga `Des_Atento` na primeira execução.
pub fn data_dir() -> PathBuf {
    // Os testes nunca tocam nos dados de verdade de quem os roda.
    if cfg!(test) {
        return std::env::temp_dir().join("navi_testes");
    }
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
    /// Avisar também no dia anterior (na mesma hora, ou às 9h se a tarefa não tem hora).
    #[serde(default)]
    pub notify_day_before: bool,
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
    /// Repetição finita: quantas ocorrências faltam, contando a da data atual. `None` = para sempre.
    #[serde(default)]
    pub repeat_times: Option<u32>,
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
    /// Em repetição finita, a última ocorrência concluída fica concluída de vez.
    /// Retorna `true` se a tarefa foi reagendada.
    pub fn roll_over(&mut self) -> bool {
        if self.repeat == Repeat::None || !self.done {
            return false;
        }
        let today = Local::now().date_naive();
        // Ocorrências que ainda restam depois desta (as puladas por atraso também contam).
        let mut left = self.repeat_times.map(|n| n as i64 - 1);
        let mut next = self.next_after(self.date.unwrap_or(today));
        for _ in 0..2000 {
            if left.is_some_and(|n| n <= 0) {
                self.repeat_times = Some(1);
                return false;
            }
            if next >= today {
                break;
            }
            left = left.map(|n| n - 1);
            next = self.next_after(next);
        }
        if left.is_some_and(|n| n <= 0) {
            self.repeat_times = Some(1);
            return false;
        }
        self.repeat_times = left.map(|n| n as u32);
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
        // Número da ocorrência (0 = a da data de início), se `d` for um dia em que a tarefa repete.
        let nth = match self.repeat {
            Repeat::None => None,
            Repeat::Daily => ((d - start).num_days() % n == 0).then(|| (d - start).num_days() / n),
            Repeat::Weekly => ((d - start).num_days() % (7 * n) == 0).then(|| (d - start).num_days() / (7 * n)),
            Repeat::Monthly => (months % n == 0 && on_day).then(|| months / n),
            Repeat::Yearly => (months % (12 * n) == 0 && on_day).then(|| months / (12 * n)),
        };
        nth.is_some_and(|k| self.repeat_times.is_none_or(|total| k < total as i64))
    }

    /// Pendente e com a data (e hora, se houver) já passada.
    pub fn overdue(&self, now: NaiveDateTime) -> bool {
        if self.done {
            return false;
        }
        match (self.date, self.time) {
            (Some(d), Some(t)) => d.and_time(t) < now,
            (Some(d), None) => d < now.date(),
            _ => false,
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
            notify_day_before: false,
            tags: Vec::new(),
            color: [110, 142, 251],
            repeat: Repeat::None,
            every: 1,
            anchor_day: 0,
            repeat_times: None,
        });
        self.dirty = true;
        self.next_id
    }

    /// Ordena as tarefas pela hora (as sem hora ficam no fim); a ordem passa a ser a da lista.
    pub fn sort_by_time(&mut self, descending: bool) {
        use std::cmp::Ordering;
        self.tasks.sort_by(|a, b| match (a.time, b.time) {
            (Some(x), Some(y)) if descending => y.cmp(&x),
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        });
        self.dirty = true;
    }

    /// Leva a tarefa `id` para a posição da tarefa `target` (as do meio se deslocam).
    pub fn move_task(&mut self, id: u64, target: u64) {
        let (Some(from), Some(to)) =
            (self.tasks.iter().position(|t| t.id == id), self.tasks.iter().position(|t| t.id == target))
        else {
            return;
        };
        if from != to {
            let t = self.tasks.remove(from);
            self.tasks.insert(to, t);
            self.dirty = true;
        }
    }

    pub fn remove(&mut self, id: u64) {
        self.tasks.retain(|t| t.id != id);
        self.dirty = true;
    }

    /// Desfaz uma mudança: a tarefa volta como estava e, se foi excluída, para a posição de antes.
    pub fn restore(&mut self, task: Task, index: usize) {
        if let Some(p) = self.tasks.iter().position(|t| t.id == task.id) {
            self.tasks[p] = task;
        } else {
            self.tasks.insert(index.min(self.tasks.len()), task);
        }
        self.dirty = true;
    }

    /// Tarefas pendentes que pedem atenção hoje, como índices: as atrasadas primeiro, depois as de hoje
    /// por hora (as sem hora no fim).
    pub fn today_pending(&self, today: NaiveDate, now: NaiveDateTime) -> Vec<usize> {
        let mut v: Vec<usize> = (0..self.tasks.len())
            .filter(|&i| {
                let t = &self.tasks[i];
                !t.done && (t.overdue(now) || t.occurs_on(today))
            })
            .collect();
        v.sort_by_key(|&i| {
            let t = &self.tasks[i];
            (!t.overdue(now), t.date, t.time.is_none(), t.time, t.id)
        });
        v
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
        let path = Self::path();
        let Ok(text) = std::fs::read_to_string(&path) else { return Self::default() };
        serde_json::from_str(&text).unwrap_or_else(|_| {
            // Arquivo ilegível: guarda uma cópia antes que a próxima gravação o substitua por uma lista vazia.
            let stamp = Local::now().format("%Y%m%d_%H%M%S");
            let _ = std::fs::copy(&path, path.with_file_name(format!("tasks_ilegivel_{stamp}.json")));
            Self::default()
        })
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

    /// Quantas tarefas de dias anteriores seguem pendentes (as que também caem hoje já estão na lista de hoje).
    pub fn pending_before(&self, today: NaiveDate) -> usize {
        self.tasks.iter().filter(|t| !t.done && t.date.is_some_and(|d| d < today) && !t.occurs_on(today)).count()
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
    fn repeticao_finita_de_3_meses() {
        let mut t = rent();
        t.date = NaiveDate::from_ymd_opt(2026, 1, 5);
        t.set_anchor();
        t.repeat_times = Some(3);
        let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        assert!(t.occurs_on(d(1, 5)) && t.occurs_on(d(2, 5)) && t.occurs_on(d(3, 5)));
        assert!(!t.occurs_on(d(4, 5)) && !t.occurs_on(d(5, 5)));
        t.repeat_times = None;
        assert!(t.occurs_on(d(4, 5)));
    }

    #[test]
    fn ultima_ocorrencia_conclui_de_vez() {
        let mut t = rent();
        t.date = Some(Local::now().date_naive());
        t.repeat = Repeat::Daily;
        t.repeat_times = Some(2);
        t.done = true;
        assert!(t.roll_over()); // sobra uma ocorrência
        assert_eq!(t.repeat_times, Some(1));
        assert!(!t.done);
        t.done = true;
        assert!(!t.roll_over()); // era a última
        assert!(t.done);
    }

    #[test]
    fn ordem_manual_e_por_hora() {
        let mut s = Store::default();
        let ids: Vec<u64> = ["a", "b", "c"].iter().map(|t| s.add((*t).into(), None)).collect();
        s.tasks[0].time = NaiveTime::from_hms_opt(15, 0, 0);
        s.tasks[2].time = NaiveTime::from_hms_opt(9, 0, 0);
        let titles = |s: &Store| s.tasks.iter().map(|t| t.title.clone()).collect::<Vec<_>>();
        s.sort_by_time(false);
        assert_eq!(titles(&s), ["c", "a", "b"]); // sem hora vai para o fim
        s.sort_by_time(true);
        assert_eq!(titles(&s), ["a", "c", "b"]);
        s.move_task(ids[1], ids[0]); // "b" sobe para a posição de "a"
        assert_eq!(titles(&s), ["b", "a", "c"]);
        s.move_task(ids[1], ids[2]); // e desce para a de "c"
        assert_eq!(titles(&s), ["a", "c", "b"]);
    }

    #[test]
    fn lista_de_hoje_poe_atrasadas_primeiro_e_ordena_por_hora() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 10).unwrap();
        let now = today.and_hms_opt(12, 0, 0).unwrap();
        let mut s = Store::default();
        let ids: Vec<u64> = ["sem hora", "tarde", "cedo", "ontem", "feita", "amanhã"].iter().map(|t| s.add((*t).into(), Some(today))).collect();
        s.tasks[1].time = NaiveTime::from_hms_opt(18, 0, 0);
        s.tasks[2].time = NaiveTime::from_hms_opt(13, 0, 0);
        s.tasks[3].date = NaiveDate::from_ymd_opt(2026, 5, 9);
        s.tasks[4].done = true;
        s.tasks[5].date = NaiveDate::from_ymd_opt(2026, 5, 11);
        let titles: Vec<&str> = s.today_pending(today, now).into_iter().map(|i| s.tasks[i].title.as_str()).collect();
        assert_eq!(titles, ["ontem", "cedo", "tarde", "sem hora"]);
        let _ = ids;
    }

    #[test]
    fn widget_migra_do_formato_antigo_e_limita_a_opacidade() {
        let mut s: Settings = serde_json::from_str("{\"widget_small\": true}").unwrap();
        s.normalize();
        assert_eq!(s.widget_format, Some(WidgetFormat::Pequeno));
        let mut s: Settings = serde_json::from_str("{}").unwrap();
        s.normalize();
        assert_eq!(s.widget_prefs(), WidgetPrefs { format: WidgetFormat::Medio, locked: false, opacity: 1.0 });
        // Uma escolha nova vale mais que o campo antigo, e a opacidade fica entre 30% e 100%.
        let mut s: Settings = serde_json::from_str("{\"widget_small\": true, \"widget_format\": \"Grande\", \"widget_opacity\": 0.05}").unwrap();
        s.normalize();
        assert_eq!(s.widget_format, Some(WidgetFormat::Grande));
        assert!((s.widget_opacity - 0.3).abs() < 1e-6);
        s.widget_opacity = f32::NAN;
        s.normalize();
        assert_eq!(s.widget_opacity, 1.0);
        // Gravar e ler de volta mantém a escolha.
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.widget_format, Some(WidgetFormat::Grande));
    }

    #[test]
    fn desfazer_devolve_a_tarefa_ao_lugar() {
        let mut s = Store::default();
        let ids: Vec<u64> = ["a", "b", "c"].iter().map(|t| s.add((*t).into(), None)).collect();
        let copy = s.tasks[1].clone();
        s.remove(ids[1]);
        s.restore(copy.clone(), 1);
        assert_eq!(s.tasks[1].title, "b");
        s.tasks[1].done = true;
        s.restore(copy, 1); // já existe: só volta ao estado de antes
        assert!(!s.tasks[1].done);
        assert_eq!(s.tasks.len(), 3);
    }

    #[test]
    fn atrasada_so_se_pendente_e_no_passado() {
        let now = NaiveDate::from_ymd_opt(2026, 5, 10).unwrap().and_hms_opt(12, 0, 0).unwrap();
        let mut t = rent();
        t.date = NaiveDate::from_ymd_opt(2026, 5, 9);
        assert!(t.overdue(now));
        t.date = NaiveDate::from_ymd_opt(2026, 5, 10);
        assert!(!t.overdue(now)); // hoje sem hora ainda não atrasou
        t.time = NaiveTime::from_hms_opt(11, 0, 0);
        assert!(t.overdue(now));
        t.done = true;
        assert!(!t.overdue(now));
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

/// Formatos padrão do widget flutuante, do mais fino ao maior.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WidgetFormat {
    /// Faixa fina: só o ícone do programa e o cronômetro.
    Barra,
    /// Faixa com o programa, o cronômetro e os botões.
    Pequeno,
    #[default]
    Medio,
    Grande,
}

impl WidgetFormat {
    pub const ALL: [WidgetFormat; 4] = [WidgetFormat::Barra, WidgetFormat::Pequeno, WidgetFormat::Medio, WidgetFormat::Grande];

    pub fn label(self) -> &'static str {
        match self {
            WidgetFormat::Barra => "Barra",
            WidgetFormat::Pequeno => "Pequeno",
            WidgetFormat::Medio => "Médio",
            WidgetFormat::Grande => "Grande",
        }
    }
}

/// Escolhas do widget que ficam gravadas: formato, tamanho trancado e opacidade.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct WidgetPrefs {
    pub format: WidgetFormat,
    /// Trancado: não dá para redimensionar a janela do widget.
    pub locked: bool,
    /// De 0.3 (bem transparente) a 1.0 (opaco).
    pub opacity: f32,
}

/// Opacidade mínima do widget: abaixo disso ele some da tela e ninguém acha de novo.
pub const OPACITY_RANGE: std::ops::RangeInclusive<f32> = 0.3..=1.0;

/// Preferências do app (`settings.json`).
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    /// Antigo: o botão de minimizar perguntava bandeja ou só minimizar. Agora minimizar vai sempre para a
    /// bandeja (com o widget aberto o Navi não fica na barra de tarefas); o campo só é lido de arquivos antigos.
    pub minimize_to_tray: Option<bool>,
    /// Escala da interface (1.0 = 100%).
    pub scale: f32,
    /// O aviso "continua na bandeja" já foi mostrado (só aparece na primeira vez).
    pub tray_notice_shown: bool,
    /// Antigo: o widget usava o formato pequeno. Só é lido de arquivos antigos, para migrar para `widget_format`.
    #[serde(skip_serializing)]
    pub widget_small: bool,
    /// Formato do widget; `None` em arquivos antigos (vale então o campo `widget_small`).
    pub widget_format: Option<WidgetFormat>,
    /// Widget com o tamanho trancado.
    pub widget_locked: bool,
    /// Opacidade do widget (0.3 a 1.0).
    pub widget_opacity: f32,
    /// Pasta dos backups (normalmente sincronizada com a nuvem). `None` = backup desligado.
    pub backup_dir: Option<PathBuf>,
    /// Quando o último backup deu certo.
    pub last_backup: Option<NaiveDateTime>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            minimize_to_tray: None,
            scale: 1.0,
            tray_notice_shown: false,
            widget_small: false,
            widget_format: None,
            widget_locked: false,
            widget_opacity: 1.0,
            backup_dir: None,
            last_backup: None,
        }
    }
}

pub const SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.7..=2.0;

impl Settings {
    fn path() -> PathBuf {
        data_dir().join("settings.json")
    }

    /// Coloca os valores lidos do arquivo dentro do que o app aceita e migra o que veio de versões antigas.
    pub fn normalize(&mut self) {
        self.scale = if self.scale.is_finite() { self.scale.clamp(*SCALE_RANGE.start(), *SCALE_RANGE.end()) } else { 1.0 };
        self.widget_opacity =
            if self.widget_opacity.is_finite() { self.widget_opacity.clamp(*OPACITY_RANGE.start(), *OPACITY_RANGE.end()) } else { 1.0 };
        if self.widget_format.is_none() {
            self.widget_format = Some(if self.widget_small { WidgetFormat::Pequeno } else { WidgetFormat::Medio });
        }
        self.widget_small = false;
    }

    pub fn widget_prefs(&self) -> WidgetPrefs {
        WidgetPrefs { format: self.widget_format.unwrap_or_default(), locked: self.widget_locked, opacity: self.widget_opacity }
    }

    pub fn set_widget_prefs(&mut self, prefs: WidgetPrefs) {
        self.widget_format = Some(prefs.format);
        self.widget_locked = prefs.locked;
        self.widget_opacity = prefs.opacity;
    }

    pub fn load() -> Self {
        let mut s: Self =
            std::fs::read_to_string(Self::path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        s.normalize();
        s
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(data_dir());
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::path(), json);
        }
    }
}
