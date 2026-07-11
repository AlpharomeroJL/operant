//! `excel` namespace adapter: open workbook, read range, write range,
//! save. Generic over [`ExcelBackend`] so tests run against
//! [`MockExcelBackend`] (never touches disk or COM) while production
//! wires in the real COM backend behind the `office-com` feature.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use operant_ir::RiskClass;
use parking_lot::Mutex;
use serde_json::json;

use super::OfficeError;
use crate::adapter::{Adapter, AdapterError, Idempotency, VerbSpec};

const NAMESPACE: &str = "excel";

pub type WorkbookId = u64;

/// What the `excel` adapter needs from an Excel automation backend.
/// `docs/specs/action.md`: "each releasing COM objects deterministically"
/// is [`ExcelBackend::close_workbook`]'s job for the real COM
/// implementation; the mock has nothing to release.
pub trait ExcelBackend: Send + Sync {
    fn open_workbook(&self, path: &str) -> Result<WorkbookId, OfficeError>;
    fn read_range(
        &self,
        workbook: WorkbookId,
        sheet: &str,
        range: &str,
    ) -> Result<Vec<Vec<serde_json::Value>>, OfficeError>;
    fn write_range(
        &self,
        workbook: WorkbookId,
        sheet: &str,
        range: &str,
        values: &[Vec<serde_json::Value>],
    ) -> Result<(), OfficeError>;
    /// Returns the path actually saved to.
    fn save_workbook(
        &self,
        workbook: WorkbookId,
        path: Option<&str>,
    ) -> Result<String, OfficeError>;
    fn close_workbook(&self, workbook: WorkbookId) -> Result<(), OfficeError>;
}

/// A1-notation range, e.g. `"A1"` or `"B2:D5"`, to 0-indexed
/// `(col0, row0, col1, row1)` inclusive bounds with `col0 <= col1` and
/// `row0 <= row1` regardless of corner order in the input.
pub fn parse_range(range: &str) -> Result<(u32, u32, u32, u32), OfficeError> {
    let bad = |reason: &str| OfficeError::BadRange {
        range: range.to_string(),
        reason: reason.to_string(),
    };
    if let Some((a, b)) = range.split_once(':') {
        let (c0, r0) = parse_cell_ref(a).ok_or_else(|| bad("bad start cell"))?;
        let (c1, r1) = parse_cell_ref(b).ok_or_else(|| bad("bad end cell"))?;
        Ok((c0.min(c1), r0.min(r1), c0.max(c1), r0.max(r1)))
    } else {
        let (c, r) = parse_cell_ref(range).ok_or_else(|| bad("bad cell reference"))?;
        Ok((c, r, c, r))
    }
}

fn parse_cell_ref(s: &str) -> Option<(u32, u32)> {
    let split_at = s.find(|c: char| c.is_ascii_digit())?;
    let (col_s, row_s) = s.split_at(split_at);
    let col = col_letters_to_index(col_s)?;
    let row: u32 = row_s.parse().ok()?;
    if row == 0 {
        return None; // A1 notation rows are 1-indexed; 0 is not valid
    }
    Some((col, row - 1))
}

fn col_letters_to_index(s: &str) -> Option<u32> {
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let mut n: u32 = 0;
    for c in s.chars() {
        n = n * 26 + (c.to_ascii_uppercase() as u32 - 'A' as u32 + 1);
    }
    Some(n - 1)
}

#[derive(Default, Clone)]
struct MockWorkbook {
    path: String,
    cells: HashMap<(String, u32, u32), serde_json::Value>,
    saved_to: Vec<String>,
}

/// In-memory [`ExcelBackend`]: no disk I/O, no COM. What every test in
/// this crate runs `excel.*` verbs against.
#[derive(Default)]
pub struct MockExcelBackend {
    next_id: AtomicU64,
    workbooks: Mutex<HashMap<WorkbookId, MockWorkbook>>,
}

