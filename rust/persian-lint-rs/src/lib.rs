//! Python bindings for `skills/revayat-subtitle/voice/persian/scripts/persian_lint.py`: generated from the signatures in `port.rs` by rust-anything.
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

fn normalize_inner(text: Bound<'_, PyAny>, digits: String) -> Result<serde_json::Value, String> {
    let text: serde_json::Value = crate::pyconv::FromPy::from_py(&text)?;
    let out = port::normalize(text, digits)?;
    Ok(out)
}

/// `normalize`.
#[pyfunction]
#[pyo3(name = "normalize", signature = (text, digits=String::from("persian")))]
fn normalize(py: Python<'_>, text: Bound<'_, PyAny>, digits: String) -> PyResult<Py<PyAny>> {
    normalize_inner(text, digits)
        .map_err(error)
        .and_then(|v| crate::pyconv::ToPy::to_py(&v, py))
}

fn suggestions_inner(text: Bound<'_, PyAny>, style: bool) -> Result<serde_json::Value, String> {
    let text: serde_json::Value = crate::pyconv::FromPy::from_py(&text)?;
    let out = port::suggestions(text, style)?;
    Ok(out)
}

/// `suggestions`.
#[pyfunction]
#[pyo3(name = "suggestions", signature = (text, style=false))]
fn suggestions(py: Python<'_>, text: Bound<'_, PyAny>, style: bool) -> PyResult<Py<PyAny>> {
    suggestions_inner(text, style)
        .map_err(error)
        .and_then(|v| crate::pyconv::ToPy::to_py(&v, py))
}

fn line_changes_inner(
    original: Bound<'_, PyAny>,
    fixed: Bound<'_, PyAny>,
) -> Result<serde_json::Value, String> {
    let original: serde_json::Value = crate::pyconv::FromPy::from_py(&original)?;
    let fixed: serde_json::Value = crate::pyconv::FromPy::from_py(&fixed)?;
    let out = port::line_changes(original, fixed)?;
    Ok(out)
}

/// `line_changes`.
#[pyfunction]
#[pyo3(name = "line_changes", signature = (original, fixed))]
fn line_changes(
    py: Python<'_>,
    original: Bound<'_, PyAny>,
    fixed: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    line_changes_inner(original, fixed)
        .map_err(error)
        .and_then(|v| crate::pyconv::ToPy::to_py(&v, py))
}

#[pymodule]
#[pyo3(name = "_native")]
fn native_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("REVAYAT_NATIVE_CONTRACT", 1u8)?;
    m.add_function(wrap_pyfunction!(normalize, m)?)?;
    m.add_function(wrap_pyfunction!(suggestions, m)?)?;
    m.add_function(wrap_pyfunction!(line_changes, m)?)?;
    Ok(())
}
