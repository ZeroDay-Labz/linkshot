//! `.snagt` templates: a page document with layout/section metadata.

use crate::document::Page;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Template {
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub created_date: String,
    #[serde(default)]
    pub drop_zone_count: u32,
    #[serde(flatten)]
    pub page: Page,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Template {
    pub fn read_file(path: &std::path::Path) -> anyhow::Result<Template> {
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
    pub fn display_name(path: &std::path::Path) -> String {
        path.file_stem().map(|s| s.to_string_lossy().replace('_', " ")).unwrap_or_default()
    }
    pub fn is_landscape(&self) -> bool {
        self.page.capture_canvas_width > self.page.capture_canvas_height
    }
}