impl MockExcelBackend {
    pub fn new() -> Self {
        Self::default()
    }

    /// Test/demo helper: pre-populate a cell as if a real workbook already
    /// held this value, since the mock never parses a real `.xlsx` file.
    pub fn seed_cell(
        &self,
        workbook: WorkbookId,
        sheet: &str,
        row: u32,
        col: u32,
        value: serde_json::Value,
    ) {
        if let Some(book) = self.workbooks.lock().get_mut(&workbook) {
            book.cells.insert((sheet.to_string(), row, col), value);
        }
    }

    /// Every path `save_workbook` was called with, in call order.
    pub fn saved_paths(&self, workbook: WorkbookId) -> Vec<String> {
        self.workbooks
            .lock()
            .get(&workbook)
            .map(|b| b.saved_to.clone())
            .unwrap_or_default()
    }
}

impl ExcelBackend for MockExcelBackend {
    fn open_workbook(&self, path: &str) -> Result<WorkbookId, OfficeError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        self.workbooks.lock().insert(
            id,
            MockWorkbook {
                path: path.to_string(),
                ..Default::default()
            },
        );
        Ok(id)
    }

    fn read_range(
        &self,
        workbook: WorkbookId,
        sheet: &str,
        range: &str,
    ) -> Result<Vec<Vec<serde_json::Value>>, OfficeError> {
        let (c0, r0, c1, r1) = parse_range(range)?;
        let books = self.workbooks.lock();
        let book = books
            .get(&workbook)
            .ok_or(OfficeError::UnknownWorkbook(workbook))?;
        let mut out = Vec::new();
        for row in r0..=r1 {
            let mut out_row = Vec::new();
            for col in c0..=c1 {
                out_row.push(
                    book.cells
                        .get(&(sheet.to_string(), row, col))
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                );
            }
            out.push(out_row);
        }
        Ok(out)
    }

    fn write_range(
        &self,
        workbook: WorkbookId,
        sheet: &str,
        range: &str,
        values: &[Vec<serde_json::Value>],
    ) -> Result<(), OfficeError> {
        let (c0, r0, c1, r1) = parse_range(range)?;
        let expected_rows = (r1 - r0 + 1) as usize;
        let expected_cols = (c1 - c0 + 1) as usize;
        if values.len() != expected_rows || values.iter().any(|row| row.len() != expected_cols) {
            return Err(OfficeError::BadRange {
                range: range.to_string(),
                reason: format!(
                    "values shape does not match the range ({expected_rows}x{expected_cols})"
                ),
            });
        }
        let mut books = self.workbooks.lock();
        let book = books
            .get_mut(&workbook)
            .ok_or(OfficeError::UnknownWorkbook(workbook))?;
        for (i, row) in (r0..=r1).enumerate() {
            for (j, col) in (c0..=c1).enumerate() {
                book.cells
                    .insert((sheet.to_string(), row, col), values[i][j].clone());
            }
        }
        Ok(())
    }

    fn save_workbook(
        &self,
        workbook: WorkbookId,
        path: Option<&str>,
    ) -> Result<String, OfficeError> {
        let mut books = self.workbooks.lock();
        let book = books
            .get_mut(&workbook)
            .ok_or(OfficeError::UnknownWorkbook(workbook))?;
        let target = path.map(String::from).unwrap_or_else(|| book.path.clone());
        book.saved_to.push(target.clone());
        Ok(target)
    }

    fn close_workbook(&self, workbook: WorkbookId) -> Result<(), OfficeError> {
        self.workbooks
            .lock()
            .remove(&workbook)
            .map(|_| ())
            .ok_or(OfficeError::UnknownWorkbook(workbook))
    }
}

/// `excel` namespace adapter.
pub struct ExcelAdapter {
    verbs: Vec<VerbSpec>,
    backend: Arc<dyn ExcelBackend>,
}

impl ExcelAdapter {
    pub fn new(backend: Arc<dyn ExcelBackend>) -> Self {
        Self {
            verbs: build_verbs(),
            backend,
        }
    }

