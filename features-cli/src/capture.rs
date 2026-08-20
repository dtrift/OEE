//! Схема CSV захвата с железа — контракт между прошивкой (запись),
//! разметкой и пайплайном обучения.
//!
//! Отличается от экспорта симулятора (`t_ms,current_a,state`) колонками
//! трассировки; `value` — те же единицы (амперы после
//! [`crate::calibration::CurrentCalibration`]), поэтому код нарезки окон и
//! фичей общий для обеих колей.

/// Заголовок CSV захвата (порядок колонок фиксирован).
pub const CAPTURE_HEADER: [&str; 6] = ["t_ms", "node", "run_id", "value", "state", "note"];

/// Значение колонки `state` до разметки (метки проставляются вручную по
/// журналу прогона — ground truth стенда).
pub const STATE_UNLABELED: &str = "";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeKind;

    #[test]
    fn header_is_stable() {
        // Контракт закреплён Literal'ом: любое изменение — осознанная правка,
        // синхронно с прошивкой-писателем и скриптом обучения.
        assert_eq!(
            CAPTURE_HEADER,
            ["t_ms", "node", "run_id", "value", "state", "note"]
        );
    }

    #[test]
    fn node_column_values_match_node_kinds() {
        // Колонка node ∈ {a,p,q} — ровно значения NodeKind::as_str.
        let values = [
            NodeKind::A.as_str(),
            NodeKind::P.as_str(),
            NodeKind::Q.as_str(),
        ];
        assert_eq!(values, ["a", "p", "q"]);
    }
}
