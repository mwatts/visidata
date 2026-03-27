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

    /// Set option: --option name=value
    #[arg(long = "option", short = 'o', value_name = "NAME=VALUE")]
    options: Vec<String>,
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

    // Load config file if it exists
    if let Some(config_path) = visidata_core::config::default_config_path()
        && let Ok(config) = visidata_core::config::load_config(&config_path)
    {
        visidata_core::config::apply_config(&config, &mut app.options);
        // Apply keybinding overrides
        for (keystroke, longname) in &config.keybindings {
            app.commands.add(keystroke, longname, "user-defined");
        }
    }

    // Apply CLI option overrides (highest priority)
    for opt_str in &cli.options {
        if let Some((name, val)) = opt_str.split_once('=') {
            app.options
                .set_global(name, visidata_core::Value::Text(val.to_owned()));
        }
    }

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
    let rows = demo_rows();
    Sheet::with_data("demo", columns, rows)
}

/// Raw data rows for the demo sheet.
fn demo_rows() -> Vec<Row> {
    // (name, age, city, salary, active)
    let records: &[(&str, i64, &str, f64, bool)] = &[
        ("Alice",  30, "New York",      85_000.0, true),
        ("Bob",    25, "San Francisco", 92_000.0, true),
        ("Carol",  35, "Chicago",       78_000.0, false),
        ("Dave",   28, "Boston",        71_000.0, true),
        ("Eve",    42, "Seattle",      105_000.0, true),
        ("Frank",  31, "Austin",        68_000.0, false),
        ("Grace",  29, "Denver",        73_000.0, true),
        ("Hank",   38, "Portland",      88_000.0, true),
        ("Ivy",    26, "Miami",         65_000.0, false),
        ("Jack",   45, "Atlanta",      110_000.0, true),
        ("Karen",  33, "Minneapolis",   79_000.0, true),
        ("Leo",    27, "Nashville",     62_000.0, false),
        ("Mona",   39, "Phoenix",       95_000.0, true),
        ("Nick",   24, "Detroit",       58_000.0, true),
        ("Olivia", 36, "San Diego",     87_000.0, false),
    ];
    records
        .iter()
        .map(|&(name, age, city, salary, active)| {
            Row::new(vec![
                name.into(),
                age.into(),
                city.into(),
                Value::Float(salary),
                active.into(),
            ])
        })
        .collect()
}
