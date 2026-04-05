use thiserror::Error;

#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("IMAP error: {0}")]
    Imap(#[from] imap::Error),

    #[error("TLS connection failed: {0}")]
    Tls(#[from] native_tls::Error),

    #[error("Handshake error: {0}")]
    Handshake(#[from] native_tls::HandshakeError<std::net::TcpStream>),

    #[error("File system error: {0}")]
    FileSystem(#[from] std::io::Error),

    #[error("Mail parse error: {0}")]
    Parse(#[from] mailparse::MailParseError),

    #[error("Login failed for user '{username}' on {host}:{port}")]
    Login {
        host: String,
        port: u16,
        username: String,
    },

    #[error("Folder listing failed for {host}:{port}")]
    FolderList { host: String, port: u16 },

    #[error("Search failed for folder '{folder}'")]
    Search {
        folder: String,
        #[source]
        source: imap::Error,
    },

    #[error("Fetch failed for UID {uid} in folder '{folder}'")]
    Fetch {
        folder: String,
        uid: u32,
        #[source]
        source: imap::Error,
    },

    #[error("SHA256 error: {0}")]
    Sha256(#[from] crate::checksum::Sha256Error),

    #[error("Email path '{path}' is not relative to base directory '{base}'")]
    InvalidPath {
        path: String,
        base: String,
        #[source]
        source: std::path::StripPrefixError,
    },
}

#[derive(Error, Debug)]
pub enum LabelError {
    #[error("Failed to read labels file: {0}")]
    ReadLabels(#[source] std::io::Error),

    #[error("Failed to parse labels JSON: {0}")]
    ParseLabels(#[source] serde_json::Error),

    #[error("Failed to parse filter '{path}': {source}")]
    ParseFilter {
        path: String,
        #[source]
        source: crate::filter::FilterError,
    },

    #[error("Failed to write label file: {0}")]
    WriteLabel(#[source] std::io::Error),
}

#[derive(Error, Debug)]
pub enum EmailError {
    #[error("Failed to read email file '{path}': {source}")]
    ReadEmail {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse email '{path}': {source}")]
    ParseEmail {
        path: String,
        #[source]
        source: mailparse::MailParseError,
    },

    #[error("Failed to get parent directory of '{path}': {source}")]
    NoParentDir {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to write attachment '{path}': {source}")]
    WriteAttachment {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to decode attachment '{path}': {err_msg}")]
    DecodeAttachment { path: String, err_msg: String },

    #[error("Failed to convert HTML to markdown: {source}")]
    HtmlToMarkdown {
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, anyhow::Error>;
