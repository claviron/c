//! Explicit native I/O executor, shared by application handles.
use crate::abi::{Result, Status};
use tokio::runtime::{Handle, Runtime};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Execution {
    MultiThread = 0,
    CurrentThread = 1,
}

impl Execution {
    pub fn validate(
        code: u32,
        threads: u32,
    ) -> Result<Self> {
        match (code, threads) {
            (0, 1..) => Ok(Self::MultiThread),
            (1, 1) => Ok(Self::CurrentThread),
            _ => Err(Status::InvalidArgument),
        }
    }
}

enum Driver {
    MultiThread(Runtime),
    CurrentThread {
        stop: CancellationToken,
        thread: std::thread::JoinHandle<()>,
    },
}

pub struct NativeRuntime {
    pub executor: Handle,
    driver: Option<Driver>,
}

impl NativeRuntime {
    pub fn open(
        execution: Execution,
        threads: u32,
    ) -> Result<Self> {
        Execution::validate(execution as u32, threads)?;
        let mut builder = match execution {
            Execution::MultiThread => {
                let mut builder = tokio::runtime::Builder::new_multi_thread();
                builder.worker_threads(threads as usize);
                builder
            }
            Execution::CurrentThread => {
                tokio::runtime::Builder::new_current_thread()
            }
        };
        let runtime =
            builder.enable_all().build().map_err(|_| Status::Transport)?;
        let executor = runtime.handle().clone();
        let driver = match execution {
            Execution::MultiThread => Driver::MultiThread(runtime),
            Execution::CurrentThread => {
                let stop = CancellationToken::new();
                let ending = stop.clone();
                let thread = std::thread::Builder::new()
                    .name("orbit-c-io".into())
                    .spawn(move || runtime.block_on(ending.cancelled()))
                    .map_err(|_| Status::Transport)?;
                Driver::CurrentThread { stop, thread }
            }
        };
        Ok(Self { executor, driver: Some(driver) })
    }
}

impl Drop for NativeRuntime {
    fn drop(&mut self) {
        match self.driver.take() {
            Some(Driver::CurrentThread { stop, thread }) => {
                stop.cancel();
                let _ = thread.join();
            }
            Some(Driver::MultiThread(runtime)) => drop(runtime),
            None => {}
        }
    }
}
