use std::{
    ffi::{c_char, CStr, CString},
    io::Read as _,
    path::{Path, PathBuf},
    sync::{Arc, Once},
};

use ast::{Command, Pipe, PipelineDefinition};

use box_format::OpenError;
use modules::{Context, Input, Module};

use pyembed::{MainPythonInterpreter, OxidizedPythonInterpreterConfig};
use pyo3::{types::PyList, IntoPy};
use tempfile::TempDir;

pub mod ast;
pub mod modules;
pub mod py;
pub mod py_rt;
pub mod repl;
mod util;

pub enum BundleContentsPath {
    TempDir(TempDir),
    Literal(PathBuf),
}

impl BundleContentsPath {
    pub fn path(&self) -> &Path {
        match self {
            BundleContentsPath::TempDir(p) => p.path(),
            BundleContentsPath::Literal(p) => p,
        }
    }
}

pub fn print_modules() {
    for module in inventory::iter::<Module>() {
        println!("{}", module);
    }
}

pub struct Bundle {
    _context: Arc<Context>,
    pipe: Pipe,
}

pub static mut PYTHON: Option<MainPythonInterpreter> = None;

fn _init_py() {
    if unsafe { PYTHON.is_some() } {
        return;
    }

    unsafe {
        let mut config = OxidizedPythonInterpreterConfig::default();
        config.interpreter_config.isolated = Some(true);
        config.argv = Some(vec![]);
        let interp = MainPythonInterpreter::new(config).unwrap();

        if let Ok(virtual_env) = std::env::var("VIRTUAL_ENV") {
            interp.with_gil(|py| {
                let syspath: &PyList = py
                    .import("sys")
                    .unwrap()
                    .getattr("path")
                    .unwrap()
                    .downcast()
                    .unwrap();
                syspath
                    .append(format!("{}/lib/python3.11/site-packages", virtual_env).into_py(py))
                    .unwrap();
            });
        }

        PYTHON = Some(interp);
    }
}

#[inline(always)]
pub fn init_py() {
    _init_py()
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Ast(#[from] ast::Error),
    #[error("{0}")]
    Command(#[from] modules::Error),
    #[error("{0}")]
    PyRt(#[from] py_rt::Error),
    #[error("{0}")]
    Bundle(#[from] OpenError),
}

impl Bundle {
    #[cfg(not(feature = "ffi"))]
    pub fn from_bundle<P: AsRef<Path>>(bundle_path: P) -> Result<Bundle, Error> {
        Self::_from_bundle(bundle_path)
    }

    fn _from_bundle<P: AsRef<Path>>(bundle_path: P) -> Result<Bundle, Error> {
        init_py();

        // For writing to a file when debugging as a dynamic library
        // let f = File::create("/tmp/divvun_runtime.log").unwrap();
        // tracing_subscriber::fmt()
        //     .with_writer(f)
        //     .without_time()
        //     .init();

        let temp_dir = tempfile::tempdir()?;
        let box_file = box_format::BoxFileReader::open(bundle_path)?;
        let context = Arc::new(Context {
            data: modules::DataRef::BoxFile(Box::new(box_file), temp_dir),
        });

        let mut file = context.load_file("pipeline.py")?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;

        let defn = crate::py_rt::interpret_pipeline(&buf)?;
        let pipe = Pipe::new(context.clone(), Arc::new(defn))?;

        Ok(Bundle {
            _context: context,
            pipe,
        })
    }

    #[cfg(not(feature = "ffi"))]
    pub fn from_path<P: AsRef<Path>>(contents_path: P) -> Result<Bundle, Error> {
        Bundle::_from_path(contents_path)
    }

    fn _from_path<P: AsRef<Path>>(contents_path: P) -> Result<Bundle, Error> {
        init_py();

        let (fp, base) = if contents_path.as_ref().is_dir() {
            (
                contents_path.as_ref().join("pipeline.py"),
                contents_path.as_ref(),
            )
        } else {
            (
                contents_path.as_ref().to_path_buf(),
                contents_path.as_ref().parent().unwrap(),
            )
        };

        let context = Arc::new(Context {
            data: modules::DataRef::Path(base.to_path_buf()),
        });

        let mut file = std::fs::File::open(fp)?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;

        let defn = crate::py_rt::interpret_pipeline(&buf)?;
        let pipe = Pipe::new(context.clone(), Arc::new(defn))?;

        Ok(Bundle {
            _context: context,
            pipe,
        })
    }

    #[cfg(not(feature = "ffi"))]
    pub async fn run_pipeline(&self, input: Input) -> Result<Input, Error> {
        self._run_pipeline(input).await
    }

    async fn _run_pipeline(&self, input: Input) -> Result<Input, Error> {
        tracing::info!("Running pipeline");
        let result = self.pipe.forward(input).await?;
        tracing::info!("Finished pipeline");
        Ok(result)
    }

    pub async fn run_pipeline_with_tap(
        &self,
        input: Input,
        tap: fn((usize, usize), &Command, &Input),
    ) -> Result<Input, Error> {
        tracing::info!("Running pipeline");
        let result = self.pipe.forward_tap(input, tap).await?;
        tracing::info!("Finished pipeline");
        Ok(result)
    }

    pub fn definition(&self) -> &Arc<PipelineDefinition> {
        &self.pipe.defn
    }
}

#[cfg(feature = "ffi")]
use cffi::{marshal, FromForeign, ToForeign};

#[cfg(feature = "ffi")]
// #[cffi::marshal]
// impl Bundle {
#[marshal(return_marshaler = cffi::ArcMarshaler::<Bundle>)]
pub fn dr__bundle__from_bundle(
    #[marshal(cffi::StrMarshaler)] bundle_path: &str,
) -> Result<Arc<Bundle>, Box<dyn std::error::Error>> {
    Bundle::_from_bundle(bundle_path)
        .map(Arc::new)
        .map_err(|e| Box::new(e) as _)
}

#[cfg(feature = "ffi")]
type U8VecMarshaler = cffi::VecMarshaler<u8>;
#[cfg(feature = "ffi")]
type BundleArcMarshaler = cffi::ArcMarshaler<Bundle>;
#[cfg(feature = "ffi")]
type BundleArcRefMarshaler = cffi::ArcRefMarshaler<Bundle>;

#[cfg(feature = "ffi")]
#[marshal(return_marshaler = U8VecMarshaler)]
pub fn dr__bundle__run_pipeline_bytes(
    #[marshal(BundleArcRefMarshaler)] bundle: Arc<Bundle>,
    #[marshal(cffi::StrMarshaler)] string: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let result = tokio::runtime::Handle::current()
        .block_on(bundle._run_pipeline(Input::String(string.to_string())))?;
    Ok(result.try_into_bytes()?)
}


#[cfg(feature = "ffi")]
#[marshal(return_marshaler = BundleArcMarshaler)]
pub fn dr__bundle__from_path(
    #[marshal(cffi::StrMarshaler)] path: &str,
) -> Result<Arc<Bundle>, Box<dyn std::error::Error>> {
    Bundle::_from_path(path)
        .map(Arc::new)
        .map_err(|e| Box::new(e) as _)
}