    /// Convenience for tests: an adapter over a fresh [`MockExcelBackend`],
    /// with a handle to that same backend kept alongside so the test can
    /// seed cells or assert on `saved_paths`.
    pub fn mock() -> (Self, Arc<MockExcelBackend>) {
        let backend = Arc::new(MockExcelBackend::new());
        (Self::new(backend.clone()), backend)
    }

    fn call_inner(
        &self,
        verb: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, OfficeError> {
        match verb {
            "open" => self.open(args),
            "read_range" => self.read_range(args),
            "write_range" => self.write_range(args),
            "save" => self.save(args),
            "close" => self.close(args),
            other => unreachable!(
                "AdapterRegistry only dispatches verbs ExcelAdapter::verbs() declared, got `{other}`"
            ),
        }
    }

    fn open(&self, args: &serde_json::Value) -> Result<serde_json::Value, OfficeError> {
        let path = str_arg(args, "path")?;
        let id = self.backend.open_workbook(path)?;
        Ok(json!({ "workbook": id.to_string() }))
    }

    fn read_range(&self, args: &serde_json::Value) -> Result<serde_json::Value, OfficeError> {
        let workbook = handle_arg(args, "workbook")?;
        let sheet = str_arg(args, "sheet")?;
        let range = str_arg(args, "range")?;
        let values = self.backend.read_range(workbook, sheet, range)?;
        Ok(json!({ "values": values }))
    }

    fn write_range(&self, args: &serde_json::Value) -> Result<serde_json::Value, OfficeError> {
        let workbook = handle_arg(args, "workbook")?;
        let sheet = str_arg(args, "sheet")?;
        let range = str_arg(args, "range")?;
        let values: Vec<Vec<serde_json::Value>> = args
            .get("values")
            .and_then(|v| v.as_array())
            .ok_or(OfficeError::MissingArg("values"))?
            .iter()
            .map(|row| row.as_array().cloned().unwrap_or_default())
            .collect();
        self.backend.write_range(workbook, sheet, range, &values)?;
        Ok(json!({ "ok": true }))
    }

    fn save(&self, args: &serde_json::Value) -> Result<serde_json::Value, OfficeError> {
        let workbook = handle_arg(args, "workbook")?;
        let path = args.get("path").and_then(|v| v.as_str());
        let saved_to = self.backend.save_workbook(workbook, path)?;
        Ok(json!({ "saved_to": saved_to }))
    }

    fn close(&self, args: &serde_json::Value) -> Result<serde_json::Value, OfficeError> {
        let workbook = handle_arg(args, "workbook")?;
        self.backend.close_workbook(workbook)?;
        Ok(json!({ "ok": true }))
    }
}

impl Adapter for ExcelAdapter {
    fn namespace(&self) -> &str {
        NAMESPACE
    }

    fn verbs(&self) -> &[VerbSpec] {
        &self.verbs
    }

