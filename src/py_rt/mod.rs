use std::sync::Arc;

use once_cell::sync::Lazy;
use pyembed::{MainPythonInterpreter, OxidizedPythonInterpreterConfig};
use pyo3::types::{PyDict, PyTuple};
use tempfile::tempdir;

use crate::ast::PipelineDefinition;

pub fn dump_ast(input: &str) -> Result<serde_json::Value, anyhow::Error> {
    use pyo3::prelude::*;

    let tmp = tempdir().unwrap();
    crate::py::generate(tmp.path().join("divvun_runtime")).unwrap();

    let py_res: PyResult<Option<serde_json::Value>> = Python::with_gil(|py| {
        let sys = py.import("sys")?;
        let locals = PyDict::new(py);
        let globals = PyDict::new(py);
        locals.set_item("sys", sys)?;

        py.eval(
            &format!("sys.path.append({:?})", tmp.path().display()),
            Some(globals),
            Some(locals),
        )?;

        let pipeline_mod = PyModule::from_code(py, input, "pipeline.py", "pipeline")?;
        let divvun_runtime_mod = py.import("divvun_runtime")?;

        let callback: Option<PyObject> = pipeline_mod
            .dict()
            .iter()
            .filter_map(|(_k, v)| {
                if v.hasattr("_is_pipeline").unwrap() {
                    Some(v.into_py(py))
                } else {
                    None
                }
            })
            .next();

        if let Some(callback) = callback {
            let res = callback.call0(py)?;
            let res = res.downcast::<PyTuple>(py)?;
            let res = divvun_runtime_mod
                .getattr("_to_json")?
                .call(res, None)?
                .extract::<String>()
                .unwrap();
            return Ok(Some(serde_json::from_str(&res).unwrap()));
        } else {
            println!("NO CALL");
        }

        Ok(None)
    });

    Ok(py_res.unwrap().unwrap())
}

pub fn interpret_pipeline(input: &str) -> Result<PipelineDefinition, anyhow::Error> {
    let res = dump_ast(input)?;
    let pd: PipelineDefinition = serde_json::from_value(res).unwrap();
    return Ok(pd);
}
