use crate::prelude::*;

use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::*;

use crate::error::Error;
use crate::settings::Settings;

/// Return the contents of the alias file, if it exists and can be parsed. \
/// The file has to be located in `pueue_directory` and named `pueue_aliases.yml`.
pub fn get_aliases(settings: &Settings) -> Result<HashMap<String, String>, Error> {
    // Go through all config directories and check for a alias file.
    let path = settings.shared.alias_file();

    // Return early if we cannot find the file
    if !path.exists() {
        info!("Didn't find pueue alias file at {path:?}.");
        return Ok(HashMap::new());
    };

    // Read the file content
    let mut alias_file = File::open(&path)
        .map_err(|err| Error::IoPathError(path.clone(), "opening alias file", err))?;
    let mut content = String::new();
    alias_file
        .read_to_string(&mut content)
        .map_err(|err| Error::IoPathError(path.clone(), "reading alias file", err))?;

    serde_yaml::from_str(&content).map_err(|err| {
        Error::ConfigDeserialization(format!("Failed to read alias configuration file:\n{err}"))
    })
}

/// Check if there exists an alias for a given command.
/// Only the first word will be replaced.
pub fn insert_alias(settings: &Settings, command: String) -> String {
    // Get the first word of the command.
    let first = match command.split_whitespace().next() {
        Some(first) => first,
        None => return command,
    };

    let aliases = match get_aliases(settings) {
        Err(err) => {
            info!("Couldn't read aliases file: {err}");
            return command;
        }
        Ok(aliases) => aliases,
    };

    if let Some(alias) = aliases.get(first) {
        return command.replacen(first, alias, 1);
    }

    command
}
