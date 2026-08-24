//! Aggregation engine (Layer 3 of `connectors/CONNECT_SPEC.md`). Runs a
//! shape's `aggregate:` verb list over the full transformed dataset from a
//! pull, producing either a flat set of computed globals (ungrouped
//! pipelines) or a set of buckets with per-bucket computed values.

use anyhow::Result;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

use crate::connect::expr::{self, BucketCond, CallArg, ExprCtx};
use crate::connect::types::{
    AggStep, BucketSpec, CompareSpec, DeriveSpec, FilterCond, RankSpec, TallySpec,
};

#[derive(Debug, Clone)]
pub struct Bucket {
    pub key: Vec<Value>,
    pub rows: Vec<Value>,
    pub computed: Map<String, Value>,
}

impl Bucket {
    /// Stable string label for this bucket's key, used to align buckets
    /// across a `bucket()` re-grouping (see `compare`) and for rendering.
    pub fn key_label(&self) -> String {
        self.key.iter().map(value_to_label).collect::<Vec<_>>().join("|")
    }
}

/// Running state through a shape's aggregate pipeline. `rows` always holds
/// the current flat dataset (pre- or post-bucket); `buckets`, once set by a
/// `bucket`/`distinct` step, is the grouped view later steps operate on.
/// `globals` holds scalar values from ungrouped `tally`/`derive` steps.
/// `history` remembers every per-bucket computed value ever produced, keyed
/// by bucket label, so `compare` can align values across a later re-bucketing
/// (e.g. maze's `severity_shift`, which buckets by `maze_severity` then again
/// by scanner `severity`).
#[derive(Debug, Clone, Default)]
pub struct AggState {
    pub rows: Vec<Value>,
    pub buckets: Option<Vec<Bucket>>,
    pub globals: Map<String, Value>,
    pub history: HashMap<String, HashMap<String, f64>>,
}

fn value_to_label(v: &Value) -> String {
    match v {
        Value::String(s) => s.to_lowercase(),
        Value::Null => "other".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn get_field<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    let path = path.trim_start_matches('.');
    if path.is_empty() {
        return Some(v);
    }
    let mut cur = v;
    for seg in path.split('.') {
        cur = cur.get(seg)?;
    }
    Some(cur)
}

fn value_eq_str(v: &Value, s: &str) -> bool {
    match v {
        Value::String(vs) => vs.eq_ignore_ascii_case(s),
        other => value_to_label(other) == s.to_lowercase(),
    }
}

fn is_empty_opt(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.is_empty(),
        Some(Value::Array(a)) => a.is_empty(),
        _ => false,
    }
}

fn json_num(v: f64) -> Value {
    match serde_json::Number::from_f64(v) {
        Some(n) => Value::Number(n),
        None => Value::Null,
    }
}

fn as_f64(v: &Value) -> Option<f64> {
    v.as_f64()
}

/// Evaluate a full expression string like `.risk_rank >= 3` or
/// `.service_transport = "tcp"`. Fixed grammar: `<path> <op> <value>`,
/// whitespace-separated (matches every expression in both reference mapping
/// files).
fn eval_full_expr(row: &Value, expr_str: &str) -> bool {
    let parts: Vec<&str> = expr_str.splitn(3, ' ').collect();
    if parts.len() != 3 {
        return false;
    }
    let (path, op, raw_val) = (parts[0], parts[1], parts[2]);
    let field = get_field(row, path);
    let target: Value = if let Some(s) = raw_val.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        Value::String(s.to_string())
    } else if let Ok(n) = raw_val.parse::<f64>() {
        json_num(n)
    } else if raw_val == "true" || raw_val == "false" {
        Value::Bool(raw_val == "true")
    } else {
        Value::String(raw_val.to_string())
    };

    match (field, &target) {
        (Some(f), Value::String(s)) if matches!(f, Value::String(_)) => {
            let fs = f.as_str().unwrap_or_default();
            match op {
                "=" => fs.eq_ignore_ascii_case(s),
                "!=" => !fs.eq_ignore_ascii_case(s),
                _ => false,
            }
        }
        (Some(f), _) => {
            let (Some(fv), Some(tv)) = (as_f64(f), target.as_f64()) else {
                return match op {
                    "=" => f == &target,
                    "!=" => f != &target,
                    _ => false,
                };
            };
            match op {
                "=" => (fv - tv).abs() < f64::EPSILON,
                "!=" => (fv - tv).abs() >= f64::EPSILON,
                ">" => fv > tv,
                ">=" => fv >= tv,
                "<" => fv < tv,
                "<=" => fv <= tv,
                _ => false,
            }
        }
        (None, _) => false,
    }
}

