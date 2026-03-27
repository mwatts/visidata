//! Arrow IPC (`.arrow`, `.feather`) file loader.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use arrow::ipc::reader::FileReader;
use visidata_core::{Sheet, Value};

use crate::arrow_util::arrow_value_at;
use crate::registry::Loader;

#[derive(Debug)]
pub struct ArrowLoader;

impl Loader for ArrowLoader {
    fn extensions(&self) -> &[&str] {
        &["arrow", "feather"]
    }

    fn load(&self, path: &Path) -> Result<Sheet> {
        let file = std::fs::File::open(path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        let reader = FileReader::try_new(file, None)
            .with_context(|| format!("failed to read Arrow IPC from {}", path.display()))?;

        let schema = reader.schema();
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("arrow");
        let mut sheet = Sheet::new(name);

        for (i, field) in schema.fields().iter().enumerate() {
            sheet.add_column(field.name().as_str(), i);
        }

        for batch_result in reader {
            let batch = batch_result.context("failed to read Arrow batch")?;
            let n_rows = batch.num_rows();
            let n_cols = batch.num_columns();
            for row_idx in 0..n_rows {
                let vals: Vec<Value> = (0..n_cols).map(|ci| {
                    let col = Arc::clone(batch.column(ci));
                    arrow_value_at(&col, row_idx)
                }).collect();
                sheet.add_row(vals);
            }
        }

        sheet.source = Some(path.to_path_buf());
        Ok(sheet)
    }
}
