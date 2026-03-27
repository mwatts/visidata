use anyhow::Result;
use rhai::{Dynamic, Engine, EvalAltResult, Scope};
use visidata_core::Value;

/// The `VisiData` scripting engine, wrapping Rhai with registered types.
#[derive(Debug)]
pub struct ScriptEngine {
    engine: Engine,
}

impl ScriptEngine {
    /// Create a new script engine with `VisiData` types registered.
    #[must_use]
    pub fn new() -> Self {
        let mut engine = Engine::new();
        Self::register_types(&mut engine);
        Self { engine }
    }

    /// Register `VisiData` types and functions with the Rhai engine.
    #[expect(clippy::cast_possible_truncation, reason = "intentional f64 to i64 coercion")]
    #[expect(clippy::cast_precision_loss, reason = "acceptable i64 to f64 precision loss")]
    #[expect(clippy::cast_possible_wrap, reason = "string lengths won't exceed i64::MAX")]
    fn register_types(engine: &mut Engine) {
        // Value conversion functions available in scripts
        engine.register_fn("to_int", |x: i64| -> i64 { x });
        engine.register_fn("to_int", |x: f64| -> i64 { x as i64 });
        engine.register_fn("to_int", |x: &str| -> Result<i64, Box<EvalAltResult>> {
            x.parse::<i64>()
                .map_err(|e| e.to_string().into())
        });

        engine.register_fn("to_float", |x: f64| -> f64 { x });
        engine.register_fn("to_float", |x: i64| -> f64 { x as f64 });
        engine.register_fn("to_float", |x: &str| -> Result<f64, Box<EvalAltResult>> {
            x.parse::<f64>()
                .map_err(|e| e.to_string().into())
        });

        engine.register_fn("is_null", |x: Dynamic| -> bool { x.is_unit() });

        engine.register_fn("str_len", |s: &str| -> i64 {
            s.len() as i64
        });

        engine.register_fn("str_upper", |s: &str| -> String {
            s.to_uppercase()
        });

        engine.register_fn("str_lower", |s: &str| -> String {
            s.to_lowercase()
        });

        engine.register_fn("str_trim", |s: &str| -> String {
            s.trim().to_owned()
        });

        engine.register_fn("str_contains", |s: &str, pattern: &str| -> bool {
            s.contains(pattern)
        });
    }

    /// Evaluate a Rhai expression and return the result as a `Value`.
    ///
    /// # Errors
    ///
    /// Returns an error if the expression fails to parse or evaluate.
    pub fn eval_expr(&self, expr: &str, scope: &mut Scope<'_>) -> Result<Value> {
        let result = self
            .engine
            .eval_with_scope::<Dynamic>(scope, expr)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(dynamic_to_value(result))
    }

    /// Execute a Rhai script block (statements, not just expressions).
    ///
    /// # Errors
    ///
    /// Returns an error if the script fails to parse or execute.
    pub fn exec_script(&self, script: &str, scope: &mut Scope<'_>) -> Result<()> {
        self.engine
            .run_with_scope(scope, script)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(())
    }

    /// Returns a reference to the underlying Rhai engine for advanced usage.
    #[must_use]
    pub const fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Returns a mutable reference to the underlying Rhai engine.
    pub const fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }
}

impl Default for ScriptEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a Rhai `Dynamic` value to a `VisiData` `Value`.
fn dynamic_to_value(d: Dynamic) -> Value {
    if d.is_unit() {
        Value::Null
    } else if d.is_int() {
        Value::Int(d.as_int().expect("checked is_int"))
    } else if d.is_float() {
        Value::Float(d.as_float().expect("checked is_float"))
    } else if d.is_bool() {
        Value::Bool(d.as_bool().expect("checked is_bool"))
    } else if d.is_string() {
        Value::Text(d.into_string().expect("checked is_string"))
    } else {
        Value::Text(d.to_string())
    }
}