fn eval_cond(row: &Value, cond: &FilterCond) -> bool {
    if let Some(list) = &cond.r#in {
        let val = get_field(row, &cond.r#where);
        return val.map(|v| list.iter().any(|s| value_eq_str(v, s))).unwrap_or(false);
    }
    if let Some(list) = &cond.not_in {
        let val = get_field(row, &cond.r#where);
        return !val.map(|v| list.iter().any(|s| value_eq_str(v, s))).unwrap_or(false);
    }
    if cond.not_empty == Some(true) {
        return !is_empty_opt(get_field(row, &cond.r#where));
    }
    if cond.is_empty == Some(true) {
        return is_empty_opt(get_field(row, &cond.r#where));
    }
    eval_full_expr(row, &cond.r#where)
}

fn apply_filter(rows: Vec<Value>, cond: &FilterCond) -> Vec<Value> {
    rows.into_iter().filter(|r| eval_cond(r, cond)).collect()
}

fn apply_expand(rows: Vec<Value>, path: &str) -> Vec<Value> {
    let field = path.trim_start_matches('.').trim_end_matches("[]");
    let mut out = Vec::new();
    for row in rows {
        match row.get(field).and_then(|v| v.as_array()).cloned() {
            Some(arr) if !arr.is_empty() => {
                for item in arr {
                    let mut merged = row.clone();
                    if let (Some(mobj), Some(iobj)) = (merged.as_object_mut(), item.as_object()) {
                        mobj.remove(field);
                        for (k, v) in iobj {
                            mobj.insert(k.clone(), v.clone());
                        }
                    }
                    out.push(merged);
                }
            }
            _ => out.push(row),
        }
    }
    out
}

fn apply_bucket(rows: Vec<Value>, spec: &BucketSpec) -> Vec<Bucket> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Bucket> = HashMap::new();
    for row in rows {
        let key: Vec<Value> = spec
            .by
            .iter()
            .map(|p| get_field(&row, p).cloned().unwrap_or(Value::Null))
            .collect();
        let label = key.iter().map(value_to_label).collect::<Vec<_>>().join("|");
        if !groups.contains_key(&label) {
            order.push(label.clone());
        }
        groups
            .entry(label)
            .or_insert_with(|| Bucket {
                key,
                rows: vec![],
                computed: Map::new(),
            })
            .rows
            .push(row);
    }

    if let Some(ordered) = &spec.ordered {
        let mut out = Vec::new();
        for label in ordered {
            if let Some(b) = groups.remove(&label.to_lowercase()) {
                out.push(b);
            }
        }
        let leftover: Vec<Bucket> = order
            .iter()
            .filter_map(|l| groups.remove(l))
            .collect();
        if !leftover.is_empty() {
            let mut other_rows = Vec::new();
            for b in leftover {
                other_rows.extend(b.rows);
            }
            out.push(Bucket {
                key: vec![Value::String("other".into())],
                rows: other_rows,
                computed: Map::new(),
            });
        }
        out
    } else {
        order.into_iter().filter_map(|l| groups.remove(&l)).collect()
    }
}

fn compute_tally(rows: &[Value], spec: &TallySpec) -> f64 {
    if spec.all == Some(true) {
        return rows.len() as f64;
    }
    if let Some(field) = &spec.distinct {
        let set: std::collections::HashSet<String> = rows
            .iter()
            .filter_map(|r| get_field(r, field))
            .map(value_to_label)
            .collect();
        return set.len() as f64;
    }
    if let Some(where_path) = &spec.r#where {
        let cond = FilterCond {
            r#where: where_path.clone(),
            r#in: spec.r#in.clone(),
            not_in: None,
            is_empty: None,
            not_empty: spec.not_empty,
        };
        return rows.iter().filter(|r| eval_cond(r, &cond)).count() as f64;
    }
    rows.len() as f64
}

fn apply_tally(state: &mut AggState, spec: &TallySpec) {
    let name = spec.r#as.clone();
    if let Some(buckets) = state.buckets.as_mut() {
        for b in buckets.iter_mut() {
            let v = compute_tally(&b.rows, spec);
            b.computed.insert(name.clone(), json_num(v));
            state.history.entry(name.clone()).or_default().insert(b.key_label(), v);
        }
    } else {
        let v = compute_tally(&state.rows, spec);
        state.globals.insert(name, json_num(v));
    }
}

fn bucket_matches_cond(b: &Bucket, cond: &Option<BucketCond>) -> bool {
    match cond {
        None => true,
        Some(c) => {
            let label = b.key_label();
            c.values.iter().any(|v| v.to_lowercase() == label)
        }
    }
}

fn eval_call(
    func: &str,
    arg: &CallArg,
    cond: &Option<BucketCond>,
    globals: &Map<String, Value>,
    buckets: &Option<Vec<Bucket>>,
    rows: &[Value],
) -> f64 {
    let values: Vec<f64> = match arg {
        CallArg::Field(path) => {
            if let Some(buckets) = buckets {
                buckets
                    .iter()
                    .filter(|b| bucket_matches_cond(b, cond))
                    .flat_map(|b| b.rows.iter())
                    .filter_map(|r| get_field(r, path))
                    .filter_map(as_f64)
                    .collect()
            } else {
                rows.iter().filter_map(|r| get_field(r, path)).filter_map(as_f64).collect()
            }
        }
        CallArg::Ident(name) => {
            if let Some(buckets) = buckets {
                buckets
                    .iter()
                    .filter(|b| bucket_matches_cond(b, cond))
                    .filter_map(|b| b.computed.get(name))
                    .filter_map(as_f64)
                    .collect()
            } else {
                globals.get(name).and_then(as_f64).into_iter().collect()
            }
        }
    };
    expr::apply_func(func, &values)
}

struct GlobalCtx<'a> {
    globals: &'a Map<String, Value>,
    buckets: &'a Option<Vec<Bucket>>,
    rows: &'a [Value],
}

impl ExprCtx for GlobalCtx<'_> {
    fn get_ident(&self, name: &str) -> Option<f64> {
        self.globals.get(name).and_then(as_f64)
    }
    fn get_field(&self, _: &str) -> Option<f64> {
        None
    }
    fn call(&self, func: &str, arg: &CallArg, cond: &Option<BucketCond>) -> f64 {
        eval_call(func, arg, cond, self.globals, self.buckets, self.rows)
    }
}

struct UnitCtx<'a> {
    globals: &'a Map<String, Value>,
    local: &'a Map<String, Value>,
    row: Option<&'a Value>,
    buckets: &'a Option<Vec<Bucket>>,
    rows: &'a [Value],
}

impl ExprCtx for UnitCtx<'_> {
    fn get_ident(&self, name: &str) -> Option<f64> {
        self.local.get(name).or_else(|| self.globals.get(name)).and_then(as_f64)
    }
    fn get_field(&self, path: &str) -> Option<f64> {
        self.row.and_then(|r| get_field(r, path)).and_then(as_f64)
    }
    fn call(&self, func: &str, arg: &CallArg, cond: &Option<BucketCond>) -> f64 {
        eval_call(func, arg, cond, self.globals, self.buckets, self.rows)
    }
}

