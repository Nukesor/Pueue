use crate::prelude::*;

use std::collections::HashMap;
use std::fs::{create_dir_all, File};
use std::io::{prelude::*, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use shellexpand::tilde;

use crate::error::Error;
use crate::setting_defaults::*;

/// The environment variable that can be set to overwrite pueue's config path.
pub const PUEUE_CONFIG_PATH_ENV: &str = "PUEUE_CONFIG_PATH";

/// All settings which are used by both, the client and the daemon
#[derive(PartialEq, Eq, Clone, Debug, Deserialize, Serialize)]
pub struct Shared {
    /// Don't access this property directly, but rather use the getter with the same name.
    /// It's only public to allow proper integration testing.
    ///
    /// The directory that is used for all of pueue's state. \
    /// I.e. task logs, state dumps, etc.
    pub pueue_directory: Option<PathBuf>,
    /// Don't access this property directly, but rather use the getter with the same name.
    /// It's only public to allow proper integration testing.
    ///
    /// The location where runtime related files will be placed.
    /// Defaults to `pueue_directory` unless `$XDG_RUNTIME_DIR` is set.
    pub runtime_directory: Option<PathBuf>,
    /// Don't access this property directly, but rather use the getter with the same name.
    /// It's only public to allow proper integration testing.
    ///
    /// The location of the alias file used by the daemon/client when working with
    /// aliases.
    pub alias_file: Option<PathBuf>,

    /// If this is set to true, unix sockets will be used.
    /// Otherwise we default to TCP+TLS
    #[cfg(not(target_os = "windows"))]
    #[serde(default = "default_true")]
    pub use_unix_socket: bool,
    /// Don't access this property directly, but rather use the getter with the same name.
    /// It's only public to allow proper integration testing.
    ///
    /// The path to the unix socket.
    #[cfg(not(target_os = "windows"))]
    pub unix_socket_path: Option<PathBuf>,
    /// Unix socket permissions. Typically specified as an octal number and
    /// defaults to `0o700` which grants only the current user access to the
    /// socket. For a client to connect to the daemon, the client must have
    /// read/write permissions.
    #[cfg(not(target_os = "windows"))]
    pub unix_socket_permissions: Option<u32>,

    /// The TCP hostname/ip address.
    #[serde(default = "default_host")]
    pub host: String,
    /// The TCP port.
    #[serde(default = "default_port")]
    pub port: String,

    /// The path where the daemon's PID is located.
    /// This is by default in `runtime_directory/pueue.pid`.
    pub pid_path: Option<PathBuf>,

    /// Don't access this property directly, but rather use the getter with the same name.
    /// It's only public to allow proper integration testing.
    ///
    /// The path to the TLS certificate used by the daemon. \
    /// This is also used by the client to verify the daemon's identity.
    pub daemon_cert: Option<PathBuf>,
    /// Don't access this property directly, but rather use the getter with the same name.
    /// It's only public to allow proper integration testing.
    ///
    /// The path to the TLS key used by the daemon.
    pub daemon_key: Option<PathBuf>,
    /// Don't access this property directly, but rather use the getter with the same name.
    /// It's only public to allow proper integration testing.
    ///
    /// The path to the file containing the shared secret used to authenticate the client.
    pub shared_secret_path: Option<PathBuf>,
}

/// The mode in which the client should edit tasks.
#[derive(PartialEq, Eq, Clone, Debug, Deserialize, Serialize, Default)]
pub enum EditMode {
    /// Edit by having one large file with all tasks to be edited inside at the same time
    #[default]
    Toml,
    /// Edit by creating a folder for each task to be edited, where each property is a single file.
    Files,
}

/// All settings which are used by the client
#[derive(PartialEq, Eq, Clone, Debug, Deserialize, Serialize)]
pub struct Client {
    /// If set to true, all tasks will be restart in place, instead of creating a new task.
    /// False is the default, as you'll lose the logs of the previously failed tasks when
    /// restarting tasks in place.
    #[serde(default = "Default::default")]
    pub restart_in_place: bool,
    /// Whether the client should read the logs directly from disk or whether it should
    /// request the data from the daemon via socket.
    #[serde(default = "default_true")]
    pub read_local_logs: bool,
    /// Whether the client should show a confirmation question on potential dangerous actions.
    #[serde(default = "Default::default")]
    pub show_confirmation_questions: bool,
    /// Whether the client should show a confirmation question on potential dangerous actions.
    #[serde(default = "Default::default")]
    pub edit_mode: EditMode,
    /// Whether aliases specified in `pueue_aliases.yml` should be expanded in the `pueue status`
    /// or shown in their short form.
    #[serde(default = "Default::default")]
    pub show_expanded_aliases: bool,
    /// Whether the client should use dark shades instead of regular colors.
    #[serde(default = "Default::default")]
    pub dark_mode: bool,
    /// The max amount of lines each task get's in the `pueue status` view.
    pub max_status_lines: Option<usize>,
    /// The format that will be used to display time formats in `pueue status`.
    #[serde(default = "default_status_time_format")]
    pub status_time_format: String,
    /// The format that will be used to display datetime formats in `pueue status`.
    #[serde(default = "default_status_datetime_format")]
    pub status_datetime_format: String,
}

/// All settings which are used by the daemon
#[derive(PartialEq, Eq, Clone, Debug, Deserialize, Serialize)]
pub struct Daemon {
    /// Whether a group should be paused as soon as a single task fails
    #[serde(default = "Default::default")]
    pub pause_group_on_failure: bool,
    /// Whether the daemon (and all groups) should be paused as soon as a single task fails
    #[serde(default = "Default::default")]
    pub pause_all_on_failure: bool,
    /// The callback that's called whenever a task finishes.
    pub callback: Option<String>,
    /// Environment variables that can be will be injected into all executed processes.
    #[serde(default = "Default::default")]
    pub env_vars: HashMap<String, String>,
    /// The amount of log lines from stdout/stderr that are passed to the callback command.
    #[serde(default = "default_callback_log_lines")]
    pub callback_log_lines: usize,
    /// The command that should be used for task and callback execution.
    /// The following are the only officially supported modi for Pueue.
    ///
    /// Unix default:
    /// `vec!["sh", "-c", "{{ pueue_command_string }}"]`.
    ///
    /// Windows default:
    /// `vec!["powershell", "-c", "[Console]::OutputEncoding = [Text.UTF8Encoding]::UTF8; {{ pueue_command_string }}"]`
    pub shell_command: Option<Vec<String>>,
}

impl Default for Shared {
    fn default() -> Self {
        Shared {
            pueue_directory: None,
            runtime_directory: None,
            alias_file: None,

            #[cfg(not(target_os = "windows"))]
            unix_socket_path: None,
            #[cfg(not(target_os = "windows"))]
            use_unix_socket: true,
            #[cfg(not(target_os = "windows"))]
            unix_socket_permissions: Some(0o700),
            host: default_host(),
            port: default_port(),

            pid_path: None,
            daemon_cert: None,
            daemon_key: None,
            shared_secret_path: None,
        }
    }
}

impl Default for Client {
    fn default() -> Self {
        Client {
            restart_in_place: false,
            read_local_logs: true,
            show_confirmation_questions: false,
            show_expanded_aliases: false,
            edit_mode: Default::default(),
            dark_mode: false,
            max_status_lines: None,
            status_time_format: default_status_time_format(),
            status_datetime_format: default_status_datetime_format(),
        }
    }
}

impl Default for Daemon {
    fn default() -> Self {
        Daemon {
            pause_group_on_failure: false,
            pause_all_on_failure: false,
            callback: None,
            callback_log_lines: default_callback_log_lines(),
            shell_command: None,
            env_vars: HashMap::new(),
        }
    }
}

/// The parent settings struct. \
/// This contains all other setting structs.
#[derive(PartialEq, Eq, Clone, Default, Debug, Deserialize, Serialize)]
pub struct Settings {
    #[serde(default = "Default::default")]
    pub client: Client,
    #[serde(default = "Default::default")]
    pub daemon: Daemon,
    #[serde(default = "Default::default")]
    pub shared: Shared,
    #[serde(default = "HashMap::new")]
    pub profiles: HashMap<String, NestedSettings>,
}

/// The nested settings struct for profiles. \
/// In contrast to the normal `Settings` struct, this struct doesn't allow profiles.
/// That way we prevent nested profiles and problems with self-referencing structs.
#[derive(PartialEq, Eq, Clone, Debug, Deserialize, Serialize)]
pub struct NestedSettings {
    #[serde(default = "Default::default")]
    pub client: Client,
    #[serde(default = "Default::default")]
    pub daemon: Daemon,
    #[serde(default = "Default::default")]
    pub shared: Shared,
}

pub fn default_configuration_directory() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("pueue"))
}

