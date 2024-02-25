use std::{
    io::Read as _,
    path::{Path, PathBuf},
    sync::{Arc, Once},
};

use ast::{Command, Pipe, PipelineDefinition};

use modules::{Context, Input};

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

pub struct Bundle {
    _context: Arc<Context>,
    pipe: Pipe,
}

pub static mut INIT: Option<MainPythonInterpreter> = None;

fn _init_py() {
    if unsafe { INIT.is_some() } {
        return;
    }

    unsafe {
        let mut config = OxidizedPythonInterpreterConfig::default();
        config.interpreter_config.isolated = Some(true);
        config.argv = Some(vec![]);
        let interp = MainPythonInterpreter::new(config).unwrap();

        if let Ok(virtual_env) = std::env::var("VIRTUAL_ENV") {
            interp.with_gil(|py| {
                let syspath: &PyList = py.import("sys")
                    .unwrap()
                    .getattr("path")
                    .unwrap()
                    .downcast()
                    .unwrap();
                syspath.append(format!("{}/lib/python3.11/site-packages", virtual_env).into_py(py)).unwrap();
            });
        }
        
        INIT = Some(interp);
    }
}

#[inline(always)]
pub fn init_py() {
    _init_py()
}

impl Bundle {
    pub fn from_bundle<P: AsRef<Path>>(bundle_path: P) -> Result<Bundle, Arc<anyhow::Error>> {
        init_py();

        // For writing to a file when debugging as a dynamic library
        // let f = File::create("/tmp/divvun_runtime.log").unwrap();
        // tracing_subscriber::fmt()
        //     .with_writer(f)
        //     .without_time()
        //     .init();

        let temp_dir = tempfile::tempdir().map_err(|e| Arc::new(e.into()))?;
        let box_file =
            box_format::BoxFileReader::open(bundle_path).map_err(|e| Arc::new(e.into()))?;
        let context = Arc::new(Context {
            data: modules::DataRef::BoxFile(Box::new(box_file), temp_dir),
        });

        let mut file = context
            .load_file("pipeline.py")
            .map_err(|e| Arc::new(e.into()))?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)
            .map_err(|e| Arc::new(e.into()))?;

        let defn = crate::py_rt::interpret_pipeline(&buf).map_err(|e| Arc::new(e.into()))?;
        let pipe = Pipe::new(context.clone(), Arc::new(defn))?;

        Ok(Bundle {
            _context: context,
            pipe,
        })
    }

    pub fn from_path<P: AsRef<Path>>(contents_path: P) -> Result<Bundle, Arc<anyhow::Error>> {
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

        let mut file = std::fs::File::open(fp).map_err(|e| Arc::new(e.into()))?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)
            .map_err(|e| Arc::new(e.into()))?;

        let defn = crate::py_rt::interpret_pipeline(&buf).map_err(|e| Arc::new(e.into()))?;
        let pipe = Pipe::new(context.clone(), Arc::new(defn))?;

        Ok(Bundle {
            _context: context,
            pipe,
        })
    }

    pub async fn run_pipeline(&self, input: Input) -> Result<Input, Arc<anyhow::Error>> {
        tracing::info!("Running pipeline");
        let result = self.pipe.forward(input).await?;
        tracing::info!("Finished pipeline");
        Ok(result)
    }

    pub async fn run_pipeline_with_tap(
        &self,
        input: Input,
        tap: fn((usize, usize), &Command, &Input),
    ) -> Result<Input, Arc<anyhow::Error>> {
        tracing::info!("Running pipeline");
        let result = self.pipe.forward_tap(input, tap).await?;
        tracing::info!("Finished pipeline");
        Ok(result)
    }

    pub fn definition(&self) -> &Arc<PipelineDefinition> {
        &self.pipe.defn
    }
}

// #[no_mangle]
// extern "C" fn bundle_load(path: *const c_char) -> *mut Bundle {
//     OsLogger::new("nu.necessary.Divvun")
//         .level_filter(LevelFilter::Debug)
//         .init()
//         .unwrap();

