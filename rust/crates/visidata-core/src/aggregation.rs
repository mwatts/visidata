//! Aggregation functions for columns (sum, avg, count, min, max).

use crate::column::Column;
use crate::row::Row;
use crate::value::Value;

/// Available aggregation functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggFunc {
    Sum,
    Avg,
    Count,
    Min,
    Max,
    Median,
    Mode,
    Stdev,
    Distinct,
}

impl AggFunc {
    /// Returns the display name of this aggregation.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Sum      => "sum",
            Self::Avg      => "avg",
            Self::Count    => "count",
            Self::Min      => "min",
            Self::Max      => "max",
            Self::Median   => "median",
            Self::Mode     => "mode",
            Self::Stdev    => "stdev",
            Self::Distinct => "distinct",
        }
    }

    /// Look up an `AggFunc` by name (case-insensitive).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "sum"      => Some(Self::Sum),
            "avg"|"mean" => Some(Self::Avg),
            "count"    => Some(Self::Count),
            "min"      => Some(Self::Min),
            "max"      => Some(Self::Max),
            "median"   => Some(Self::Median),
            "mode"     => Some(Self::Mode),
            "stdev"|"std" => Some(Self::Stdev),
            "distinct" => Some(Self::Distinct),
            _ => None,
        }
    }
}

/// Compute an aggregation over the given rows for a column.
#[must_use]
#[expect(clippy::cast_precision_loss, reason = "acceptable for aggregation summary")]
#[expect(clippy::too_many_lines, reason = "flat match over all aggregation variants")]
pub fn aggregate(col: &Column, rows: &[Row], func: AggFunc) -> Value {
    match func {
        AggFunc::Count => {
            let count = rows.iter().filter(|r| !col.raw_value(r).is_null()).count();
            #[expect(clippy::cast_possible_wrap, reason = "count won't exceed i64")]
            Value::Int(count as i64)
        }
        AggFunc::Sum => {
            let mut sum = 0.0_f64;
            let mut has_value = false;
            for row in rows {
                if let Some(f) = col.typed_value(row).as_float() {
                    sum += f;
                    has_value = true;
                }
            }
            if has_value {
                Value::Float(sum)
            } else {
                Value::Null
            }
        }
        AggFunc::Avg => {
            let mut sum = 0.0_f64;
            let mut count = 0_usize;
            for row in rows {
                if let Some(f) = col.typed_value(row).as_float() {
                    sum += f;
                    count += 1;
                }
            }
            if count > 0 {
                Value::Float(sum / count as f64)
            } else {
                Value::Null
            }
        }
        AggFunc::Min => {
            let mut min_val: Option<Value> = None;
            for row in rows {
                let val = col.typed_value(row);
                if val.is_null() {
                    continue;
                }
                if let Some(ref current) = min_val {
                    if val.partial_cmp(current) == Some(std::cmp::Ordering::Less) {
                        min_val = Some(val);
                    }
                } else {
                    min_val = Some(val);
                }
            }
            min_val.unwrap_or(Value::Null)
        }
        AggFunc::Max => {
            let mut max_val: Option<Value> = None;
            for row in rows {
                let val = col.typed_value(row);
                if val.is_null() { continue; }
                if let Some(ref current) = max_val {
                    if val.partial_cmp(current) == Some(std::cmp::Ordering::Greater) {
                        max_val = Some(val);
                    }
                } else {
                    max_val = Some(val);
                }
            }
            max_val.unwrap_or(Value::Null)
        }
        AggFunc::Median => {
            let mut vals: Vec<f64> = rows.iter()
                .filter_map(|r| col.typed_value(r).as_float())
                .collect();
            if vals.is_empty() { return Value::Null; }
            vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mid = vals.len() / 2;
            let median = if vals.len().is_multiple_of(2) {
                f64::midpoint(vals[mid - 1], vals[mid])
            } else {
                vals[mid]
            };
            Value::Float(median)
        }
        AggFunc::Mode => {
            use std::collections::HashMap;
            let mut counts: HashMap<String, (usize, Value)> = HashMap::new();
            for row in rows {
                let val = col.typed_value(row);
                if val.is_null() { continue; }
                let key = val.to_string();
                let entry = counts.entry(key).or_insert((0, val));
                entry.0 += 1;
            }
            counts.into_values()
                .max_by_key(|(n, _)| *n)
                .map_or(Value::Null, |(_, v)| v)
        }
        #[expect(clippy::cast_precision_loss, reason = "acceptable for stdev")]
        AggFunc::Stdev => {
            let vals: Vec<f64> = rows.iter()
                .filter_map(|r| col.typed_value(r).as_float())
                .collect();
            let n = vals.len();
            if n < 2 { return Value::Null; }
            let mean = vals.iter().sum::<f64>() / n as f64;
            let variance = vals.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
            Value::Float(variance.sqrt())
        }
        AggFunc::Distinct => {
            use std::collections::HashSet;
            let distinct: HashSet<String> = rows.iter()
                .map(|r| col.typed_value(r))
                .filter(|v| !v.is_null())
                .map(|v| v.to_string())
                .collect();
            #[expect(clippy::cast_possible_wrap, reason = "distinct count < i64::MAX")]
            Value::Int(distinct.len() as i64)
        }
    }
}