/// Get the default config directory.
/// If no config can be found, fallback to the current directory.
pub fn configuration_directories() -> Vec<PathBuf> {
    if let Some(config_dir) = default_configuration_directory() {
        vec![config_dir, PathBuf::from(".")]
    } else {
        vec![PathBuf::from(".")]
    }
}

/// Little helper which expands a given path's `~` characters to a fully qualified path.
pub fn expand_home(old_path: &Path) -> PathBuf {
    PathBuf::from(tilde(&old_path.to_string_lossy()).into_owned())
}

impl Shared {
    pub fn pueue_directory(&self) -> PathBuf {
        if let Some(path) = &self.pueue_directory {
            expand_home(path)
        } else if let Some(path) = dirs::data_local_dir() {
            path.join("pueue")
        } else {
            PathBuf::from("./pueue")
        }
    }

    /// Get the current runtime directory in the following precedence.
    /// 1. Config value
    /// 2. Environment configuration
    /// 3. Pueue directory
    pub fn runtime_directory(&self) -> PathBuf {
        if let Some(path) = &self.runtime_directory {
            expand_home(path)
        } else if let Some(path) = dirs::runtime_dir() {
            path
        } else {
            self.pueue_directory()
        }
    }

    /// The unix socket path can either be explicitly specified or it's simply placed in the
    /// current runtime directory.
    #[cfg(not(target_os = "windows"))]
    pub fn unix_socket_path(&self) -> PathBuf {
        if let Some(path) = &self.unix_socket_path {
            expand_home(path)
        } else {
            self.runtime_directory()
                .join(format!("pueue_{}.socket", whoami::username()))
        }
    }

