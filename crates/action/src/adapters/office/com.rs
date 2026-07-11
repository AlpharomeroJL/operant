//! Real Office COM automation, behind the `office-com` cargo feature.
//! Late-bound `IDispatch` automation of Excel/Word (ProgID
//! `"Excel.Application"`/`"Word.Application"`): no compile-time type
//! library binding, everything resolved at runtime via
//! `GetIDsOfNames`/`Invoke`, the same technique VBA and every scripting
//! automation of Office uses. This keeps the `windows` dependency to the
//! generic COM/Automation surface (`Win32_System_Com`, `..._Variant`,
//! `..._Ole`) instead of a generated Office type library binding.
//!
//! Not exercised by `cargo test` (this lane's brief: a mock/trait so
//! tests never need Office installed, `docs/specs/action.md`). This file
//! is compiled (`cargo build -p operant-action --features office-com`)
//! but unverified against a real Office install; see FOLLOWUPS in
//! `RESULT.md`. Cell-at-a-time range I/O and a whole-document-text
//! replace (rather than `Word.Find.Execute`, which needs a much larger
//! named-argument `Invoke` call) trade some performance and Word
//! formatting-preservation for a much smaller, more auditable surface of
//! unverified `unsafe` code.

#![allow(non_snake_case)] // mirrors the COM member names being invoked (PascalCase).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;
use windows::core::{GUID, PCWSTR, VARIANT};
use windows::Win32::System::Com::{
    CLSIDFromProgID, CoCreateInstance, CoInitializeEx, CoUninitialize, IDispatch,
    CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, DISPATCH_METHOD, DISPATCH_PROPERTYGET,
    DISPATCH_PROPERTYPUT, DISPPARAMS, EXCEPINFO,
};

use super::excel::{parse_range, ExcelBackend, WorkbookId};
use super::word::{DocumentId, WordBackend};
use super::OfficeError;

/// `DISPID_PROPERTYPUT`: stable OLE Automation ABI constant (`oaidl.h`),
/// not exported by the `windows` crate under that name. Mirrors
/// `real_win.rs`'s `CF_UNICODETEXT` precedent for the same situation.
const DISPID_PROPERTYPUT: i32 = -3;
/// `VARENUM::VT_DISPATCH`: stable ABI constant (`wtypes.h`); the safe
/// `windows_core::VARIANT` wrapper does not expose VT_DISPATCH extraction
/// (only VT_UNKNOWN/VT_BSTR/numeric/VT_BOOL), so [`variant_to_dispatch`]
/// reads the raw union directly, the same pattern the crate's own
/// `TryFrom<&VARIANT> for IUnknown` uses internally for VT_UNKNOWN.
const VT_DISPATCH: u16 = 9;
const LOCALE_USER_DEFAULT: u32 = 0x0400;

thread_local! {
    /// One apartment-threaded `CoInitializeEx` per thread that touches a
    /// COM backend; `CoUninitialize` runs when the thread-local drops at
    /// thread exit.
    static COM_INIT: ComInit = ComInit::new();
}

struct ComInit;

impl ComInit {
    fn new() -> Self {
        unsafe {
            // SAFETY: matched by CoUninitialize in Drop, once per thread.
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        Self
    }
}

impl Drop for ComInit {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn ensure_com_initialized() {
    COM_INIT.with(|_| {});
}

/// STA COM objects (what `CoInitializeEx(COINIT_APARTMENTTHREADED)`
/// creates, above) may only ever be called from the thread that created
/// them. [`super::ExcelBackend`]/[`super::WordBackend`] require
/// `Send + Sync` so an adapter can be shared behind `Arc<dyn ...>`
/// alongside every other adapter in the registry; [`ComExcelBackend`]
/// and [`ComWordBackend`] assert that with an `unsafe impl`, and in
/// return every trait method calls [`ThreadAffinity::check`] first and
/// refuses with a typed [`OfficeError::Com`] on a thread mismatch
/// *before* touching any COM pointer. That check reads only
/// `std::thread::current().id()` and an immutable `Copy` field, so it
/// never itself races; a real cross-thread call is rejected cleanly
/// rather than reaching the COM proxy in an unsupported way.
#[derive(Clone, Copy)]
struct ThreadAffinity(std::thread::ThreadId);

impl ThreadAffinity {
    fn here() -> Self {
        Self(std::thread::current().id())
    }

