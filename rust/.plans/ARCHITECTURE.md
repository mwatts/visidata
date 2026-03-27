# VisiData Rust — Architecture

## Crate Dependency Graph

```
bin/vd
  ├── visidata-tui
  │   ├── visidata-core
  │   └── visidata-scripting
  │       └── visidata-core
  └── visidata-loaders
      └── visidata-core
```

## Key Types by Crate

### `visidata-core`

```
Value (enum)           — cell value: Int, Float, Text, Bool, Date, Null, Error
Row (struct)           — Vec<Value> with RowId
Column (struct)        — name, type, width, getter, formatter
Sheet (struct)         — name, columns, rows, cursor, source, selection, sort state
SheetStack (struct)    — Vec<Sheet> with push/pop/active
Command (struct)       — longname, keystrokes, help, handler
CommandRegistry        — HashMap<String, Command>, keystroke lookup
Option/OptionsManager  — hierarchical option resolution
```

### `visidata-scripting`

```
ScriptEngine (struct)  — wraps rhai::Engine with VisiData type registrations
CommandHandler (enum)  — Native(Box<dyn Fn>) | Script(String)
Scope (struct)         — rhai::Scope populated with sheet/row/col/vd context
```

**Rhai Integration Points:**
- Command execution: commands can be Rust closures OR Rhai expression strings
- Expression columns: computed columns defined as Rhai expressions
- User config: `~/.visidatarc.rhai` for custom commands, hooks, keybindings
- Interactive eval: `exec-rhai` command for ad-hoc expression evaluation
- Cell transforms: apply Rhai expressions to selected cells/columns

### `visidata-tui`

```
App (struct)           — SheetStack + terminal + event loop
InputMode (enum)       — Normal, Input, Command, Menu
Theme (struct)         — color slots for all UI elements
ColorAttr (struct)     — fg, bg, modifiers
Renderer               — draws Sheet → ratatui Frame
InputHandler           — keystroke → Command dispatch
StatusBar              — left/right status rendering
```

### `visidata-loaders`

```
Loader (trait)         — can_load(path) + load(path) -> Sheet
LoaderRegistry         — extension → Loader dispatch
CsvLoader, TsvLoader, JsonLoader, SqliteLoader, ...
```

## Data Flow

```
CLI args
  → LoaderRegistry.load_file(path)
    → Loader.load(path)
      → Sheet { columns, rows }
        → SheetStack.push(sheet)
          → App.run()
            → loop {
                draw(frame, sheet)   // ratatui
                event = poll()       // crossterm
                dispatch(event)      // command system
              }
```

## Python → Rust Mapping

| Python (VisiData) | Rust equivalent |
|-------------------|-----------------|
| `vd` singleton | `App` struct passed through event loop |
| `BaseSheet` | `Sheet` struct (no inheritance — composition) |
| `Column.getter` lambda | `Fn(&Row) -> Value` or column index |
| `execstr` (Python eval) | `CommandHandler::Script(String)` — Rhai expression, or `CommandHandler::Native(Fn)` for built-ins |
| `LazyChainMap` | `rhai::Scope` populated with sheet/vd/col context before eval |
| `@asyncthread` | `tokio::spawn` |
| `options` hierarchy | `OptionsManager` with layered `HashMap` |
| `addCommand()` class method | `CommandRegistry::register()` |
| curses | ratatui + crossterm |
| Dynamic row types | `Row = Vec<Value>` (uniform) |

## Design Principles

1. **No inheritance** — use traits + composition instead of Python's class hierarchy
2. **Dual dispatch** — built-in commands are Rust closures for performance; user-defined commands are Rhai scripts for flexibility
3. **Type-safe values** — `Value` enum instead of `Any`, registered as Rhai custom type
4. **Indexed columns** — columns access rows by index, not arbitrary getters (simpler, faster)
5. **Owned data** — sheets own their rows; no shared mutable state between sheets
6. **Event-driven** — ratatui's immediate-mode rendering + crossterm event polling
7. **Scriptable** — Rhai embedded from day one; user config, expression columns, and custom commands all use it
