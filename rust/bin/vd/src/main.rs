use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use std::path::Path;

use anyhow::Result;
use clap::Parser;
use visidata_core::{Column, ColumnId, Row, Sheet, Value};
use visidata_loaders::LoaderRegistry;
use visidata_tui::App;

/// `VisiData` — a terminal interface for exploring and arranging tabular data.
#[derive(Parser, Debug)]
#[command(name = "vd", version, about)]
struct Cli {
    /// File(s) to open.
    files: Vec<String>,

    /// Start cursor at row N (1-indexed).
    #[arg(long = "row", short = 'r')]
    start_row: Option<usize>,

    /// Start cursor at column N (1-indexed).
    #[arg(long = "col", short = 'c')]
    start_col: Option<usize>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let registry = LoaderRegistry::with_builtins();

    let sheet = if cli.files.is_empty() {
        demo_sheet()
    } else {
        let path = Path::new(&cli.files[0]);
        registry.load_file(path)?
    };

    let mut app = App::new(sheet);

    // Apply start position if given
    if let Some(sheet) = app.stack.active_mut() {
        if let Some(r) = cli.start_row {
            sheet.cursor_row = r.saturating_sub(1);
        }
        if let Some(c) = cli.start_col {
            sheet.cursor_col = c.saturating_sub(1);
        }
        sheet.clamp_cursor();
    }

    app.run()
}

/// Create a demo sheet with sample data for testing.
fn demo_sheet() -> Sheet {
    let columns = vec![
        Column::new(ColumnId(0), "Name", 0),
        Column::new(ColumnId(1), "Age", 1),
        Column::new(ColumnId(2), "City", 2),
        Column::new(ColumnId(3), "Salary", 3),
        Column::new(ColumnId(4), "Active", 4),
    ];

    let data: Vec<Vec<Value>> = vec![
        vec!["Alice".into(), 30_i64.into(), "New York".into(), Value::Float(85000.0), true.into()],
        vec!["Bob".into(), 25_i64.into(), "San Francisco".into(), Value::Float(92000.0), true.into()],
        vec!["Carol".into(), 35_i64.into(), "Chicago".into(), Value::Float(78000.0), false.into()],
        vec!["Dave".into(), 28_i64.into(), "Boston".into(), Value::Float(71000.0), true.into()],
        vec!["Eve".into(), 42_i64.into(), "Seattle".into(), Value::Float(105_000.0), true.into()],
        vec!["Frank".into(), 31_i64.into(), "Austin".into(), Value::Float(68000.0), false.into()],
        vec!["Grace".into(), 29_i64.into(), "Denver".into(), Value::Float(73000.0), true.into()],
        vec!["Hank".into(), 38_i64.into(), "Portland".into(), Value::Float(88000.0), true.into()],
        vec!["Ivy".into(), 26_i64.into(), "Miami".into(), Value::Float(65000.0), false.into()],
        vec!["Jack".into(), 45_i64.into(), "Atlanta".into(), Value::Float(110_000.0), true.into()],
        vec!["Karen".into(), 33_i64.into(), "Minneapolis".into(), Value::Float(79000.0), true.into()],
        vec!["Leo".into(), 27_i64.into(), "Nashville".into(), Value::Float(62000.0), false.into()],
        vec!["Mona".into(), 39_i64.into(), "Phoenix".into(), Value::Float(95000.0), true.into()],
        vec!["Nick".into(), 24_i64.into(), "Detroit".into(), Value::Float(58000.0), true.into()],
        vec!["Olivia".into(), 36_i64.into(), "San Diego".into(), Value::Float(87000.0), false.into()],
    ];

    let rows: Vec<Row> = data.into_iter().map(Row::new).collect();
    Sheet::with_data("demo", columns, rows)
}
