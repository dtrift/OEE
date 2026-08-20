//! Feature parity (разд. 6 плана): фичи для обучения и инференса считает
//! один Rust-крейт — numpy получает уже готовые фичи.
//!
//! Неделя 1: каркас-заглушка. Реальные фичи узлов A/Q — недели 3-4.

/// Признаковое окно узла A (заглушка; контракт — неделя 3).
pub fn window_len() -> usize {
    128
}

#[cfg(test)]
mod tests {
    #[test]
    fn window_matches_spike_model() {
        // Должно совпадать с TIMESTEPS в ml/scripts/build_conv1d_model.py.
        assert_eq!(super::window_len(), 128);
    }
}
