//! Reading Combine datacards.

mod ast;
mod parser;

pub use ast::{AutoMcStats, Datacard, Effect, KeyEntry, ShapeLine, SystKind, Systematic};

impl std::str::FromStr for Datacard {
    type Err = crate::error::Error;
    fn from_str(s: &str) -> crate::error::Result<Self> {
        parser::parse(s)
    }
}

impl Datacard {
    pub fn parse(src: &str) -> crate::error::Result<Self> {
        parser::parse(src)
    }
}
