use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::abi::{Result, Status};
use crate::application::{Application, Exchange};
use crate::runtime::NativeRuntime;

#[derive(Default)]
struct Registry {
    next: u64,
    runtimes: HashMap<u64, Arc<NativeRuntime>>,
    applications: HashMap<u64, Application>,
    exchanges: HashMap<u64, Arc<Exchange>>,
}

pub fn insert_runtime(runtime: NativeRuntime) -> Result<u64> {
    let mut registry = registry().lock().map_err(|_| Status::Panic)?;
    let id = registry.identity()?;
    registry.runtimes.insert(id, Arc::new(runtime));
    Ok(id)
}

pub fn runtime(id: u64) -> Result<Arc<NativeRuntime>> {
    registry()
        .lock()
        .map_err(|_| Status::Panic)?
        .runtimes
        .get(&id)
        .cloned()
        .ok_or(Status::InvalidHandle)
}

pub fn remove_runtime(id: u64) -> Result<()> {
    let removed = {
        let mut registry = registry().lock().map_err(|_| Status::Panic)?;
        let runtime =
            registry.runtimes.get(&id).ok_or(Status::InvalidHandle)?;
        // Applications and concurrent open calls retain an external lease.
        if Arc::strong_count(runtime) != 1 {
            return Err(Status::Busy);
        }
        registry.runtimes.remove(&id).ok_or(Status::InvalidHandle)?
    };
    // Runtime shutdown runs on the C caller, outside the registry lock.
    drop(removed);
    Ok(())
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(Mutex::default)
}

impl Registry {
    fn identity(&mut self) -> Result<u64> {
        self.next = self.next.checked_add(1).ok_or(Status::Capacity)?;
        Ok(self.next)
    }
}

pub fn insert_application(application: Application) -> Result<u64> {
    let mut registry = registry().lock().map_err(|_| Status::Panic)?;
    let id = registry.identity()?;
    registry.applications.insert(id, application);
    Ok(id)
}

pub fn application(id: u64) -> Result<Arc<crate::application::State>> {
    registry()
        .lock()
        .map_err(|_| Status::Panic)?
        .applications
        .get(&id)
        .map(|app| Arc::clone(&app.state))
        .ok_or(Status::InvalidHandle)
}

pub fn remove_application(id: u64) -> Result<()> {
    // The application's executor lease drops outside the registry lock.
    let application = {
        let mut registry = registry().lock().map_err(|_| Status::Panic)?;
        let app =
            registry.applications.get(&id).ok_or(Status::InvalidHandle)?;
        if !app.state.releasable() {
            return Err(Status::Busy);
        }
        registry.applications.remove(&id).ok_or(Status::InvalidHandle)?
    };
    drop(application);
    Ok(())
}

pub fn insert_exchange(exchange: Arc<Exchange>) -> Result<u64> {
    let mut registry = registry().lock().map_err(|_| Status::Panic)?;
    let id = registry.identity()?;
    registry.exchanges.insert(id, exchange);
    Ok(id)
}

pub fn exchange(id: u64) -> Result<Arc<Exchange>> {
    registry()
        .lock()
        .map_err(|_| Status::Panic)?
        .exchanges
        .get(&id)
        .cloned()
        .ok_or(Status::InvalidHandle)
}

pub fn remove_exchange(
    app: u64,
    id: u64,
) -> Result<()> {
    let removed = {
        let mut registry = registry().lock().map_err(|_| Status::Panic)?;
        let owner =
            registry.applications.get(&app).ok_or(Status::InvalidHandle)?;
        let exchange =
            registry.exchanges.get(&id).ok_or(Status::InvalidHandle)?;
        if !Arc::ptr_eq(&owner.state, &exchange.application) {
            return Err(Status::InvalidHandle);
        }
        if !exchange.releasable() {
            return Err(Status::Busy);
        }
        registry.exchanges.remove(&id).ok_or(Status::InvalidHandle)?
    };
    drop(removed);
    Ok(())
}

#[cfg(test)]
#[path = "../tests/registry.rs"]
mod tests;