fn apply_derive(state: &mut AggState, spec: &DeriveSpec) -> Result<()> {
    let ast = expr::parse(&spec.expr)?;
    if expr::has_bare_field_ref(&ast) {
        if state.buckets.is_some() {
            let results: Vec<f64> = {
                let buckets = state.buckets.as_ref().unwrap();
                buckets
                    .iter()
                    .map(|b| {
                        let ctx = UnitCtx {
                            globals: &state.globals,
                            local: &b.computed,
                            row: b.rows.first(),
                            buckets: &state.buckets,
                            rows: &state.rows,
                        };
                        expr::eval(&ast, &ctx)
                    })
                    .collect()
            };
            let buckets = state.buckets.as_mut().unwrap();
            for (b, v) in buckets.iter_mut().zip(results.iter()) {
                b.computed.insert(spec.name.clone(), json_num(*v));
            }
            for (b, v) in buckets.iter().zip(results.iter()) {
                state
                    .history
                    .entry(spec.name.clone())
                    .or_default()
                    .insert(b.key_label(), *v);
            }
            if let Some(first) = results.first() {
                if results.iter().all(|v| (v - first).abs() < 1e-9) {
                    state.globals.insert(spec.name.clone(), json_num(*first));
                }
            }
        } else {
            let n = state.rows.len();
            let mut results = Vec::with_capacity(n);
            for row in state.rows.iter() {
                let ctx = UnitCtx {
                    globals: &state.globals,
                    local: &Map::new(),
                    row: Some(row),
                    buckets: &state.buckets,
                    rows: &state.rows,
                };
                results.push(expr::eval(&ast, &ctx));
            }
            for (row, v) in state.rows.iter_mut().zip(results.into_iter()) {
                if let Some(obj) = row.as_object_mut() {
                    obj.insert(spec.name.clone(), json_num(v));
                }
            }
        }
    } else {
        let v = {
            let ctx = GlobalCtx {
                globals: &state.globals,
                buckets: &state.buckets,
                rows: &state.rows,
            };
            expr::eval(&ast, &ctx)
        };
        state.globals.insert(spec.name.clone(), json_num(v));
        if let Some(buckets) = state.buckets.as_mut() {
            for b in buckets.iter_mut() {
                b.computed.insert(spec.name.clone(), json_num(v));
            }
        }
    }
    Ok(())
}