    fn call(
        &self,
        verb: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, AdapterError> {
        self.call_inner(verb, args)
            .map_err(|e| AdapterError::CallFailed {
                namespace: NAMESPACE.to_string(),
                verb: verb.to_string(),
                message: e.to_string(),
            })
    }
}

fn str_arg<'a>(args: &'a serde_json::Value, key: &'static str) -> Result<&'a str, OfficeError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or(OfficeError::MissingArg(key))
}

/// Handles round-trip through JSON as strings (`{"workbook": "3"}`) since
/// `serde_json::Value` numbers cannot safely carry a full `u64` and the
/// adapter's own JSON contract should not depend on that edge case.
fn handle_arg(args: &serde_json::Value, key: &'static str) -> Result<u64, OfficeError> {
    let raw = str_arg(args, key)?;
    raw.parse::<u64>()
        .map_err(|_| OfficeError::BadHandle(raw.to_string()))
}

fn build_verbs() -> Vec<VerbSpec> {
    let handle_prop = json!({ "type": "string", "minLength": 1 });
    vec![
        VerbSpec::new(
            "open",
            json!({
                "type": "object",
                "required": ["path"],
                "properties": { "path": { "type": "string", "minLength": 1 } },
                "additionalProperties": false
            }),
            RiskClass::Read,
            Idempotency::Idempotent,
        ),
        VerbSpec::new(
            "read_range",
            json!({
                "type": "object",
                "required": ["workbook", "sheet", "range"],
                "properties": {
                    "workbook": handle_prop,
                    "sheet": { "type": "string", "minLength": 1 },
                    "range": { "type": "string", "minLength": 1 }
                },
                "additionalProperties": false
            }),
            RiskClass::Read,
            Idempotency::Idempotent,
        ),
        VerbSpec::new(
            "write_range",
            json!({
                "type": "object",
                "required": ["workbook", "sheet", "range", "values"],
                "properties": {
                    "workbook": handle_prop,
                    "sheet": { "type": "string", "minLength": 1 },
                    "range": { "type": "string", "minLength": 1 },
                    "values": { "type": "array", "items": { "type": "array" } }
                },
                "additionalProperties": false
            }),
            RiskClass::Write,
            Idempotency::Idempotent,
        ),
        VerbSpec::new(
            "save",
            json!({
                "type": "object",
                "required": ["workbook"],
                "properties": {
                    "workbook": handle_prop,
                    "path": { "type": "string", "minLength": 1 }
                },
                "additionalProperties": false
            }),
            RiskClass::Write,
            Idempotency::Idempotent,
        ),
        VerbSpec::new(
            "close",
            json!({
                "type": "object",
                "required": ["workbook"],
                "properties": { "workbook": handle_prop },
                "additionalProperties": false
            }),
            RiskClass::Read,
            // Closing an already-closed handle is a typed error, not
            // silently absorbed (unlike fs.delete's "ensure absent"): a
            // stale handle is usually a caller bug worth surfacing.
            Idempotency::Unknown,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_parsing_handles_single_cells_and_reversed_corners() {
        assert_eq!(parse_range("A1").unwrap(), (0, 0, 0, 0));
        assert_eq!(parse_range("B2").unwrap(), (1, 1, 1, 1));
        assert_eq!(parse_range("A1:C3").unwrap(), (0, 0, 2, 2));
        assert_eq!(
            parse_range("C3:A1").unwrap(),
            (0, 0, 2, 2),
            "corners normalize regardless of order"
        );
        assert_eq!(
            parse_range("AA1").unwrap(),
            (26, 0, 26, 0),
            "multi-letter columns"
        );
    }

    #[test]
    fn range_parsing_rejects_garbage() {
        assert!(parse_range("").is_err());
        assert!(parse_range("1A").is_err());
        assert!(parse_range("A0").is_err(), "rows are 1-indexed");
    }

    #[test]
    fn open_write_read_save_round_trip_on_the_mock() {
        let (adapter, backend) = ExcelAdapter::mock();
        let open = adapter
            .call("open", &json!({ "path": "C:/tmp/book.xlsx" }))
            .unwrap();
        let workbook = open["workbook"].as_str().unwrap().to_string();

        adapter
            .call(
                "write_range",
                &json!({
                    "workbook": workbook, "sheet": "Sheet1", "range": "A1:B2",
                    "values": [["Name", "Total"], ["Acme", 142.50]]
                }),
            )
            .unwrap();

        let read = adapter
            .call(
                "read_range",
                &json!({ "workbook": workbook, "sheet": "Sheet1", "range": "A1:B2" }),
            )
            .unwrap();
        assert_eq!(read["values"], json!([["Name", "Total"], ["Acme", 142.50]]));

        let save = adapter
            .call("save", &json!({ "workbook": workbook }))
            .unwrap();
        assert_eq!(save["saved_to"], json!("C:/tmp/book.xlsx"));
        assert_eq!(
            backend.saved_paths(workbook.parse().unwrap()),
            vec!["C:/tmp/book.xlsx".to_string()]
        );

        adapter
            .call("close", &json!({ "workbook": workbook }))
            .unwrap();
        let err = adapter
            .call(
                "read_range",
                &json!({ "workbook": workbook, "sheet": "Sheet1", "range": "A1" }),
            )
            .unwrap_err();
        assert!(
            matches!(err, AdapterError::CallFailed { .. }),
            "closed handle must not still work"
        );
    }

    #[test]
    fn unset_cells_read_back_as_null() {
        let (adapter, _backend) = ExcelAdapter::mock();
        let open = adapter.call("open", &json!({ "path": "x.xlsx" })).unwrap();
        let workbook = open["workbook"].as_str().unwrap().to_string();
        let read = adapter
            .call(
                "read_range",
                &json!({ "workbook": workbook, "sheet": "Sheet1", "range": "A1:B1" }),
            )
            .unwrap();
        assert_eq!(read["values"], json!([[null, null]]));
    }

    #[test]
    fn write_range_rejects_a_shape_mismatch() {
        let (adapter, _backend) = ExcelAdapter::mock();
        let open = adapter.call("open", &json!({ "path": "x.xlsx" })).unwrap();
        let workbook = open["workbook"].as_str().unwrap().to_string();
        let err = adapter
            .call(
                "write_range",
                &json!({ "workbook": workbook, "sheet": "Sheet1", "range": "A1:B2", "values": [["only one row"]] }),
            )
            .unwrap_err();
        assert!(matches!(err, AdapterError::CallFailed { .. }));
    }

    #[test]
    fn round_trips_through_action_ir_with_write_range_gated_as_write_risk() {
        use crate::{AdapterRegistry, Executor, MockSynthesizer, NoopSleeper};
        use operant_ir::{Action, ActionKind, Grounding, Pace, Retry};

        let (adapter, _backend) = ExcelAdapter::mock();
        let mut adapters = AdapterRegistry::new();
        adapters.register(Box::new(adapter));
        let exec = Executor::with_adapters(MockSynthesizer::new(), adapters)
            .with_sleeper(Box::new(NoopSleeper));

        let mut params = serde_json::Map::new();
        params.insert("namespace".into(), json!("excel"));
        params.insert("verb".into(), json!("open"));
        params.insert("args".into(), json!({ "path": "invoice.xlsx" }));
        let open_action = Action {
            v: 1,
            id: "excel-open".into(),
            kind: ActionKind::AdapterCall,
            intent: None,
            target: None,
            params,
            pace: Pace::Instant,
            risk_class: RiskClass::Read,
            irreversible: false,
            grounding: Grounding::Adapter,
            timeout_ms: 5000,
            retry: Retry {
                attempts: 0,
                backoff_ms: 0,
            },
        };
        let outcome = exec.execute(&open_action, None, None).unwrap();
        let workbook = outcome.adapter_result.unwrap()["workbook"]
            .as_str()
            .unwrap()
            .to_string();

        let mut params = serde_json::Map::new();
        params.insert("namespace".into(), json!("excel"));
        params.insert("verb".into(), json!("write_range"));
        params.insert(
            "args".into(),
            json!({ "workbook": workbook, "sheet": "Sheet1", "range": "A1", "values": [["hi"]] }),
        );
        let write_action = Action {
            risk_class: RiskClass::Write,
            id: "excel-write".into(),
            params,
            ..open_action.clone()
        };
        assert!(exec.execute(&write_action, None, None).is_ok());
    }

    #[test]
    fn namespace_and_verbs_match_the_action_ir_contract() {
        let (adapter, _backend) = ExcelAdapter::mock();
        assert_eq!(adapter.namespace(), "excel");
        let names: Vec<_> = adapter.verbs().iter().map(|v| v.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["open", "read_range", "write_range", "save", "close"]
        );
    }
}