    /// The location of the alias file used by the daemon/client when working with
    /// task aliases.
    pub fn alias_file(&self) -> PathBuf {
        if let Some(path) = &self.alias_file {
            expand_home(path)
        } else if let Some(config_dir) = default_configuration_directory() {
            config_dir.join("pueue_aliases.yml")
        } else {
            PathBuf::from("pueue_aliases.yml")
        }
    }

    /// The daemon's pid path can either be explicitly specified or it's simply placed in the
    /// current runtime directory.
    pub fn pid_path(&self) -> PathBuf {
        if let Some(path) = &self.pid_path {
            expand_home(path)
        } else {
            self.runtime_directory().join("pueue.pid")
        }
    }

    pub fn daemon_cert(&self) -> PathBuf {
        if let Some(path) = &self.daemon_cert {
            expand_home(path)
        } else {
            self.pueue_directory().join("certs").join("daemon.cert")
        }
    }

    pub fn daemon_key(&self) -> PathBuf {
        if let Some(path) = &self.daemon_key {
            expand_home(path)
        } else {
            self.pueue_directory().join("certs").join("daemon.key")
        }
    }

    pub fn shared_secret_path(&self) -> PathBuf {
        if let Some(path) = &self.shared_secret_path {
            expand_home(path)
        } else {
            self.pueue_directory().join("shared_secret")
        }
    }
}

impl Settings {
    /// Try to read existing config files, while using default values for non-existing fields.
    /// If successful, this will return a full config as well as a boolean on whether we found an
    /// existing configuration file or not.
    ///
    /// The default local config locations depends on the current target.
    pub fn read(from_file: &Option<PathBuf>) -> Result<(Settings, bool), Error> {
        // If no explicit path is provided, we look for the PUEUE_CONFIG_PATH env variable.
        let from_file = from_file
            .clone()
            .or_else(|| std::env::var(PUEUE_CONFIG_PATH_ENV).map(PathBuf::from).ok());

        // Load the config from a very specific file path
        if let Some(path) = &from_file {
            // Open the file in read-only mode with buffer.
            let file = File::open(path)
                .map_err(|err| Error::IoPathError(path.clone(), "opening config file", err))?;
            let reader = BufReader::new(file);

            let settings = serde_yaml::from_reader(reader)
                .map_err(|err| Error::ConfigDeserialization(err.to_string()))?;
            return Ok((settings, true));
        };

        info!("Parsing config files");

        let config_dirs = configuration_directories();
        for directory in config_dirs.into_iter() {
            let path = directory.join("pueue.yml");
            info!("Checking path: {path:?}");

            // Check if the file exists and parse it.
            if path.exists() && path.is_file() {
                info!("Found config file at: {path:?}");

                // Open the file in read-only mode with buffer.
                let file = File::open(&path)
                    .map_err(|err| Error::IoPathError(path, "opening config file.", err))?;
                let reader = BufReader::new(file);

                let settings = serde_yaml::from_reader(reader)
                    .map_err(|err| Error::ConfigDeserialization(err.to_string()))?;
                return Ok((settings, true));
            }
        }

        info!("No config file found. Use default config.");
        // Return a default configuration if we couldn't find a file.
        Ok((Settings::default(), false))
    }

