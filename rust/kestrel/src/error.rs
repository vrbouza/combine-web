//! The error type is the contract between the core and the interface: every
//! variant maps onto one arm of the `ErrorKind` union in `www/src/types.ts`.

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("line {line}: {msg}")]
    Syntax { line: usize, msg: String },

    /// A construct this tool deliberately does not implement. The interface
    /// shows `what` verbatim, so it must name the construct plainly.
    #[error("line {line}: unsupported: {what}")]
    Unsupported { line: usize, what: String },

    #[error("histogram {path:?} not found in {file:?}")]
    MissingHistogram { file: String, path: String },

    #[error("shape file {0:?} was referenced by the datacard but not provided")]
    MissingShapeFile(String),

    #[error("inconsistent card: {0}")]
    Inconsistent(String),

    #[error("the fit did not converge: {0}")]
    Converge(String),
}

impl Error {
    pub(crate) fn syntax(line: usize, msg: impl Into<String>) -> Self {
        Error::Syntax { line, msg: msg.into() }
    }

    pub(crate) fn unsupported(line: usize, what: impl Into<String>) -> Self {
        Error::Unsupported { line, what: what.into() }
    }
}