    fn check(&self) -> Result<(), OfficeError> {
        if std::thread::current().id() == self.0 {
            Ok(())
        } else {
            Err(OfficeError::Com(
                "this Office COM backend is apartment-threaded (STA) and can only be \
                 called from the thread that created it"
                    .into(),
            ))
        }
    }
}

fn com_err(context: &str, e: windows::core::Error) -> OfficeError {
    OfficeError::Com(format!("{context}: {e}"))
}

/// Null-terminated UTF-16 buffer a [`PCWSTR`] can point into. The caller
/// must keep the returned `Vec` alive for as long as the `PCWSTR` is used.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Thin late-bound `IDispatch` wrapper: get/put/call a member by name,
/// resolving its DISPID via `GetIDsOfNames` on every call. No DISPID
/// cache: this is a correctness-first helper for a handful of calls per
/// adapter verb, not a hot loop.
struct Dispatch(IDispatch);

impl Dispatch {
    fn dispid(&self, name: &str) -> Result<i32, OfficeError> {
        let buf = wide(name);
        let pcwstr = PCWSTR::from_raw(buf.as_ptr());
        let mut id = 0i32;
        unsafe {
            self.0
                .GetIDsOfNames(&GUID::zeroed(), &pcwstr, 1, LOCALE_USER_DEFAULT, &mut id)
                .map_err(|e| com_err(&format!("GetIDsOfNames(\"{name}\")"), e))?;
        }
        Ok(id)
    }

    /// # Safety
    /// `args` must be valid `VARIANT`s; ownership of each stays with the
    /// caller (`Invoke` does not take it), matching every call site below
    /// which lets its temporary `Vec<VARIANT>` drop normally afterward.
    unsafe fn invoke(
        &self,
        dispid: i32,
        flags: windows::Win32::System::Com::DISPATCH_FLAGS,
        args: &mut [VARIANT],
        named_put: bool,
    ) -> Result<VARIANT, OfficeError> {
        // DISPPARAMS is documented to want arguments in reverse order
        // (rightmost parameter first).
        args.reverse();
        let mut put_dispid = DISPID_PROPERTYPUT;
        let params = DISPPARAMS {
            rgvarg: args.as_mut_ptr(),
            cArgs: args.len() as u32,
            rgdispidNamedArgs: if named_put {
                &mut put_dispid
            } else {
                std::ptr::null_mut()
            },
            cNamedArgs: if named_put { 1 } else { 0 },
        };
        let mut result = VARIANT::new();
        let mut excep: EXCEPINFO = core::mem::zeroed();
        self.0
            .Invoke(
                dispid,
                &GUID::zeroed(),
                LOCALE_USER_DEFAULT,
                flags,
                &params,
                Some(&mut result),
                Some(&mut excep),
                None,
            )
            .map_err(|e| com_err("Invoke", e))?;
        Ok(result)
    }

    fn get(&self, name: &str) -> Result<VARIANT, OfficeError> {
        let id = self.dispid(name)?;
        unsafe { self.invoke(id, DISPATCH_PROPERTYGET, &mut [], false) }
    }

    fn put(&self, name: &str, value: VARIANT) -> Result<(), OfficeError> {
        let id = self.dispid(name)?;
        let mut args = [value];
        unsafe { self.invoke(id, DISPATCH_PROPERTYPUT, &mut args, true) }?;
        Ok(())
    }

    fn call(&self, name: &str, mut args: Vec<VARIANT>) -> Result<VARIANT, OfficeError> {
        let id = self.dispid(name)?;
        unsafe { self.invoke(id, DISPATCH_METHOD, &mut args, false) }
    }

    fn get_dispatch(&self, name: &str) -> Result<Dispatch, OfficeError> {
        variant_to_dispatch(&self.get(name)?).map(Dispatch)
    }

