//! Decide se o programa em primeiro plano está sendo usado de verdade, e não só aberto.
//!
//! Lógica pura, sem Windows: o laço do rastreador entrega os sinais (relógio, janela em foco,
//! instante da última tecla ou clique e os sinais passivos) e recebe o estado. Tudo aqui é
//! testável com relógios de mentira.
//!
//! Regras:
//! - Teclado ou mouse nos últimos [`READ_GRACE_SECS`] segundos, e depois de a janela ter vindo
//!   para a frente, é uso. A folga cobre quem está lendo sem mexer em nada.
//! - Sem entrada, ainda é uso se o programa está em tela cheia (vídeo, jogo), se está tocando
//!   áudio ou se um controle de jogo está sendo mexido.
//! - Senão, o programa está só aberto; passados [`AWAY_SECS`] sem nada, a pessoa saiu.

/// Quantos segundos sem teclado nem mouse ainda contam como uso (leitura).
pub const READ_GRACE_SECS: u32 = 90;
/// Sem nada acontecer por este tempo, a pessoa não está mais no computador.
pub const AWAY_SECS: u32 = 300;
/// Teto do uso passivo (tela cheia, áudio, controle) sem nenhuma tecla ou clique: cabe um filme longo,
/// mas não uma noite inteira com a música ligada.
pub const PASSIVE_MAX_SECS: u32 = 3 * 3600;

/// Por que o programa conta como em uso.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Evidence {
    /// Teclado ou mouse recentes, com o programa na frente.
    Input,
    /// Programa em tela cheia (vídeo, apresentação, jogo).
    Fullscreen,
    /// O programa está tocando áudio.
    Audio,
    /// Controle de jogo em uso.
    Controller,
}

/// O que o rastreador sabe sobre o programa em primeiro plano neste segundo.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum State {
    /// Em uso: o tempo conta.
    Using(Evidence),
    /// Aberto na frente, mas sem uso: o tempo não conta.
    Open,
    /// Ninguém no computador (ou nada na frente).
    #[default]
    Away,
}

impl State {
    /// O tempo deste segundo é somado ao programa?
    pub fn counting(self) -> bool {
        matches!(self, State::Using(_))
    }

    /// Texto curto para a interface.
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            State::Using(Evidence::Input) => "em uso",
            State::Using(Evidence::Fullscreen) => "em tela cheia",
            State::Using(Evidence::Audio) => "tocando áudio",
            State::Using(Evidence::Controller) => "com controle",
            State::Open => "aberto, sem uso",
            State::Away => "sem atividade",
        }
    }
}

/// Sinais que valem como uso mesmo sem teclado nem mouse. Só são consultados quando a entrada
/// já está velha, para não gastar nada enquanto a pessoa digita.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Passive {
    pub fullscreen: bool,
    pub audio: bool,
    pub controller: bool,
}

impl Passive {
    fn evidence(self) -> Option<Evidence> {
        if self.fullscreen {
            Some(Evidence::Fullscreen)
        } else if self.audio {
            Some(Evidence::Audio)
        } else if self.controller {
            Some(Evidence::Controller)
        } else {
            None
        }
    }
}

/// Diferença entre dois instantes do relógio de milissegundos do Windows, que dá a volta a cada 49 dias.
fn since(from: u32, to: u32) -> i32 {
    to.wrapping_sub(from) as i32
}

/// Segundos desde a última tecla ou clique.
pub fn stale_secs(now: u32, last_input: u32) -> u32 {
    (since(last_input, now).max(0) as u32) / 1000
}

/// A entrada já está velha o bastante para valer a pena consultar os sinais passivos?
/// Começa um pouco antes da folga acabar, para o estado não oscilar na virada.
pub fn wants_passive(now: u32, last_input: u32) -> bool {
    stale_secs(now, last_input) + 5 >= READ_GRACE_SECS
}

/// Acompanha a janela em foco entre um segundo e outro.
#[derive(Default)]
pub struct Watch {
    /// Janela que estava na frente na última observação.
    focus: Option<usize>,
    /// Só a entrada feita estritamente depois deste instante é da janela atual.
    focus_since: u32,
    /// Instante da observação anterior.
    last_seen: Option<u32>,
}

