use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub output_dir: PathBuf,
}

/// Get the default labels directory
/// Uses XDG_CONFIG_HOME/dumbsterfire if available, otherwise /etc/dumbsterfire
pub fn default_label_dir() -> PathBuf {
    if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg_config)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config")
    } else {
        PathBuf::from("/etc")
    }
    .join("dumbsterfire")
    .join("labels")
}

/// Get the default output directory for downloaded emails
/// Uses XDG_DATA_HOME/dumbsterfire/emails if available, otherwise /var/lib/dumbsterfire/emails
pub fn default_output_dir() -> PathBuf {
    if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
        PathBuf::from(xdg_data)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".local").join("share")
    } else {
        PathBuf::from("/var/lib/")
    }
    .join("dumbsterfire")
    .join("emails")
}