//     let bytes = unsafe { CStr::from_ptr(path).to_bytes() };
//     let path = std::str::from_utf8(bytes).unwrap();

//     let bundle = Bundle::load(Path::new(path)).unwrap();
//     // std::env::set_current_dir(bundle.path()).unwrap();

//     tracing::info!("Load bundle: {:?}", bundle.path());

//     Box::into_raw(Box::new(bundle))
// }

// #[no_mangle]
// extern "C" fn bundle_run_pipeline(bundle: *mut Bundle, input: *const c_char) -> *mut c_char {
//     let bytes = unsafe { CStr::from_ptr(input).to_bytes() };
//     let input = std::str::from_utf8(bytes).unwrap();

//     // tracing::info!("Run pipeline: {:?}", context.path);

//     let rt = tokio::runtime::Runtime::new().unwrap();
//     let res = rt
//         .handle()
//         .block_on(unsafe { bundle.as_ref().unwrap() }.run_pipeline(input.to_string().into()));

//     // let Ok(res) = res else {
//     //     // Return empty string
//     //     return CString::new("nope").unwrap().into_raw();
//     // };

//     let res = match res {
//         Ok(result) => result.try_into_string().unwrap(),
//         Err(error) => {
//             return CString::new(format!("Error: {}", error))
//                 .unwrap()
//                 .into_raw()
//         }
//     };

//     CString::new(res).unwrap().into_raw()
// }

// #[no_mangle]
// extern "C" fn bundle_run_pipeline_bytes(bundle: *mut Bundle, input: FfiString) -> FfiSlice {
//     log::error!("MCPLS HI");
//     let input = unsafe { input.as_str() }.to_string();
//     let bundle = unsafe { bundle.as_ref().unwrap() };
//     log::error!("MCPLS HI 2 {:?}", input);

//     let rt = tokio::runtime::Runtime::new().unwrap();
//     log::error!("MCPLS HI 4");

//     std::panic::set_hook(Box::new(|err| {
//         if let Some(x) = err.payload().downcast_ref::<&str>() {
//             log::error!("MCPLS HI 5 {:?}", &x);
//         }
//         if let Some(x) = err.payload().downcast_ref::<String>() {
//             log::error!("MCPLS HI 5 {:?}", &x);
//         }
//         log::error!("PANIC");
//     }));

//     let res = rt.handle().block_on(bundle.run_pipeline(input.into()));

//     log::error!("MCPLS HI 5 {:?}", &res);
//     let res = match res {
//         Ok(result) => result.try_into_bytes().unwrap(),
//         Err(_error) => {
//             return FfiSlice {
//                 data: std::ptr::null_mut(),
//                 len: 0,
//             };
//         }
//     };

//     // Box::into_raw(res.into_boxed_slice()) as *mut _
//     FfiSlice::from(res)
// }

// #[repr(C)]
// #[derive(Debug)]
// pub struct FfiString {
//     data: *mut u8,
//     len: usize,
// }

// impl FfiString {
//     unsafe fn as_str(&self) -> &str {
//         std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.data as *const u8, self.len))
//     }
// }

// impl From<String> for FfiString {
//     fn from(data: String) -> FfiString {
//         FfiString {
//             len: data.len(),
//             data: Box::into_raw(data.into_boxed_str()) as *mut _,
//         }
//     }
// }

// #[repr(C)]
// #[derive(Debug)]
// pub struct FfiSlice {
//     data: *mut u8,
//     len: usize,
// }

// impl FfiSlice {
//     unsafe fn as_bytes(&self) -> &[u8] {
//         std::slice::from_raw_parts(self.data as *const u8, self.len)
//     }
// }

// impl From<Vec<u8>> for FfiSlice {
//     fn from(data: Vec<u8>) -> FfiSlice {
//         FfiSlice {
//             len: data.len(),
//             data: Box::into_raw(data.into_boxed_slice()) as *mut _,
//         }
//     }
// }
