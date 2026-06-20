use std::fs;
use std::io::Write;
use std::path::PathBuf;

use chrono::{NaiveDate, Utc};
use std::num::NonZeroU32;

use crate::{
    checksum::merge_sha256_files,
    config::Config,
    error::{DownloadError as Error, Result},
    labels,
    models::Sanitized,
};
use io_imap::client::ImapClientStd;
use io_imap::codec::imap_types::core::Vec1;
use io_imap::codec::imap_types::datetime::NaiveDate as ImapNaiveDate;
use io_imap::codec::imap_types::fetch::{MacroOrMessageDataItemNames, MessageDataItem};
use io_imap::codec::imap_types::mailbox::{ListMailbox, Mailbox};
use io_imap::codec::imap_types::search::SearchKey;
use io_imap::codec::imap_types::sequence::SequenceSet;
use mailparse::parse_mail;
use pimalaya_stream::tls::Tls;
use sha2::{Digest, Sha256};
use tracing::{debug, error, info, instrument, warn};
use url::Url;

#[instrument(skip_all, fields(host = config.host, output = %config.output_dir.display()))]
pub fn download(config: &Config) -> Result<()> {
    let tls = Tls::default();
    let url = Url::parse(&format!("imaps://{}:{}", config.host, config.port))?;

    let (mut client, _capabilities) =
        ImapClientStd::connect(&url, &tls, false, None::<pimalaya_stream::sasl::Sasl>, None)?;

    let _capabilities = client
        .login(&config.username, &config.password, Default::default())
        .map_err(|_| Error::Login {
            host: config.host.clone(),
            port: config.port,
            username: config.username.clone(),
        })?;

    let server_dir = PathBuf::from(&config.output_dir).join(&config.host);

    fs::create_dir_all(&server_dir)?;

    let listing = client
        .list(
            Mailbox::try_from("").unwrap(),
            ListMailbox::try_from("*").unwrap(),
        )
        .map_err(|_| Error::FolderList {
            host: config.host.clone(),
            port: config.port,
        })?;

    let last_checked_path = server_dir.join("last_checked");
    let previous_checked_path = server_dir.join("last_checked.previous");
    let last_checked = fs::read_to_string(&last_checked_path).ok();
    let search_filter = last_checked
        .as_ref()
        .map(|ts| format!("SINCE {ts}"))
        .unwrap_or_else(|| "ALL".to_string());
    let mut write_last_checked = true;

    // TODO: use thread_local to handle those concurrently
    for (folder, _, _) in listing.into_iter() {
        if let Err(error) = process_folder(&mut client, folder, &search_filter, &server_dir) {
            write_last_checked = false;
            error!(error = ?error, "Error processing folder");
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

    client.logout()?;
    merge_sha256_files(config, last_checked)?;

    info!("Download complete!");

    labels::process_emails(&server_dir, &config.label_dir)?;

    Ok(())
}

fn parse_search_filter(search_filter: &str) -> Result<Vec1<SearchKey<'static>>> {
    let result = if let Some(date_str) = search_filter.strip_prefix("SINCE ") {
        let date = NaiveDate::parse_from_str(date_str, "%d-%b-%Y").map_err(|_| Error::Search {
            folder: "unknown".to_string(),
            source: io_imap::client::ImapClientStdError::MessageSearch(
                io_imap::rfc3501::search::ImapMessageSearchError::Bad(
                    "Failed to parse date".to_string(),
                ),
            ),
        })?;
        let date = ImapNaiveDate::unvalidated(date);
        SearchKey::Since(date)
    } else {
        SearchKey::All
    };
    Ok(Vec1::from(result))
}

#[instrument(skip(client, base_dir))]
fn process_folder(
    client: &mut ImapClientStd,
    mailbox: Mailbox<'static>,
    search_filter: &str,
    base_dir: &std::path::Path,
) -> Result<usize> {
    let folder = match &mailbox {
        Mailbox::Inbox => "inbox",
        Mailbox::Other(mailbox_other) => {
            std::str::from_utf8(mailbox_other.as_ref()).expect("utf8 mailbox")
        }
    }
    .to_owned();
    let folder_dir = base_dir.join(Sanitized::from(&folder).to_str());

    debug!("select");

    let _select_data = client.select(mailbox)?;

    info!("search");
    let criteria = parse_search_filter(search_filter)?;

    let messages = match client.search(criteria, false) {
        Ok(m) => m,
        Err(e) => {
            return Err(Error::Search { folder, source: e }.into());
        }
    };
    info!(count = messages.len(), "found messages");

    for (idx, uid) in messages.iter().enumerate() {
        debug!(
            uid = uid.get(),
            index = idx,
            total = messages.len(),
            "fetching"
        );

        let items = MacroOrMessageDataItemNames::from(vec![
            io_imap::codec::imap_types::fetch::MessageDataItemName::Rfc822,
        ]);
        let uid_nonzero = NonZeroU32::new(uid.get()).expect("UID should be non-zero");

        let fetch = client
            .fetch(SequenceSet::from(uid_nonzero), items, false)
            .map_err(|e| Error::Fetch {
                folder: folder.to_string(),
                uid: uid.get(),
                source: e,
            })?;

        info!(folder, messages = fetch.len(), "Fetched",);

        for (msg_uid, message_data) in fetch.iter() {
            debug!(msg_uid = ?msg_uid, "Processing",);
            for item in (*message_data).as_ref() {
                if let MessageDataItem::Rfc822(email_data) = item {
                    let email_data = match email_data.0 {
                        Some(ref data) => data.as_ref(),
                        None => continue,
                    };

                    let email = crate::models::Email::from(parse_mail(email_data)?);
                    let header = crate::models::EmailHeader::from(&email);
                    let email_dir = header.to_path(&folder_dir);
                    fs::create_dir_all(&email_dir)?;

                    let email_path = email_dir.join(format!("{}.eml", msg_uid.get()));
                    info!("Saving email to {}", email_path.display());
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
            }
            debug!(msg_uid = ?msg_uid, "Processed");
        }
    }

    info!(folder, "processed");

    Ok(messages.len())
}