fn compare_op(op: &str, a: f64, b: f64) -> f64 {
    match op {
        "subtract" => a - b,
        "ratio" => {
            if b != 0.0 {
                a / b
            } else {
                0.0
            }
        }
        "percent_change" => {
            if b != 0.0 {
                (a - b) / b * 100.0
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

fn apply_compare(state: &mut AggState, spec: &CompareSpec) {
    if let Some(buckets) = state.buckets.as_mut() {
        for b in buckets.iter_mut() {
            let label = b.key_label();
            let av = state
                .history
                .get(&spec.a)
                .and_then(|m| m.get(&label))
                .copied()
                .or_else(|| b.computed.get(&spec.a).and_then(as_f64))
                .unwrap_or(0.0);
            let bv = state
                .history
                .get(&spec.b)
                .and_then(|m| m.get(&label))
                .copied()
                .or_else(|| b.computed.get(&spec.b).and_then(as_f64))
                .unwrap_or(0.0);
            let r = compare_op(&spec.op, av, bv);
            b.computed.insert(spec.r#as.clone(), json_num(r));
        }
    } else {
        let av = state.globals.get(&spec.a).and_then(as_f64).unwrap_or(0.0);
        let bv = state.globals.get(&spec.b).and_then(as_f64).unwrap_or(0.0);
        let r = compare_op(&spec.op, av, bv);
        state.globals.insert(spec.r#as.clone(), json_num(r));
    }
}

fn sort_key(bucket_computed: Option<&Map<String, Value>>, row: Option<&Value>, field: &str, order: &Option<Vec<String>>) -> f64 {
    if let Some(c) = bucket_computed {
        if let Some(v) = c.get(field).and_then(as_f64) {
            return v;
        }
    }
    let path = if field.starts_with('.') {
        field.to_string()
    } else {
        format!(".{}", field)
    };
    if let Some(r) = row {
        if let Some(v) = get_field(r, &path) {
            if let Some(n) = as_f64(v) {
                return n;
            }
            if let (Some(order_list), Some(s)) = (order, v.as_str()) {
                if let Some(idx) = order_list.iter().position(|o| o.eq_ignore_ascii_case(s)) {
                    return (order_list.len() - idx) as f64;
                }
                return f64::MIN;
            }
        }
    }
    f64::MIN
}

fn apply_rank(state: &mut AggState, spec: &RankSpec) {
    let by = spec.by.as_vec();
    let dirs = spec.direction.as_ref().map(|d| d.as_vec()).unwrap_or_default();
    let dir_desc = |i: usize| -> bool { dirs.get(i).map(|d| d != "asc").unwrap_or(true) };

    let cmp_keys = |ka: &[f64], kb: &[f64]| -> std::cmp::Ordering {
        for (i, (a, b)) in ka.iter().zip(kb.iter()).enumerate() {
            let ord = a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal);
            let ord = if dir_desc(i) { ord.reverse() } else { ord };
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
        }
        std::cmp::Ordering::Equal
    };

    if let Some(buckets) = state.buckets.as_mut() {
        let mut keyed: Vec<(Vec<f64>, Bucket)> = buckets
            .drain(..)
            .map(|b| {
                let keys: Vec<f64> = by
                    .iter()
                    .map(|f| sort_key(Some(&b.computed), b.rows.first(), f, &spec.order))
                    .collect();
                (keys, b)
            })
            .collect();
        keyed.sort_by(|a, b| cmp_keys(&a.0, &b.0));
        *buckets = keyed.into_iter().map(|(_, b)| b).collect();
    } else {
        let mut keyed: Vec<(Vec<f64>, Value)> = state
            .rows
            .drain(..)
            .map(|r| {
                let keys: Vec<f64> = by.iter().map(|f| sort_key(None, Some(&r), f, &spec.order)).collect();
                (keys, r)
            })
            .collect();
        keyed.sort_by(|a, b| cmp_keys(&a.0, &b.0));
        state.rows = keyed.into_iter().map(|(_, r)| r).collect();
    }
}

fn apply_take(state: &mut AggState, n: usize) {
    if let Some(buckets) = state.buckets.as_mut() {
        buckets.truncate(n);
    } else {
        state.rows.truncate(n);
    }
}

pub fn run_aggregate(rows: Vec<Value>, steps: &[AggStep]) -> Result<AggState> {
    let mut state = AggState {
        rows,
        buckets: None,
        globals: Map::new(),
        history: HashMap::new(),
    };
    for step in steps {
        match step {
            AggStep::Expand { expand } => {
                state.rows = apply_expand(std::mem::take(&mut state.rows), expand);
            }
            AggStep::Filter { filter } => {
                state.rows = apply_filter(std::mem::take(&mut state.rows), filter);
            }
            AggStep::Bucket { bucket } => {
                state.buckets = Some(apply_bucket(state.rows.clone(), bucket));
            }
            AggStep::Tally { tally } => apply_tally(&mut state, tally),
            AggStep::Derive { derive } => apply_derive(&mut state, derive)?,
            AggStep::Compare { compare } => apply_compare(&mut state, compare),
            AggStep::Rank { rank } => apply_rank(&mut state, rank),
            AggStep::Take { take } => apply_take(&mut state, *take),
            AggStep::Distinct { distinct } => {
                // Pragmatic reading of `distinct` in an aggregate pipeline:
                // it's used to group by that field (see maze's
                // `scanner_coverage`, where a `tally: all` immediately
                // follows to count per-value). A pure unique-values list
                // would be `state.globals[name] = [...]` instead, but no
                // shape in either reference mapping needs that form.
                state.buckets = Some(apply_bucket(
                    state.rows.clone(),
                    &BucketSpec {
                        by: vec![distinct.clone()],
                        ordered: None,
                    },
                ));
            }
        }
    }
    Ok(state)
}

pub fn to_json_summary(state: &AggState) -> Value {
    if let Some(buckets) = &state.buckets {
        let arr: Vec<Value> = buckets
            .iter()
            .map(|b| json!({ "key": b.key, "computed": b.computed, "row_count": b.rows.len() }))
            .collect();
        json!({ "globals": state.globals, "buckets": arr })
    } else {
        json!({ "globals": state.globals, "rows": state.rows })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bucket_tally_derive_pipeline() {
        let rows = vec![
            json!({"risk": "critical"}),
            json!({"risk": "critical"}),
            json!({"risk": "low"}),
        ];
        let steps = vec![
            AggStep::Bucket {
                bucket: BucketSpec {
                    by: vec![".risk".into()],
                    ordered: Some(vec!["critical".into(), "high".into(), "low".into()]),
                },
            },
            AggStep::Tally {
                tally: TallySpec {
                    all: Some(true),
                    r#where: None,
                    not_empty: None,
                    r#in: None,
                    distinct: None,
                    r#as: "asset_count".into(),
                },
            },
            AggStep::Derive {
                derive: DeriveSpec {
                    name: "total".into(),
                    expr: "sum(asset_count)".into(),
                },
            },
        ];
        let state = run_aggregate(rows, &steps).unwrap();
        assert_eq!(state.globals.get("total").and_then(as_f64), Some(3.0));
        let buckets = state.buckets.unwrap();
        assert_eq!(buckets[0].computed.get("asset_count").and_then(as_f64), Some(2.0));
    }

    #[test]
    fn per_row_derive_adds_field() {
        let rows = vec![json!({"vulnerability_count": 10.0, "service_count": 5.0})];
        let steps = vec![AggStep::Derive {
            derive: DeriveSpec {
                name: "vuln_density".into(),
                expr: ".vulnerability_count / .service_count".into(),
            },
        }];
        let state = run_aggregate(rows, &steps).unwrap();
        assert_eq!(state.rows[0]["vuln_density"], json!(2.0));
    }
}