    /// Save the current configuration as a file to the given path. \
    /// If no path is given, the default configuration path will be used. \
    /// The file is then written to the main configuration directory of the respective OS.
    pub fn save(&self, path: &Option<PathBuf>) -> Result<(), Error> {
        let config_path = if let Some(path) = path {
            path.clone()
        } else if let Ok(path) = std::env::var(PUEUE_CONFIG_PATH_ENV) {
            PathBuf::from(path)
        } else if let Some(path) = dirs::config_dir() {
            let path = path.join("pueue");
            path.join("pueue.yml")
        } else {
            return Err(Error::Generic(
                "Failed to resolve default config directory. User home cannot be determined."
                    .into(),
            ));
        };
        let config_dir = config_path
            .parent()
            .ok_or_else(|| Error::InvalidPath("Couldn't resolve config directory".into()))?;

        // Create the config dir, if it doesn't exist yet
        if !config_dir.exists() {
            create_dir_all(config_dir).map_err(|err| {
                Error::IoPathError(config_dir.to_path_buf(), "creating config dir", err)
            })?;
        }

        let content = match serde_yaml::to_string(self) {
            Ok(content) => content,
            Err(error) => {
                return Err(Error::Generic(format!(
                    "Configuration file serialization failed:\n{error}"
                )))
            }
        };
        let mut file = File::create(&config_path).map_err(|err| {
            Error::IoPathError(config_dir.to_path_buf(), "creating settings file", err)
        })?;
        file.write_all(content.as_bytes()).map_err(|err| {
            Error::IoPathError(config_dir.to_path_buf(), "writing settings file", err)
        })?;

        Ok(())
    }

    /// Try to load a profile. Error if it doesn't exist.
    pub fn load_profile(&mut self, profile: &str) -> Result<(), Error> {
        let profile = self.profiles.remove(profile).ok_or_else(|| {
            Error::ConfigDeserialization(format!("Couldn't find profile with name \"{profile}\""))
        })?;

        self.client = profile.client;
        self.daemon = profile.daemon;
        self.shared = profile.shared;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// Check if profiles get loaded correctly.
    #[test]
    fn test_load_profile() {
        // Create some default settings and ensure that default values are loaded.
        let mut settings = Settings::default();
        assert_eq!(
            settings.client.status_time_format,
            default_status_time_format()
        );
        assert_eq!(
            settings.daemon.callback_log_lines,
            default_callback_log_lines()
        );
        assert_eq!(settings.shared.host, default_host());

        // Crate a new profile with slightly different values.
        let mut profile = Settings::default();
        profile.client.status_time_format = "test".to_string();
        profile.daemon.callback_log_lines = 100_000;
        profile.shared.host = "quatschhost".to_string();
        let profile = NestedSettings {
            client: profile.client,
            daemon: profile.daemon,
            shared: profile.shared,
        };

        settings.profiles.insert("testprofile".to_string(), profile);

        // Load the profile and ensure the new values are now loaded.
        settings
            .load_profile("testprofile")
            .expect("We just added the profile");

        assert_eq!(settings.client.status_time_format, "test");
        assert_eq!(settings.daemon.callback_log_lines, 100_000);
        assert_eq!(settings.shared.host, "quatschhost");
    }

    /// A proper pueue [Error] should be thrown if the profile cannot be found.
    #[test]
    fn test_error_on_missing_profile() {
        let mut settings = Settings::default();

        let result = settings.load_profile("doesn't exist");
        let expected_error_message = "Couldn't find profile with name \"doesn't exist\"";
        if let Err(Error::ConfigDeserialization(error_message)) = result {
            assert_eq!(error_message, expected_error_message);
            return;
        }

        panic!("Got unexpected result when expecting missing profile error: {result:?}");
    }
}
