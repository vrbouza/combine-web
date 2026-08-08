//! The negative log-likelihood, as Minuit2 sees it.

use std::cell::RefCell;

use crate::model::CompiledModel;

#[derive(Default)]
struct Scratch {
    expected: Vec<f64>,
    eval: crate::model::EvalScratch,
}

pub struct Nll {
    pub model: CompiledModel,
    scratch: RefCell<Scratch>,
    /// Combine offsets the NLL at the pre-fit point. This is a numerical
    /// requirement rather than a nicety: Migrad's convergence test is on the
    /// EDM, and a raw NLL of ~1e5 against a scale of 1e-4 loses nine digits.
    offset: f64,
}

impl Nll {
    pub fn new(model: CompiledModel) -> Self {
        let mut nll = Nll {
            scratch: RefCell::new(Scratch {
                expected: vec![0.0; model.n_bins],
                eval: Default::default(),
            }),
            model,
            offset: 0.0,
        };
        let init: Vec<f64> = nll.model.params.iter().map(|p| p.init).collect();
        nll.offset = nll.raw(&init);
        nll
    }

    /// The likelihood before offsetting.
    pub fn raw(&self, par: &[f64]) -> f64 {
        let mut s = self.scratch.borrow_mut();
        let Scratch { expected, eval } = &mut *s;
        expected.resize(self.model.n_bins, 0.0);
        self.model.expected(par, expected, eval);

        // Kahan summation: cheap insurance against the bin ordering shifting
        // the last few digits of the NLL.
        let (mut sum, mut c) = (0.0f64, 0.0f64);
        for (&e, &n) in expected.iter().zip(&self.model.obs) {
            let e = e.max(1e-12);
            let term = e - n * e.ln();
            let y = term - c;
            let t = sum + y;
            c = (t - sum) - y;
            sum = t;
        }
        for (p, &v) in self.model.params.iter().zip(par) {
            sum += p.constraint(v);
        }
        // The Barlow-Beeston parameters are profiled analytically inside
        // `expected`, but their unit-Gaussian priors still count.
        for &x in &eval.bb_x {
            sum += 0.5 * x * x;
        }
        sum
    }

    pub fn offset(&self) -> f64 {
        self.offset
    }
}

impl minuit2::FCN for Nll {
    fn value(&self, par: &[f64]) -> f64 {
        self.raw(par) - self.offset
    }

    /// 0.5 for a negative log-likelihood, so MINOS crosses at dNLL = 0.5 and
    /// HESSE errors are one-sigma.
    fn error_def(&self) -> f64 {
        0.5
    }
}
