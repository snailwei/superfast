//! Error types for the FAST decoder.

#[derive(Debug)]
pub enum Error {
    Static(String),
    Dynamic(String),
    Runtime(String),
    Eof,
    UnexpectedEof,
    IoError(std::io::Error),
    ParseIntError(std::num::ParseIntError),
    ParseFloatError(std::num::ParseFloatError),
    FromUtf8Error(std::string::FromUtf8Error),
    XmlError(roxmltree::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Static(s) => write!(f, "Static Error: {s}"),
            Self::Dynamic(s) => write!(f, "Dynamic Error: {s}"),
            Self::Runtime(s) => write!(f, "Runtime Error: {s}"),
            Self::Eof => f.write_str("End of file/stream reached"),
            Self::UnexpectedEof => f.write_str("Unexpected end of file/stream reached"),
            Self::IoError(e) => write!(f, "IO Error: {e}"),
            Self::ParseIntError(e) => write!(f, "Parse int error: {e}"),
            Self::ParseFloatError(e) => write!(f, "Parse float error: {e}"),
            Self::FromUtf8Error(e) => write!(f, "UTF-8 error: {e}"),
            Self::XmlError(e) => write!(f, "XML error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(e: std::num::ParseIntError) -> Self {
        Self::ParseIntError(e)
    }
}

impl From<std::num::ParseFloatError> for Error {
    fn from(e: std::num::ParseFloatError) -> Self {
        Self::ParseFloatError(e)
    }
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(e: std::string::FromUtf8Error) -> Self {
        Self::FromUtf8Error(e)
    }
}

impl From<roxmltree::Error> for Error {
    fn from(e: roxmltree::Error) -> Self {
        Self::XmlError(e)
    }
}

impl serde::de::Error for Error {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Self::Runtime(msg.to_string())
    }
}

impl serde::ser::Error for Error {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Self::Runtime(msg.to_string())
    }
}

pub type Result<T, E = Error> = core::result::Result<T, E>;