impl Watch {
    /// `now` e `last_input` são instantes em milissegundos; `focus` identifica a janela da frente.
    pub fn observe(&mut self, now: u32, focus: Option<usize>, last_input: u32, passive: Passive) -> State {
        let Some(hwnd) = focus else {
            self.focus = None;
            self.last_seen = Some(now);
            return State::Away;
        };
        if self.focus != Some(hwnd) {
            self.focus_since = match self.last_seen {
                // A troca aconteceu entre a observação anterior e esta: o Alt+Tab ou o clique que a fez
                // também, e já conta para a janela nova. Uma janela que se põe na frente sozinha, sem
                // entrada nenhuma nesse intervalo, não ganha nada.
                Some(seen) => seen,
                // Primeira observação de todas: a entrada feita pouco antes de o Navi abrir já conta.
                None if since(last_input, now) < 60_000 => last_input.wrapping_sub(1),
                None => now,
            };
            self.focus = Some(hwnd);
        }
        self.last_seen = Some(now);
        let stale = stale_secs(now, last_input);
        let input_here = since(self.focus_since, last_input) > 0;
        if input_here && stale <= READ_GRACE_SECS {
            return State::Using(Evidence::Input);
        }
        // Filme ou música valem sem teclado, mas não para sempre: quem saiu de casa com o som ligado parou.
        if let Some(e) = passive.evidence()
            && stale < PASSIVE_MAX_SECS
        {
            return State::Using(e);
        }
        if stale >= AWAY_SECS { State::Away } else { State::Open }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u32 = 1000;
    const AUDIO: Passive = Passive { fullscreen: false, audio: true, controller: false };

    #[test]
    fn digitar_e_uso_e_parar_de_digitar_deixa_de_ser() {
        let mut w = Watch::default();
        let t0 = 10 * S;
        assert_eq!(w.observe(t0, Some(1), t0, Passive::default()), State::Using(Evidence::Input));
        // Lendo: dentro da folga ainda conta.
        assert_eq!(w.observe(t0 + READ_GRACE_SECS * S, Some(1), t0, Passive::default()), State::Using(Evidence::Input));
        // Passou da folga: aberto, sem uso.
        assert_eq!(w.observe(t0 + (READ_GRACE_SECS + 1) * S, Some(1), t0, Passive::default()), State::Open);
        // Muito tempo: saiu.
        assert_eq!(w.observe(t0 + AWAY_SECS * S, Some(1), t0, Passive::default()), State::Away);
        // Voltou a mexer: conta de novo.
        let t1 = t0 + (AWAY_SECS + 7) * S;
        assert_eq!(w.observe(t1, Some(1), t1, Passive::default()), State::Using(Evidence::Input));
    }

    #[test]
    fn janela_que_rouba_o_foco_nao_conta_ate_a_pessoa_mexer_nela() {
        let mut w = Watch::default();
        let t0 = 10 * S;
        assert_eq!(w.observe(t0, Some(1), t0, Passive::default()), State::Using(Evidence::Input));
        // Outra janela aparece na frente sozinha: a tecla de antes era para a janela 1.
        assert_eq!(w.observe(t0 + 2 * S, Some(2), t0, Passive::default()), State::Open);
        assert_eq!(w.observe(t0 + 30 * S, Some(2), t0, Passive::default()), State::Open);
        // A pessoa clica na nova janela: agora é uso dela.
        assert_eq!(w.observe(t0 + 31 * S, Some(2), t0 + 31 * S, Passive::default()), State::Using(Evidence::Input));
        // E ao voltar para a janela 1 pelo teclado (a tecla vem entre uma observação e outra), continua uso.
        assert_eq!(w.observe(t0 + 40 * S, Some(1), t0 + 39 * S + 500, Passive::default()), State::Using(Evidence::Input));
    }

    #[test]
    fn trocar_de_janela_e_ficar_lendo_conta_para_a_janela_nova() {
        let mut w = Watch::default();
        let t0 = 10 * S;
        assert_eq!(w.observe(t0, Some(1), t0, Passive::default()), State::Using(Evidence::Input));
        assert_eq!(w.observe(t0 + 10 * S, Some(1), t0, Passive::default()), State::Using(Evidence::Input));
        // Alt+Tab meio segundo depois da observação anterior: a janela 2 aparece na observação seguinte.
        let tab = t0 + 10 * S + 500;
        assert_eq!(w.observe(t0 + 11 * S, Some(2), tab, Passive::default()), State::Using(Evidence::Input));
        // Um minuto lendo, sem mexer em nada: ainda é uso da janela 2.
        assert_eq!(w.observe(t0 + 71 * S, Some(2), tab, Passive::default()), State::Using(Evidence::Input));
        assert_eq!(w.observe(t0 + 102 * S, Some(2), tab, Passive::default()), State::Open);
    }

    #[test]
    fn entrada_feita_antes_de_o_navi_abrir_ja_conta() {
        let mut w = Watch::default();
        let t0 = 500 * S;
        // O Navi acabou de abrir; a pessoa digitou 3 s antes, em outra janela que segue na frente.
        assert_eq!(w.observe(t0, Some(7), t0 - 3 * S, Passive::default()), State::Using(Evidence::Input));
        // Mas não se foi há muito tempo: então ela não está usando nada.
        let mut w = Watch::default();
        assert_eq!(w.observe(t0, Some(7), t0 - 200 * S, Passive::default()), State::Open);
    }

    #[test]
    fn audio_tela_cheia_e_controle_valem_sem_teclado() {
        let mut w = Watch::default();
        let t0 = 10 * S;
        let late = t0 + 20 * 60 * S;
        assert_eq!(w.observe(late, Some(1), t0, AUDIO), State::Using(Evidence::Audio));
        let full = Passive { fullscreen: true, ..Passive::default() };
        assert_eq!(w.observe(late + S, Some(1), t0, full), State::Using(Evidence::Fullscreen));
        let pad = Passive { controller: true, ..Passive::default() };
        assert_eq!(w.observe(late + 2 * S, Some(1), t0, pad), State::Using(Evidence::Controller));
        // Acabou o filme: volta a ser só "sem atividade".
        assert_eq!(w.observe(late + 3 * S, Some(1), t0, Passive::default()), State::Away);
        // Som ligado há horas sem ninguém tocar no computador: não conta mais.
        let gone = t0 + PASSIVE_MAX_SECS * S;
        assert_eq!(w.observe(gone - S, Some(1), t0, AUDIO), State::Using(Evidence::Audio));
        assert_eq!(w.observe(gone, Some(1), t0, AUDIO), State::Away);
    }

    #[test]
    fn sem_janela_na_frente_e_ausencia() {
        let mut w = Watch::default();
        assert_eq!(w.observe(S, None, S, AUDIO), State::Away);
        assert!(!State::Away.counting() && !State::Open.counting() && State::Using(Evidence::Audio).counting());
    }

    #[test]
    fn relogio_que_da_a_volta() {
        let mut w = Watch::default();
        let before = u32::MAX - 2 * S;
        assert_eq!(w.observe(before, Some(1), before, Passive::default()), State::Using(Evidence::Input));
        // 4 s depois o contador já virou: a tecla de 4 s atrás ainda está na folga.
        let after = 2 * S;
        assert_eq!(stale_secs(after, before), 4);
        assert_eq!(w.observe(after, Some(1), before, Passive::default()), State::Using(Evidence::Input));
        // Tecla "no futuro" (arredondamento do relógio) conta como agora.
        assert_eq!(stale_secs(after, after + 10), 0);
    }

    #[test]
    fn so_consulta_os_sinais_passivos_perto_do_fim_da_folga() {
        let t = 1000 * S;
        assert!(!wants_passive(t, t - 10 * S));
        assert!(wants_passive(t, t - (READ_GRACE_SECS - 5) * S));
        assert!(wants_passive(t, t.wrapping_sub(3600 * S)));
    }
}
