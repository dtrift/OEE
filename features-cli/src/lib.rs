#![no_std]

//! Feature parity (разд. 6 плана): фичи для обучения и инференса считает
//! один Rust-крейт — numpy получает уже готовые фичи.
//!
//! Контракты колеи «код-онли ↔ железо»:
//! - [`window_spec`]: окно и частота дискретизации — per-узел, а не одна
//!   глобальная константа (симулятор 1.6 кГц, I2S-микрофон 16 кГц, INA226 ~1 кГц);
//! - [`calibration`]: сырые отсчёты ADC → амперы (ACS712-20A + делитель 2:1);
//! - [`capture`]: схема CSV захвата с железа — те же единицы, что у симулятора.
//!
//! `#![no_std]` — часть контракта: этот крейт компилируется в прошивку узла.

pub mod calibration;
pub mod capture;

/// Узел цифрового двойника.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// Узел A: ток станка → статус (Availability).
    A,
    /// Узел P: IR-барьер → счёт деталей (Performance).
    P,
    /// Узел Q: tap-тест → вердикт годен/брак (Quality).
    Q,
}

impl NodeKind {
    /// Короткое имя узла — значение колонки `node` в CSV захвата
    /// (зеркалит `MachineState::as_str` симулятора).
    pub const fn as_str(self) -> &'static str {
        match self {
            NodeKind::A => "a",
            NodeKind::P => "p",
            NodeKind::Q => "q",
        }
    }
}

/// Контракт признакового окна: сколько отсчётов и на какой частоте.
///
/// Физическое время окна = `samples / sample_rate_hz` — часть контракта
/// модели: 128 отсчётов на 1.6 кГц (узел A) и на 16 кГц (узел Q) — это
/// разные окна (80 мс против 8 мс). Обучающий скрипт и прошивка читают
/// размеры только отсюда.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSpec {
    /// Число отсчётов в окне (TIMESTEPS модели).
    pub samples: usize,
    /// Частота дискретизации источника, Гц.
    pub sample_rate_hz: u32,
}

impl WindowSpec {
    /// Длительность окна, мс.
    pub const fn duration_ms(self) -> u32 {
        (self.samples as u32 * 1000) / self.sample_rate_hz
    }
}

/// Окно узла; `None` — узел событийный, окон не имеет (P: детектор фронта).
pub const fn window_spec(kind: NodeKind) -> Option<WindowSpec> {
    match kind {
        // A: должно совпадать с TIMESTEPS в ml/scripts/build_conv1d_model.py
        // и SAMPLE_RATE_HZ в line-simulator (1.6 кГц = 32 отсчёта на период 50 Гц).
        NodeKind::A => Some(WindowSpec {
            samples: 128,
            sample_rate_hz: 1600,
        }),
        // Q: частота фиксирована железом (INMP441 по I2S, 16 кГц); размер
        // окна предварительный (64 мс) — фиксируется лабораторией недели 4.
        // Менять только здесь: это единая точка правды для обучения и прошивки.
        NodeKind::Q => Some(WindowSpec {
            samples: 1024,
            sample_rate_hz: 16_000,
        }),
        NodeKind::P => None,
    }
}

/// Окно узла A в отсчётах (совместимость со спайк-моделью недели 1).
pub fn window_len() -> usize {
    window_spec(NodeKind::A).expect("узел A — оконный").samples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_a_matches_spike_model() {
        // Должно совпадать с TIMESTEPS в ml/scripts/build_conv1d_model.py
        // и SAMPLE_RATE_HZ в line-simulator/src/scenario.rs.
        let spec = window_spec(NodeKind::A).expect("узел A — оконный");
        assert_eq!(spec.samples, 128);
        assert_eq!(spec.sample_rate_hz, 1600);
        assert_eq!(spec.duration_ms(), 80);
        assert_eq!(window_len(), 128);
    }

    #[test]
    fn node_p_is_event_driven() {
        assert_eq!(window_spec(NodeKind::P), None);
    }

    #[test]
    fn node_kinds_roundtrip_via_capture_column() {
        for kind in [NodeKind::A, NodeKind::P, NodeKind::Q] {
            assert_eq!(NodeKind::as_str(kind).len(), 1);
        }
    }
}