/// Compute all aggregations for a column and return as a formatted string.
#[must_use]
pub fn aggregate_summary(col: &Column, rows: &[Row]) -> String {
    let funcs = [
        AggFunc::Count,
        AggFunc::Sum,
        AggFunc::Avg,
        AggFunc::Min,
        AggFunc::Max,
    ];
    funcs
        .iter()
        .filter_map(|func| {
            let val = aggregate(col, rows, *func);
            if val.is_null() {
                None
            } else {
                Some(format!("{}={val}", func.name()))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::column::{ColumnId, ColumnType};

    fn sample_rows() -> (Column, Vec<Row>) {
        let mut col = Column::new(ColumnId(0), "val", 0);
        col.col_type = ColumnType::Float;
        let rows = vec![
            Row::new(vec![Value::Float(10.0)]),
            Row::new(vec![Value::Float(20.0)]),
            Row::new(vec![Value::Float(30.0)]),
            Row::new(vec![Value::Null]),
        ];
        (col, rows)
    }

    #[test]
    fn agg_count() {
        let (col, rows) = sample_rows();
        assert_eq!(aggregate(&col, &rows, AggFunc::Count), Value::Int(3));
    }

    #[test]
    fn agg_sum() {
        let (col, rows) = sample_rows();
        assert_eq!(aggregate(&col, &rows, AggFunc::Sum), Value::Float(60.0));
    }

    #[test]
    fn agg_avg() {
        let (col, rows) = sample_rows();
        assert_eq!(aggregate(&col, &rows, AggFunc::Avg), Value::Float(20.0));
    }

    #[test]
    fn agg_min() {
        let (col, rows) = sample_rows();
        assert_eq!(aggregate(&col, &rows, AggFunc::Min), Value::Float(10.0));
    }

    #[test]
    fn agg_max() {
        let (col, rows) = sample_rows();
        assert_eq!(aggregate(&col, &rows, AggFunc::Max), Value::Float(30.0));
    }

    #[test]
    fn agg_empty() {
        let col = Column::new(ColumnId(0), "val", 0);
        let rows: Vec<Row> = vec![];
        assert_eq!(aggregate(&col, &rows, AggFunc::Sum), Value::Null);
        assert_eq!(aggregate(&col, &rows, AggFunc::Count), Value::Int(0));
    }

    #[test]
    fn agg_summary() {
        let (col, rows) = sample_rows();
        let summary = aggregate_summary(&col, &rows);
        assert!(summary.contains("count=3"));
        assert!(summary.contains("sum=60"));
    }

    #[test]
    fn agg_func_names() {
        assert_eq!(AggFunc::Sum.name(), "sum");
        assert_eq!(AggFunc::Avg.name(), "avg");
        assert_eq!(AggFunc::Count.name(), "count");
        assert_eq!(AggFunc::Min.name(), "min");
        assert_eq!(AggFunc::Max.name(), "max");
    }
}
