use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::{
    checksum::merge_sha256_files,
    config::Config,
    error::{DownloadError as Error, Result},
    models::Sanitized,
};
use imap::Client;
use mailparse::parse_mail;
use native_tls::TlsConnector;
use sha2::{Digest, Sha256};

pub fn download(config: &Config) -> Result<()> {
    let connector = TlsConnector::new()?;
    let stream = std::net::TcpStream::connect((config.host.as_str(), config.port))?;
    let tls_stream = connector.connect(&config.host, stream)?;
    let client = Client::new(tls_stream);

    let mut session = match client.login(&config.username, &config.password) {
        Ok(s) => s,
        Err(_) => return Err(Error::Login {
            host: config.host.clone(),
            port: config.port,
            username: config.username.clone(),
        }
        .into()),
    };

    let server_dir = PathBuf::from(&config.output_dir).join(&config.host);
    fs::create_dir_all(&server_dir)?;

    let folders = match session.list(Some(""), Some("%")) {
        Ok(f) => f,
        Err(_) => return Err(Error::FolderList {
            host: config.host.clone(),
            port: config.port,
        }
        .into()),
    };

    for folder in folders.iter() {
        let folder_name = folder.name();
        if let Err(error) = process_folder(&mut session, folder_name, &server_dir) {
            eprintln!("Error processing folder {folder_name}: {error}");
        }
    }
    session.logout()?;

    println!("Download complete!");
    merge_sha256_files(config)?;

    Ok(())
}

fn process_folder(
    session: &mut imap::Session<native_tls::TlsStream<std::net::TcpStream>>,
    folder: &str,
    base_dir: &std::path::Path,
) -> Result<()> {
    let folder_dir = base_dir.join(Sanitized::from(folder).to_str());

    println!("Processing folder: {}...", folder);

    session.select(folder)?;

    let messages = match session.search("ALL") {
        Ok(m) => m,
        Err(e) => return Err(Error::Search { folder: folder.to_string(), source: e }.into()),
    };

    for uid in messages.iter() {
        let fetch = match session.fetch(uid.to_string(), "RFC822") {
            Ok(f) => f,
            Err(e) => return Err(Error::Fetch {
                folder: folder.to_string(),
                uid: *uid,
                source: e,
            }
            .into()),
        };

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

            let email_rel_path = email_path
                .strip_prefix(base_dir)
                .map_err(|e| Error::InvalidPath {
                    path: email_path.display().to_string(),
                    base: base_dir.display().to_string(),
                    source: e,
                })?;
            writeln!(shafile, "{}  {}", hash, email_rel_path.display())?;

            //parse_and_save_attachments(email.as_ref(), &email_dir)?;
        }
    }

    println!("  Downloaded {} emails", messages.len());

    Ok(())
}
