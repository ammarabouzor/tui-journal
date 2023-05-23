use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::get_default_data_dir;

#[derive(Debug, Deserialize, Serialize)]
pub struct JsonBackend {
    pub file_path: PathBuf,
}

impl JsonBackend {
    pub fn get_default() -> anyhow::Result<Self> {
        Ok(JsonBackend {
            file_path: get_default_json_path()?,
        })
    }
}

pub fn get_default_json_path() -> anyhow::Result<PathBuf> {
    Ok(get_default_data_dir()?.join("entries.json"))
}