use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub position: Option<(i32, i32)>,
    pub selected: String,
    pub cli_path: Option<PathBuf>,
}
pub struct Store {
    pub path: PathBuf,
    pub value: Settings,
}
impl Store {
    pub fn load(path: PathBuf) -> Self {
        let value = fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self { path, value }
    }
    pub fn save(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&self.value).map_err(|_| "无法保存设置")?;
        fs::write(&self.path, bytes).map_err(|_| "无法写入用户数据目录".to_string())
    }
}
