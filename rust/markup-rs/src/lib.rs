//! Python bindings for `skills/revayat-subtitle/scripts/markup.py`: generated from the signatures in `port.rs` by rust-anything.
//! Regenerate with `rust-anything refresh` after changing them.

#![allow(clippy::too_many_arguments)]

use pyo3::prelude::*;

pub mod js;
pub mod port;
pub mod pyconv;
pub mod pynum;

#[allow(unused_imports)]
use pynum::Num;

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

fn srt_tag_fits_inner(tag: String) -> Result<bool, String> {
    let out = port::srt_tag_fits(tag)?;
    Ok(out)
}

/// `srt_tag_fits`.
#[pyfunction]
#[pyo3(name = "srt_tag_fits", signature = (tag))]
fn srt_tag_fits(tag: String) -> PyResult<bool> {
    srt_tag_fits_inner(tag).map_err(error)
}

fn has_rtl_inner(text: String) -> Result<bool, String> {
    let out = port::has_rtl(text)?;
    Ok(out)
}

/// `has_rtl`.
#[pyfunction]
#[pyo3(name = "has_rtl", signature = (text))]
fn has_rtl(text: String) -> PyResult<bool> {
    has_rtl_inner(text).map_err(error)
}

fn has_ltr_inner(text: String) -> Result<bool, String> {
    let out = port::has_ltr(text)?;
    Ok(out)
}

/// `has_ltr`.
#[pyfunction]
#[pyo3(name = "has_ltr", signature = (text))]
fn has_ltr(text: String) -> PyResult<bool> {
    has_ltr_inner(text).map_err(error)
}

fn srt_font_names_inner(tag: String) -> Result<serde_json::Value, String> {
    let out = port::srt_font_names(tag)?;
    Ok(out)
}

/// `srt_font_names`.
#[pyfunction]
#[pyo3(name = "srt_font_names", signature = (tag))]
fn srt_font_names(py: Python<'_>, tag: String) -> PyResult<Py<PyAny>> {
    srt_font_names_inner(tag)
        .map_err(error)
        .and_then(|v| crate::pyconv::ToPy::to_py(&v, py))
}

fn drawing_mode_inner(block: String, drawing: bool) -> Result<bool, String> {
    let out = port::drawing_mode(block, drawing)?;
    Ok(out)
}

/// `drawing_mode`.
#[pyfunction]
#[pyo3(name = "drawing_mode", signature = (block, drawing))]
fn drawing_mode(block: String, drawing: bool) -> PyResult<bool> {
    drawing_mode_inner(block, drawing).map_err(error)
}

fn uncomment_inner(text: String, kind: String) -> Result<String, String> {
    let out = port::uncomment(text, kind)?;
    Ok(out)
}

/// `uncomment`.
#[pyfunction]
#[pyo3(name = "uncomment", signature = (text, kind))]
fn uncomment(text: String, kind: String) -> PyResult<String> {
    uncomment_inner(text, kind).map_err(error)
}

fn remap_resets_inner(text: String, mapping: Bound<'_, PyAny>) -> Result<String, String> {
    let mapping: serde_json::Value = crate::pyconv::FromPy::from_py(&mapping)?;
    let out = port::remap_resets(text, mapping)?;
    Ok(out)
}

/// `remap_resets`.
#[pyfunction]
#[pyo3(name = "remap_resets", signature = (text, mapping))]
fn remap_resets(text: String, mapping: Bound<'_, PyAny>) -> PyResult<String> {
    remap_resets_inner(text, mapping).map_err(error)
}

fn validate_bidi_inner(text: String, kind: String) -> Result<(), String> {
    port::validate_bidi(text, kind)?;
    Ok(())
}

/// `validate_bidi`.
#[pyfunction]
#[pyo3(name = "validate_bidi", signature = (text, kind))]
fn validate_bidi(text: String, kind: String) -> PyResult<()> {
    validate_bidi_inner(text, kind).map_err(error)
}

fn rtl_inner(text: String, kind: String, direction: String, force: bool) -> Result<String, String> {
    let out = port::rtl(text, kind, direction, force)?;
    Ok(out)
}

/// `rtl`.
#[pyfunction]
#[pyo3(name = "rtl", signature = (text, kind, direction=String::from("rtl"), *, force=false))]
fn rtl(text: String, kind: String, direction: String, force: bool) -> PyResult<String> {
    rtl_inner(text, kind, direction, force).map_err(error)
}

fn clean_empty_lines_inner(text: String, kind: String, preserve: bool) -> Result<String, String> {
    let out = port::clean_empty_lines(text, kind, preserve)?;
    Ok(out)
}

/// `clean_empty_lines`.
#[pyfunction]
#[pyo3(name = "clean_empty_lines", signature = (text, kind, preserve=false))]
fn clean_empty_lines(text: String, kind: String, preserve: bool) -> PyResult<String> {
    clean_empty_lines_inner(text, kind, preserve).map_err(error)
}

fn paragraph_direction_inner(language: String) -> Result<String, String> {
    let out = port::paragraph_direction(language)?;
    Ok(out)
}

/// `paragraph_direction`.
#[pyfunction]
#[pyo3(name = "paragraph_direction", signature = (language))]
fn paragraph_direction(language: String) -> PyResult<String> {
    paragraph_direction_inner(language).map_err(error)
}

#[pymodule]
#[pyo3(name = "_native")]
fn native_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(srt_tag_fits, m)?)?;
    m.add_function(wrap_pyfunction!(has_rtl, m)?)?;
    m.add_function(wrap_pyfunction!(has_ltr, m)?)?;
    m.add_function(wrap_pyfunction!(srt_font_names, m)?)?;
    m.add_function(wrap_pyfunction!(drawing_mode, m)?)?;
    m.add_function(wrap_pyfunction!(uncomment, m)?)?;
    m.add_function(wrap_pyfunction!(remap_resets, m)?)?;
    m.add_function(wrap_pyfunction!(validate_bidi, m)?)?;
    m.add_function(wrap_pyfunction!(rtl, m)?)?;
    m.add_function(wrap_pyfunction!(clean_empty_lines, m)?)?;
    m.add_function(wrap_pyfunction!(paragraph_direction, m)?)?;
    Ok(())
}