    fn call_dispatch(&self, name: &str, args: Vec<VARIANT>) -> Result<Dispatch, OfficeError> {
        variant_to_dispatch(&self.call(name, args)?).map(Dispatch)
    }
}

/// Read a `VT_DISPATCH` `VARIANT` (what a COM automation property/method
/// returns for a sub-object, e.g. `Workbooks`, a `Worksheet`, a `Range`)
/// as an owned, AddRef'd [`IDispatch`].
fn variant_to_dispatch(v: &VARIANT) -> Result<IDispatch, OfficeError> {
    unsafe {
        let raw = v.as_raw();
        if raw.Anonymous.Anonymous.vt != VT_DISPATCH {
            return Err(OfficeError::Com(format!(
                "expected an object result (VT_DISPATCH), got vt={}",
                raw.Anonymous.Anonymous.vt
            )));
        }
        let ptr = raw.Anonymous.Anonymous.Anonymous.pdispVal;
        if ptr.is_null() {
            return Err(OfficeError::Com("null IDispatch result".into()));
        }
        // Same pattern windows_core's own `TryFrom<&VARIANT> for IUnknown`
        // uses for VT_UNKNOWN: reinterpret the borrowed raw pointer as a
        // reference to the interface wrapper, then `clone()` (AddRef) so
        // the result outlives this VARIANT.
        let borrowed: &IDispatch = core::mem::transmute(&ptr);
        Ok(borrowed.clone())
    }
}

fn json_to_variant(value: &serde_json::Value) -> VARIANT {
    match value {
        serde_json::Value::Null => VARIANT::new(),
        serde_json::Value::Bool(b) => VARIANT::from(*b),
        serde_json::Value::Number(n) => VARIANT::from(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => VARIANT::from(s.as_str()),
        // Excel/Word cells hold scalars; a nested array/object has no
        // single-cell representation, so it is stringified rather than
        // silently dropped.
        other => VARIANT::from(other.to_string().as_str()),
    }
}

fn variant_to_json(v: &VARIANT) -> Result<serde_json::Value, OfficeError> {
    if v.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    if let Ok(n) = f64::try_from(v) {
        // Distinguish "really a number" from a numeric-looking string by
        // trying the numeric conversion before the string one; VT_R8/I4/
        // etc. convert cleanly, non-numeric strings return an error and
        // fall through to the BSTR branch below.
        return Ok(serde_json::json!(n));
    }
    if let Ok(b) = bool::try_from(v) {
        return Ok(serde_json::json!(b));
    }
    if let Ok(s) = windows::core::BSTR::try_from(v) {
        return Ok(serde_json::json!(s.to_string()));
    }
    Err(OfficeError::Com(
        "unsupported VARIANT type in cell value".into(),
    ))
}

fn create_app(prog_id: &str) -> Result<Dispatch, OfficeError> {
    ensure_com_initialized();
    unsafe {
        let name = windows::core::HSTRING::from(prog_id);
        let clsid = CLSIDFromProgID(&name)
            .map_err(|e| com_err(&format!("CLSIDFromProgID(\"{prog_id}\")"), e))?;
        let dispatch: IDispatch = CoCreateInstance(&clsid, None, CLSCTX_LOCAL_SERVER)
            .map_err(|e| com_err(&format!("CoCreateInstance(\"{prog_id}\")"), e))?;
        Ok(Dispatch(dispatch))
    }
}

fn cell_address(row: u32, col: u32) -> String {
    let mut letters = String::new();
    let mut n = col + 1;
    while n > 0 {
        let rem = (n - 1) % 26;
        letters.insert(0, (b'A' + rem as u8) as char);
        n = (n - 1) / 26;
    }
    format!("{letters}{}", row + 1)
}

/// Real Excel automation. `open_workbook`/`read_range`/`write_range`/
/// `save`/`close` each round-trip through `IDispatch::Invoke`;
/// `close_workbook` is where the COM `Workbook` reference is released
/// (dropping the `Dispatch` releases the underlying `IDispatch`), and
/// `Drop` quits the `Excel.Application` process once the backend itself
/// goes away, per `docs/specs/action.md`'s "releasing COM objects
/// deterministically".
pub struct ComExcelBackend {
    app: Dispatch,
    workbooks: Mutex<HashMap<WorkbookId, Dispatch>>,
    next_id: AtomicU64,
    owner: ThreadAffinity,
}

// SAFETY: see `ThreadAffinity`'s doc comment. Every `ExcelBackend` method
// below calls `self.owner.check()?` before it touches `app`/`workbooks`.
unsafe impl Send for ComExcelBackend {}
unsafe impl Sync for ComExcelBackend {}

impl ComExcelBackend {
    pub fn new() -> Result<Self, OfficeError> {
        let app = create_app("Excel.Application")?;
        app.put("DisplayAlerts", VARIANT::from(false))?;
        Ok(Self {
            app,
            workbooks: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(0),
            owner: ThreadAffinity::here(),
        })
    }

    fn worksheet(&self, workbook: WorkbookId, sheet: &str) -> Result<Dispatch, OfficeError> {
        let books = self.workbooks.lock();
        let wb = books
            .get(&workbook)
            .ok_or(OfficeError::UnknownWorkbook(workbook))?;
        let sheets = wb.get_dispatch("Worksheets")?;
        sheets.call_dispatch("Item", vec![VARIANT::from(sheet)])
    }
}

impl ExcelBackend for ComExcelBackend {
    fn open_workbook(&self, path: &str) -> Result<WorkbookId, OfficeError> {
        self.owner.check()?;
        let workbooks = self.app.get_dispatch("Workbooks")?;
        let wb = workbooks.call_dispatch("Open", vec![VARIANT::from(path)])?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        self.workbooks.lock().insert(id, wb);
        Ok(id)
    }

    fn read_range(
        &self,
        workbook: WorkbookId,
        sheet: &str,
        range: &str,
    ) -> Result<Vec<Vec<serde_json::Value>>, OfficeError> {
        self.owner.check()?;
        let (c0, r0, c1, r1) = parse_range(range)?;
        let ws = self.worksheet(workbook, sheet)?;
        let mut out = Vec::new();
        for row in r0..=r1 {
            let mut out_row = Vec::new();
            for col in c0..=c1 {
                let cell = ws.call_dispatch(
                    "Range",
                    vec![VARIANT::from(cell_address(row, col).as_str())],
                )?;
                out_row.push(variant_to_json(&cell.get("Value")?)?);
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
        self.owner.check()?;
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
        let ws = self.worksheet(workbook, sheet)?;
        for (i, row) in (r0..=r1).enumerate() {
            for (j, col) in (c0..=c1).enumerate() {
                let cell = ws.call_dispatch(
                    "Range",
                    vec![VARIANT::from(cell_address(row, col).as_str())],
                )?;
                cell.put("Value", json_to_variant(&values[i][j]))?;
            }
        }
        Ok(())
    }

    fn save_workbook(
        &self,
        workbook: WorkbookId,
        path: Option<&str>,
    ) -> Result<String, OfficeError> {
        self.owner.check()?;
        let books = self.workbooks.lock();
        let wb = books
            .get(&workbook)
            .ok_or(OfficeError::UnknownWorkbook(workbook))?;
        match path {
            Some(p) => {
                wb.call("SaveAs", vec![VARIANT::from(p)])?;
                Ok(p.to_string())
            }
            None => {
                wb.call("Save", vec![])?;
                let name = wb.get("FullName")?;
                Ok(windows::core::BSTR::try_from(&name)
                    .map(|b| b.to_string())
                    .unwrap_or_default())
            }
        }
    }

    fn close_workbook(&self, workbook: WorkbookId) -> Result<(), OfficeError> {
        self.owner.check()?;
        let mut books = self.workbooks.lock();
        let wb = books
            .remove(&workbook)
            .ok_or(OfficeError::UnknownWorkbook(workbook))?;
        // SaveChanges: false. The adapter's `save` verb is the sanctioned
        // way to persist changes; `close` should not silently write.
        wb.call("Close", vec![VARIANT::from(false)])?;
        Ok(())
    }
}

impl Drop for ComExcelBackend {
    fn drop(&mut self) {
        let _ = self.app.call("Quit", vec![]);
    }
}

/// Real Word automation. `replace_text` reads the whole document's
/// `Content.Text`, replaces in Rust, and writes it back, rather than
/// driving `Word.Find.Execute` (whose `Invoke` call would need many named
/// arguments); this trades per-run formatting preservation for a much
/// smaller unverified `unsafe` surface. See the module doc comment and
/// FOLLOWUPS.
pub struct ComWordBackend {
    app: Dispatch,
    documents: Mutex<HashMap<DocumentId, Dispatch>>,
    next_id: AtomicU64,
    owner: ThreadAffinity,
}

// SAFETY: see `ThreadAffinity`'s doc comment. Every `WordBackend` method
// below calls `self.owner.check()?` before it touches `app`/`documents`.
unsafe impl Send for ComWordBackend {}
unsafe impl Sync for ComWordBackend {}

impl ComWordBackend {
    pub fn new() -> Result<Self, OfficeError> {
        let app = create_app("Word.Application")?;
        app.put("DisplayAlerts", VARIANT::from(0i32))?; // wdAlertsNone
        Ok(Self {
            app,
            documents: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(0),
            owner: ThreadAffinity::here(),
        })
    }

    fn content(&self, document: DocumentId) -> Result<Dispatch, OfficeError> {
        let docs = self.documents.lock();
        let doc = docs
            .get(&document)
            .ok_or(OfficeError::UnknownDocument(document))?;
        doc.get_dispatch("Content")
    }
}

impl WordBackend for ComWordBackend {
    fn open_document(&self, path: &str) -> Result<DocumentId, OfficeError> {
        self.owner.check()?;
        let documents = self.app.get_dispatch("Documents")?;
        let doc = documents.call_dispatch("Open", vec![VARIANT::from(path)])?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        self.documents.lock().insert(id, doc);
        Ok(id)
    }

    fn get_text(&self, document: DocumentId) -> Result<String, OfficeError> {
        self.owner.check()?;
        let text = self.content(document)?.get("Text")?;
        windows::core::BSTR::try_from(&text)
            .map(|b| b.to_string())
            .map_err(|e| com_err("Content.Text was not a string", e))
    }

    fn replace_text(
        &self,
        document: DocumentId,
        find: &str,
        replace: &str,
    ) -> Result<u32, OfficeError> {
        self.owner.check()?;
        if find.is_empty() {
            return Ok(0);
        }
        let content = self.content(document)?;
        let current = windows::core::BSTR::try_from(&content.get("Text")?)
            .map_err(|e| com_err("Content.Text was not a string", e))?
            .to_string();
        let count = current.matches(find).count() as u32;
        if count > 0 {
            content.put(
                "Text",
                VARIANT::from(current.replace(find, replace).as_str()),
            )?;
        }
        Ok(count)
    }

    fn save_document(
        &self,
        document: DocumentId,
        path: Option<&str>,
    ) -> Result<String, OfficeError> {
        self.owner.check()?;
        let docs = self.documents.lock();
        let doc = docs
            .get(&document)
            .ok_or(OfficeError::UnknownDocument(document))?;
        match path {
            Some(p) => {
                doc.call("SaveAs", vec![VARIANT::from(p)])?;
                Ok(p.to_string())
            }
            None => {
                doc.call("Save", vec![])?;
                let name = doc.get("FullName")?;
                Ok(windows::core::BSTR::try_from(&name)
                    .map(|b| b.to_string())
                    .unwrap_or_default())
            }
        }
    }

    fn close_document(&self, document: DocumentId) -> Result<(), OfficeError> {
        self.owner.check()?;
        let mut docs = self.documents.lock();
        let doc = docs
            .remove(&document)
            .ok_or(OfficeError::UnknownDocument(document))?;
        doc.call("Close", vec![VARIANT::from(false)])?;
        Ok(())
    }
}

impl Drop for ComWordBackend {
    fn drop(&mut self) {
        let _ = self.app.call("Quit", vec![]);
    }
}