/// Convert a `VisiData` `Value` to a Rhai `Dynamic`.
pub fn value_to_dynamic(v: &Value) -> Dynamic {
    match v {
        Value::Null => Dynamic::UNIT,
        Value::Bool(b) => Dynamic::from(*b),
        Value::Int(n) => Dynamic::from(*n),
        Value::Float(f) => Dynamic::from(*f),
        Value::Text(s) => Dynamic::from(s.clone()),
        Value::Date(d) => Dynamic::from(d.to_string()),
        Value::Bytes(b) => Dynamic::from(format!("<{} bytes>", b.len())),
        Value::Error(e) => Dynamic::from(format!("!{e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_arithmetic() {
        let engine = ScriptEngine::new();
        let mut scope = Scope::new();
        let result = engine.eval_expr("2 + 3", &mut scope).unwrap();
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn eval_with_variables() {
        let engine = ScriptEngine::new();
        let mut scope = Scope::new();
        scope.push("x", 10_i64);
        scope.push("y", 20_i64);
        let result = engine.eval_expr("x + y", &mut scope).unwrap();
        assert_eq!(result, Value::Int(30));
    }

    #[test]
    fn eval_string_operations() {
        let engine = ScriptEngine::new();
        let mut scope = Scope::new();
        scope.push("name", "hello world".to_owned());
        let result = engine.eval_expr("str_upper(name)", &mut scope).unwrap();
        assert_eq!(result, Value::Text("HELLO WORLD".into()));
    }

    #[test]
    fn eval_boolean() {
        let engine = ScriptEngine::new();
        let mut scope = Scope::new();
        scope.push("x", 5_i64);
        let result = engine.eval_expr("x > 3", &mut scope).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn eval_float() {
        let engine = ScriptEngine::new();
        let mut scope = Scope::new();
        let result = engine.eval_expr("3.14 * 2.0", &mut scope).unwrap();
        assert_eq!(result, Value::Float(6.28));
    }

    #[test]
    fn exec_script_sets_variables() {
        let engine = ScriptEngine::new();
        let mut scope = Scope::new();
        engine
            .exec_script("let result = 42; let name = \"test\";", &mut scope)
            .unwrap();
        let result = engine.eval_expr("result", &mut scope).unwrap();
        assert_eq!(result, Value::Int(42));
        let name = engine.eval_expr("name", &mut scope).unwrap();
        assert_eq!(name, Value::Text("test".into()));
    }

    #[test]
    fn eval_error() {
        let engine = ScriptEngine::new();
        let mut scope = Scope::new();
        let result = engine.eval_expr("undefined_var", &mut scope);
        assert!(result.is_err());
    }

    #[test]
    fn type_conversion_functions() {
        let engine = ScriptEngine::new();
        let mut scope = Scope::new();

        let result = engine.eval_expr("to_int(3.7)", &mut scope).unwrap();
        assert_eq!(result, Value::Int(3));

        let result = engine.eval_expr("to_float(42)", &mut scope).unwrap();
        assert_eq!(result, Value::Float(42.0));

        let result = engine.eval_expr("to_int(\"123\")", &mut scope).unwrap();
        assert_eq!(result, Value::Int(123));
    }

    #[test]
    fn string_functions() {
        let engine = ScriptEngine::new();
        let mut scope = Scope::new();

        let result = engine.eval_expr("str_len(\"hello\")", &mut scope).unwrap();
        assert_eq!(result, Value::Int(5));

        let result = engine
            .eval_expr("str_trim(\"  hi  \")", &mut scope)
            .unwrap();
        assert_eq!(result, Value::Text("hi".into()));

        let result = engine
            .eval_expr("str_contains(\"hello world\", \"world\")", &mut scope)
            .unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn value_roundtrip() {
        let values = vec![
            Value::Null,
            Value::Int(42),
            Value::Float(3.14),
            Value::Bool(true),
            Value::Text("hello".into()),
        ];
        for v in &values {
            let dyn_val = value_to_dynamic(v);
            let back = dynamic_to_value(dyn_val);
            assert_eq!(&back, v, "roundtrip failed for {v:?}");
        }
    }
}
