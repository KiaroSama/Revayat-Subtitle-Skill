//! Python bindings for `skills/revayat-subtitle/scripts/text_pipeline.py`: generated from the signatures in `port.rs` by rust-anything.
//! Regenerate with `rust-anything refresh` after changing them.

#![allow(clippy::too_many_arguments)]

use pyo3::prelude::*;

pub mod port;

fn error(message: String) -> PyErr {
    use pyo3::exceptions as e;
    /// `Err("Kind: text")` for a `Kind` the match below does not know: a built-in exception's name or
    /// `package.module.Class`, called with the text. None when it names no `Exception` subclass or the
    /// class cannot be made from the text alone (its constructor takes more arguments).
    fn named_error(kind: &str, text: &str) -> Option<PyErr> {
        Python::attach(|py| {
            // Each dotted part must be a Python identifier, by Python's own rules (`str.isidentifier`).
            let identifier = |part: &str| {
                pyo3::types::PyString::new(py, part)
                    .call_method0("isidentifier")
                    .and_then(|ok| ok.extract::<bool>())
                    .unwrap_or(false)
            };
            if !kind.split('.').all(identifier) {
                return None;
            }
            let (module, name) = kind.rsplit_once('.').unwrap_or(("builtins", kind));
            let class = py.import(module).ok()?.getattr(name).ok()?;
            let class = class.cast_into::<pyo3::types::PyType>().ok()?;
            if !class
                .is_subclass_of::<pyo3::exceptions::PyException>()
                .ok()?
            {
                return None;
            }
            let value = class.call1((text,)).ok()?;
            if !value.is_instance(&class).ok()? {
                return None;
            }
            Some(PyErr::from_value(value))
        })
    }
    let (kind, text) = message.split_once(": ").unwrap_or(("", message.as_str()));
    let text = text.to_string();
    match kind {
        "TypeError" => e::PyTypeError::new_err(text),
        "KeyError" => e::PyKeyError::new_err(text),
        "IndexError" => e::PyIndexError::new_err(text),
        "OverflowError" => e::PyOverflowError::new_err(text),
        "ZeroDivisionError" => e::PyZeroDivisionError::new_err(text),
        "AttributeError" => e::PyAttributeError::new_err(text),
        "RuntimeError" => e::PyRuntimeError::new_err(text),
        "NotImplementedError" => e::PyNotImplementedError::new_err(text),
        "LookupError" => e::PyLookupError::new_err(text),
        "ArithmeticError" => e::PyArithmeticError::new_err(text),
        "UnicodeError" => e::PyUnicodeError::new_err(text),
        "ValueError" => e::PyValueError::new_err(text),
        _ => named_error(kind, &text).unwrap_or_else(|| e::PyValueError::new_err(message)),
    }
}

fn normalize_persian_zwnj_inner(text: String) -> Result<String, String> {
    let out = port::normalize_persian_zwnj(text)?;
    Ok(out)
}

/// `normalize_persian_zwnj`.
#[pyfunction]
#[pyo3(name = "normalize_persian_zwnj", signature = (text))]
fn normalize_persian_zwnj(text: String) -> PyResult<String> {
    normalize_persian_zwnj_inner(text).map_err(error)
}

fn watermark_reason_inner(text: String) -> Result<String, String> {
    let out = port::watermark_reason(text)?;
    Ok(out)
}

/// `watermark_reason`.
#[pyfunction]
#[pyo3(name = "watermark_reason", signature = (text))]
fn watermark_reason(text: String) -> PyResult<String> {
    watermark_reason_inner(text).map_err(error)
}

fn is_watermark_cue_inner(text: String) -> Result<bool, String> {
    let out = port::is_watermark_cue(text)?;
    Ok(out)
}

/// `is_watermark_cue`.
#[pyfunction]
#[pyo3(name = "is_watermark_cue", signature = (text))]
fn is_watermark_cue(text: String) -> PyResult<bool> {
    is_watermark_cue_inner(text).map_err(error)
}

fn convert_srt_markup_to_ass_inner(text: String) -> Result<String, String> {
    let out = port::convert_srt_markup_to_ass(text)?;
    Ok(out)
}

/// `convert_srt_markup_to_ass`.
#[pyfunction]
#[pyo3(name = "convert_srt_markup_to_ass", signature = (text))]
fn convert_srt_markup_to_ass(text: String) -> PyResult<String> {
    convert_srt_markup_to_ass_inner(text).map_err(error)
}

fn wrap_subtitle_lines_inner(
    text: String,
    max_chars: Option<i64>,
    max_lines: Option<i64>,
) -> Result<String, String> {
    let out = port::wrap_subtitle_lines(text, max_chars, max_lines)?;
    Ok(out)
}

/// `wrap_subtitle_lines`.
#[pyfunction]
#[pyo3(name = "wrap_subtitle_lines", signature = (text, max_chars=42, max_lines=2))]
fn wrap_subtitle_lines(
    text: String,
    max_chars: Option<i64>,
    max_lines: Option<i64>,
) -> PyResult<String> {
    wrap_subtitle_lines_inner(text, max_chars, max_lines).map_err(error)
}

fn anchor_rtl_line_inner(
    text: String,
    kind: Option<String>,
    direction: Option<String>,
) -> Result<String, String> {
    let out = port::anchor_rtl_line(text, kind, direction)?;
    Ok(out)
}

/// `anchor_rtl_line`.
#[pyfunction]
#[pyo3(name = "anchor_rtl_line", signature = (text, kind=String::from("ass"), direction=String::from("rtl")))]
fn anchor_rtl_line(
    text: String,
    kind: Option<String>,
    direction: Option<String>,
) -> PyResult<String> {
    anchor_rtl_line_inner(text, kind, direction).map_err(error)
}

fn process_cue_text_inner(
    text: String,
    kind: Option<String>,
    direction: Option<String>,
    max_chars: Option<i64>,
    fix_zwnj: Option<bool>,
    wrap_lines: Option<bool>,
) -> Result<String, String> {
    let out = port::process_cue_text(text, kind, direction, max_chars, fix_zwnj, wrap_lines)?;
    Ok(out)
}

/// `process_cue_text`.
#[pyfunction]
#[pyo3(name = "process_cue_text", signature = (text, kind=String::from("ass"), direction=String::from("rtl"), max_chars=42, fix_zwnj=true, wrap_lines=true))]
fn process_cue_text(
    text: String,
    kind: Option<String>,
    direction: Option<String>,
    max_chars: Option<i64>,
    fix_zwnj: Option<bool>,
    wrap_lines: Option<bool>,
) -> PyResult<String> {
    process_cue_text_inner(text, kind, direction, max_chars, fix_zwnj, wrap_lines).map_err(error)
}

#[pymodule]
#[pyo3(name = "_native")]
fn native_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(normalize_persian_zwnj, m)?)?;
    m.add_function(wrap_pyfunction!(watermark_reason, m)?)?;
    m.add_function(wrap_pyfunction!(is_watermark_cue, m)?)?;
    m.add_function(wrap_pyfunction!(convert_srt_markup_to_ass, m)?)?;
    m.add_function(wrap_pyfunction!(wrap_subtitle_lines, m)?)?;
    m.add_function(wrap_pyfunction!(anchor_rtl_line, m)?)?;
    m.add_function(wrap_pyfunction!(process_cue_text, m)?)?;
    Ok(())
}
