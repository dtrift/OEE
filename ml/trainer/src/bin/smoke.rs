//! Track D0: the burn smoke test — Conv1d forward on a 128×1 window, one
//! backward step. Retires the "won't build / too slow" risk before any
//! training code lands.
//!
//!     cargo run -p trainer --bin smoke

use burn::backend::ndarray::NdArray;
use burn::backend::Autodiff;
use burn::tensor::backend::Backend;
use burn::tensor::Tensor;

type B = Autodiff<NdArray>;

fn main() {
    let start = std::time::Instant::now();
    let device = Default::default();
    B::seed(&device, 42);

    let model = trainer::model::ModelCnn::<B>::init(&device, &trainer::TaskSpec::a());

    let input = Tensor::<B, 3>::from_floats([[[0.5f32; 128]]], &device);
    let logits = model.forward(input.clone());
    let probs: Vec<f32> = model
        .forward_softmax(input)
        .into_data()
        .iter::<f32>()
        .collect();
    println!("softmax: {probs:?}");

    // One backward step through a scalar loss (the autodiff sanity).
    let loss = logits.sum();
    let grads = loss.backward();
    let _ = grads;

    println!("smoke ok in {:?}", start.elapsed());
}
