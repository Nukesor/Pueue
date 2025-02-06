use crate::prelude::*;

use std::collections::HashMap;
use std::env::temp_dir;
use std::fs::{canonicalize, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use assert_cmd::prelude::*;
use tempfile::{Builder, TempDir};
use tokio::io::{self, AsyncWriteExt};

use pueue::daemon::run;
use pueue_lib::settings::*;
use tokio::task::JoinHandle;

use crate::helper::*;

/// All info about a booted standalone test daemon.
/// This daemon is executed in the same async environment as the rest of the test.
pub struct PueueDaemon {
    pub settings: Settings,
    pub tempdir: TempDir,
    #[allow(dead_code)]
    pub pid: i32,
    // The async join handle of the daemon function.
    // Can be used to abort the daemon manually.
    pub join_handle: JoinHandle<Result<()>>,
}

/// Implement a custom drop for the Daemon test struct
impl Drop for PueueDaemon {
    fn drop(&mut self) {
        // The daemon runs in background tokio task.
        // Use this handle to make sure that it gets always killed.
        let _ = self.join_handle.abort_handle();
    }
}

/// A helper function which creates some test config, sets up a temporary directory and spawns
/// a daemon into the async tokio runtime.
/// This is done in 90% of our tests, thereby this convenience helper.
pub async fn daemon() -> Result<PueueDaemon> {
    let (settings, tempdir) = daemon_base_setup()?;
    daemon_with_settings(settings, tempdir).await
}

/// A helper function which takes a Pueue config, a temporary directory and spawns
/// a daemon into the async tokio runtime.
pub async fn daemon_with_settings(settings: Settings, tempdir: TempDir) -> Result<PueueDaemon> {
    // Uncomment the next line to get some daemon logging.
    // Ignore any logger initialization errors, as multiple loggers will be initialized.
    // let _ =
    //     simplelog::SimpleLogger::init(simplelog::LevelFilter::Debug, simplelog::Config::default());

    let pueue_dir = tempdir.path();
    let path = pueue_dir.to_path_buf();
    // Start/spin off the daemon and get its PID
    let join_handle = tokio::spawn(run_and_handle_error(path, true));
    let pid = get_pid(&settings.shared.pid_path()).await?;

    let sleep = 50;
    let tries = TIMEOUT / sleep;
    let mut current_try = 0;

    // Wait up to 1s for the unix socket to pop up.
    let socket_path = settings.shared.unix_socket_path();
    while current_try < tries {
        sleep_ms(sleep).await;
        if socket_path.exists() {
            create_test_groups(&settings.shared).await?;
            return Ok(PueueDaemon {
                settings,
                tempdir,
                pid,
                join_handle,
            });
        }

        current_try += 1;
    }

    bail!("Daemon didn't boot after 1sec")
}

/// Internal helper function, which wraps the daemon main logic inside tokio and prints any errors.
async fn run_and_handle_error(pueue_dir: PathBuf, test: bool) -> Result<()> {
    if let Err(err) = run(Some(pueue_dir.join("pueue.yml")), None, test).await {
        let mut stdout = io::stdout();
        stdout
            .write_all(format!("Entcountered error: {err:?}").as_bytes())
            .await
            .expect("Failed to write to stdout.");
        stdout.flush().await?;

        return Err(err);
    }

    Ok(())
}

/// Spawn the daemon by calling the actual pueued binary.
/// This function also checks for the pid file and the unix socket to appear.
pub async fn standalone_daemon(shared: &Shared) -> Result<Child> {
    // Inject an environment variable into the daemon.
    // This is used to test that the spawned subprocesses won't inherit the daemon's environment.
    let mut envs = HashMap::new();
    envs.insert("PUEUED_TEST_ENV_VARIABLE", "Test");

    let child = Command::cargo_bin("pueued")?
        .arg("--config")
        .arg(shared.pueue_directory().join("pueue.yml").to_str().unwrap())
        .arg("-vvv")
        .envs(envs)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let sleep = 50;
    let tries = TIMEOUT / sleep;
    let mut current_try = 0;

    // Wait up to 1s for the unix socket to pop up.
    let socket_path = shared.unix_socket_path();
    while current_try < tries {
        sleep_ms(sleep).await;
        if socket_path.exists() {
            return Ok(child);
        }

        current_try += 1;
    }

    bail!("Daemon didn't boot in stand-alone mode after 1sec")
}

/// This is the base setup for all daemon test setups.
pub fn daemon_base_setup() -> Result<(Settings, TempDir)> {
    // Init the logger for debug output during tests.
    // We ignore the result, as the logger can be initialized multiple times due to the
    // way tests are run in Rust.
    //use log::LevelFilter;
    //use simplelog::{Config, SimpleLogger};
    //let _ = SimpleLogger::init(LevelFilter::Info, Config::default());

    // Create a temporary directory used for testing.
    // The path is canonicalized to ensure test consistency across platforms.
    let tempdir = Builder::new()
        .prefix("pueue-")
        .tempdir_in(canonicalize(temp_dir())?)?;
    let tempdir_path = tempdir.path();

    std::fs::create_dir(tempdir_path.join("certs")).unwrap();

    let shared = Shared {
        pueue_directory: Some(tempdir_path.to_path_buf()),
        runtime_directory: Some(tempdir_path.to_path_buf()),
        alias_file: Some(tempdir_path.join("pueue_aliases.yml")),
        host: "localhost".to_string(),
        port: "51230".to_string(),
        daemon_cert: Some(tempdir_path.join("certs").join("daemon.cert")),
        daemon_key: Some(tempdir_path.join("certs").join("daemon.key")),
        shared_secret_path: Some(tempdir_path.join("secret")),
        ..Default::default()
    };

    let client = Client {
        max_status_lines: Some(15),
        status_datetime_format: "%Y-%m-%d %H:%M:%S".into(),
        edit_mode: EditMode::Files,
        ..Default::default()
    };

    #[allow(deprecated)]
    let daemon = Daemon {
        callback_log_lines: 15,
        ..Default::default()
    };

    let settings = Settings {
        client,
        daemon,
        shared,
        profiles: HashMap::new(),
    };

    settings
        .save(&Some(tempdir_path.join("pueue.yml")))
        .context("Couldn't write pueue config to temporary directory")?;

    Ok((settings, tempdir))
}

/// Create a few test groups that have various parallel task settings.
pub async fn create_test_groups(shared: &Shared) -> Result<()> {
    add_group_with_slots(shared, "test_2", 2).await?;
    add_group_with_slots(shared, "test_3", 3).await?;
    add_group_with_slots(shared, "test_5", 5).await?;

    wait_for_group(shared, "test_3").await?;
    wait_for_group(shared, "test_5").await?;

    Ok(())
}

/// Create an alias file that'll be used by the daemon to do task aliasing.
/// This fill should be created in the daemon's temporary runtime directory.
pub fn create_test_alias_file(config_dir: &Path, aliases: HashMap<String, String>) -> Result<()> {
    let content = serde_yaml::to_string(&aliases)
        .context("Failed to serialize aliase configuration file.")?;

    // Write the deserialized content to our alias file.
    let path = config_dir.join("pueue_aliases.yml");
    let mut alias_file = File::create(path).context("Failed to open alias file")?;

    alias_file
        .write_all(content.as_bytes())
        .context("Failed writing to alias file")?;

    Ok(())
}
