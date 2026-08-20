//! Узлы цифрового двойника (разд. 3 плана): A (ток), P (счёт), Q (акустика).
//!
//! Неделя 1: каркас-заглушки. Инференс через `#[model]` — недели 3-4,
//! MQTT-публикация — неделя 4-5.

/// Узел A: ток станка → фичи → 1D-CNN → статус (idle/run/jam/overload).
pub mod a;

/// Узел P: IR-барьер → детектор фронта → счёт деталей.
pub mod p;

/// Узел Q: tap-тест → синтез звука → 1D-CNN → годен/брак.
pub mod q;

#[cfg(test)]
mod tests {
    #[test]
    fn skeleton_imports() {
        // Проверка, что модули-заглушки компилируются.
        let _ = super::a::describe();
        let _ = super::p::describe();
        let _ = super::q::describe();
    }
}
