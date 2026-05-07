use std::fs;
use std::io::Write;
use std::path::PathBuf;

use chrono::Utc;

use crate::{
    checksum::merge_sha256_files,
    config::Config,
    error::{DownloadError as Error, Result},
    labels,
    models::Sanitized,
};
use imap::Client;
use mailparse::parse_mail;
use native_tls::TlsConnector;
use sha2::{Digest, Sha256};
use tracing::{debug, error, info, instrument, warn};

#[instrument(skip_all, fields(host = config.host, output = %config.output_dir.display()))]
pub fn download(config: &Config) -> Result<()> {
    let connector = TlsConnector::new()?;
    let stream = std::net::TcpStream::connect((config.host.as_str(), config.port))?;
    let tls_stream = connector.connect(&config.host, stream)?;
    let client = Client::new(tls_stream);

    let mut session = match client.login(&config.username, &config.password) {
        Ok(s) => s,
        Err(_) => {
            return Err(Error::Login {
                host: config.host.clone(),
                port: config.port,
                username: config.username.clone(),
            }
            .into());
        }
    };

    let server_dir = PathBuf::from(&config.output_dir).join(&config.host);

    fs::create_dir_all(&server_dir)?;

    let folders = match session.list(Some(""), Some("%")) {
        Ok(f) => f,
        Err(_) => {
            return Err(Error::FolderList {
                host: config.host.clone(),
                port: config.port,
            }
            .into());
        }
    };

    let last_checked_path = server_dir.join("last_checked");
    let previous_checked_path = server_dir.join("last_checked.previous");
    let last_checked = fs::read_to_string(&last_checked_path).ok();
    let search_filter = last_checked
        .as_ref()
        .map(|ts| format!("SINCE {ts}"))
        .unwrap_or_else(|| "ALL".to_string());
    let mut write_last_checked = true;

    // TODO: use thread_local to handle those concurrently
    for folder in folders.iter() {
        let folder_name = folder.name();

        if let Err(error) = process_folder(&mut session, folder_name, &search_filter, &server_dir) {
            write_last_checked = false;
            error!(folder = folder_name, error = ?error, "Error processing folder");
        }
    }
    if write_last_checked {
        if last_checked_path.exists()
            && let Err(e) = fs::rename(&last_checked_path, &previous_checked_path)
        {
            warn!(target: "imap", "Could not rename last_checked to last_checked.previous: {}", e);
        }

        let now = Utc::now();
        let formatted_time = now.format("%d-%b-%Y").to_string();
        if let Err(e) = fs::write(&last_checked_path, &formatted_time) {
            warn!(error=%e, "Could not write last_checked file");
        }
    }

    session.logout()?;
    merge_sha256_files(config, last_checked)?;

    info!("Download complete!");

    labels::process_emails(&server_dir, &config.label_dir)?;

    Ok(())
}

#[instrument(skip(session, base_dir))]
fn process_folder(
    session: &mut imap::Session<native_tls::TlsStream<std::net::TcpStream>>,
    folder: &str,
    search_filter: &str,
    base_dir: &std::path::Path,
) -> Result<usize> {
    let folder_dir = base_dir.join(Sanitized::from(folder).to_str());

    debug!("select");
    session.select(folder)?;
    info!("search");

    let messages = match session.search(search_filter) {
        Ok(m) => m,
        Err(e) => {
            return Err(Error::Search {
                folder: folder.to_string(),
                source: e,
            }
            .into());
        }
    };
    info!(count = messages.len(), "found messages");

    for (idx, uid) in messages.iter().enumerate() {
        info!(uid, index = idx, total = messages.len(), "fetching");
        let fetch = match session.fetch(uid.to_string(), "RFC822") {
            Ok(f) => f,
            Err(e) => {
                return Err(Error::Fetch {
                    folder: folder.to_string(),
                    uid: *uid,
                    source: e,
                }
                .into());
            }
        };

        debug!(len = fetch.len(), "storing");
        // Potentially a bug that we just care about first?
        if let Some(msg) = fetch.first()
            && let Some(email_data) = msg.body()
        {
            let email = crate::models::Email::from(parse_mail(email_data)?);
            let header = crate::models::EmailHeader::from(&email);
            let email_dir = header.to_path(&folder_dir);
            fs::create_dir_all(&email_dir)?;

            let email_path = email_dir.join(format!("{}.eml", uid));
            fs::write(&email_path, email_data)?;

            let mut hasher = Sha256::new();
            hasher.update(email_data);
            let hash = hex::encode(hasher.finalize());

            let shafile_path = base_dir.join("sha256sums.new");
            let mut shafile = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&shafile_path)?;

            let email_rel_path =
                email_path
                    .strip_prefix(base_dir)
                    .map_err(|e| Error::InvalidPath {
                        path: email_path.display().to_string(),
                        base: base_dir.display().to_string(),
                        source: e,
                    })?;
            writeln!(shafile, "{}  {}", hash, email_rel_path.display())?;
            crate::email::process(email.as_ref(), &email_dir)?;
        }
        info!("processed");
    }

    info!("folder processed");

    Ok(messages.len())
}
