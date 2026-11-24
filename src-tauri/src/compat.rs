//! Compatibility layer for the small set of Tauri APIs the download backend
//! used. Keeping these types means the engine, ledger, history, dump and
//! scraper modules keep their existing shape; only the shell around them is
//! Rune.
//!
//! Everything here is backed by the OS/tokio, not by a webview.

use std::any::Any;
use std::future::Future;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use rune::Waker;

use serde::Serialize;
use tokio::sync::mpsc;

use crate::PluckState;

/// One event from the backend to the Rune UI.
#[derive(Clone, Debug)]
pub struct UiEvent {
    pub name: String,
    pub payload: serde_json::Value,
}

pub struct UiReceiver(mpsc::UnboundedReceiver<UiEvent>);

impl UiReceiver {
    pub fn try_recv(&mut self) -> Result<UiEvent, mpsc::error::TryRecvError> {
        self.0.try_recv()
    }
}

pub struct Backend {
    /// Matches Tauri's app_data_dir so cookies/ledger/history carry over.
    pub data_dir: PathBuf,
    /// Directory holding yt-dlp, ffmpeg and yt-dlp-plugins.
    pub resource_dir: PathBuf,
    pub window_visible: AtomicBool,
    events: mpsc::UnboundedSender<UiEvent>,
    waker: Mutex<Option<Waker>>,
}

static BACKEND: OnceLock<Backend> = OnceLock::new();

/// Install the process-wide backend context. Called once before `App::run`.
pub fn init(data_dir: PathBuf, resource_dir: PathBuf) -> UiReceiver {
    let (tx, rx) = mpsc::unbounded_channel();
    BACKEND
        .set(Backend {
            data_dir,
            resource_dir,
            window_visible: AtomicBool::new(true),
            events: tx,
            waker: Mutex::new(None),
        })
        .ok()
        .expect("backend already initialised");
    UiReceiver(rx)
}

pub fn backend() -> &'static Backend {
    BACKEND.get().expect("backend not initialised")
}

pub fn set_window_visible(visible: bool) {
    backend().window_visible.store(visible, Ordering::SeqCst);
}

pub fn set_waker(waker: Waker) {
    *backend().waker.lock().unwrap() = Some(waker);
}

pub fn wake() {
    if let Some(waker) = backend().waker.lock().unwrap().as_ref() {
        waker();
    }
}

/// Reproduce Tauri v2's app_data_dir for the configured identifier, so an
/// upgrade from 5.x finds existing cookies, ledger, history and archives.
pub fn default_data_dir() -> PathBuf {
    let id = "tech.xyruscode.videoplucker";
    #[cfg(windows)]
    {
        let root = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir());
        root.join(id)
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir());
        home.join("Library").join("Application Support").join(id)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let root = std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
            .unwrap_or_else(|| std::env::temp_dir());
        root.join(id)
    }
}

pub fn default_resource_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(std::env::temp_dir)
}

// ---------------------------------------------------------------------------
// Managed state

pub trait ManagedState: Any + Send + Sync {
    fn instance() -> &'static Self;
}

impl ManagedState for PluckState {
    fn instance() -> &'static Self {
        static STATE: OnceLock<PluckState> = OnceLock::new();
        STATE.get_or_init(PluckState::default)
    }
}

pub struct State<T: ManagedState>(&'static T);

impl<T: ManagedState> Deref for State<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.0
    }
}

// ---------------------------------------------------------------------------
// AppHandle and its extension traits

#[derive(Clone, Default)]
pub struct AppHandle;

pub trait Manager {
    fn path(&self) -> PathResolver {
        PathResolver
    }

    fn state<T: ManagedState>(&self) -> State<T> {
        State(T::instance())
    }

    /// Stands in for the main webview window: visible iff the Rune window is.
    fn get_webview_window(&self, _label: &str) -> Option<WebviewWindow> {
        Some(WebviewWindow)
    }

    fn package_info(&self) -> PackageInfo {
        PackageInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

impl Manager for AppHandle {}

pub struct PathResolver;

impl PathResolver {
    pub fn app_data_dir(&self) -> Result<PathBuf, String> {
        Ok(backend().data_dir.clone())
    }
    pub fn resource_dir(&self) -> Result<PathBuf, String> {
        Ok(backend().resource_dir.clone())
    }
}

pub struct WebviewWindow;

impl WebviewWindow {
    pub fn is_visible(&self) -> Result<bool, String> {
        Ok(backend().window_visible.load(Ordering::SeqCst))
    }
}

pub struct PackageInfo {
    pub version: String,
}

pub trait Emitter {
    fn emit<S: Serialize + Send + 'static>(&self, name: &str, payload: S) -> Result<(), String>;
}

impl Emitter for AppHandle {
    fn emit<S: Serialize + Send + 'static>(&self, name: &str, payload: S) -> Result<(), String> {
        let payload = serde_json::to_value(payload).map_err(|e| e.to_string())?;
        let _ = backend().events.send(UiEvent {
            name: name.to_string(),
            payload,
        });
        wake();
        Ok(())
    }
}

pub trait ShellExt {
    fn shell(&self) -> Shell {
        Shell
    }
}

impl ShellExt for AppHandle {}

pub struct Shell;

impl Shell {
    /// A bundled sidecar sits next to the running executable.
    pub fn sidecar(&self, name: &str) -> Result<Command, String> {
        let name = if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_string()
        };
        Ok(Command::new(backend().resource_dir.join(name)))
    }

    pub fn command<P: AsRef<Path>>(&self, path: P) -> Command {
        Command::new(path)
    }
}

// ---------------------------------------------------------------------------
// Process shim. Mirrors tauri-plugin-shell's CommandEvent stream on tokio.

pub mod process {
    use super::*;

    #[derive(Debug)]
    pub struct TerminatedPayload {
        pub code: Option<i32>,
    }

    #[derive(Debug)]
    pub enum CommandEvent {
        Stdout(Vec<u8>),
        Stderr(Vec<u8>),
        Terminated(TerminatedPayload),
    }

    pub struct Receiver(mpsc::UnboundedReceiver<CommandEvent>);

    impl Receiver {
        pub async fn recv(&mut self) -> Option<CommandEvent> {
            self.0.recv().await
        }
    }

    pub struct CommandChild {
        pid: u32,
    }

    impl CommandChild {
        pub fn pid(&self) -> u32 {
            self.pid
        }

        /// Kill the process *and* its ffmpeg children. yt-dlp spawns ffmpeg;
        /// killing only the parent leaves a lock on the output file.
        pub fn kill(&mut self) -> Result<(), String> {
            crate::pluck::kill_tree(self.pid);
            Ok(())
        }
    }

    pub struct Command {
        inner: tokio::process::Command,
    }

    impl Command {
        pub fn new<P: AsRef<Path>>(path: P) -> Self {
            Self {
                inner: tokio::process::Command::new(path.as_ref()),
            }
        }

        pub fn env<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(mut self, k: K, v: V) -> Self {
            self.inner.env(k, v);
            self
        }

        pub fn arg<A: AsRef<std::ffi::OsStr>>(mut self, arg: A) -> Self {
            self.inner.arg(arg);
            self
        }

        pub fn args<I, A>(mut self, args: I) -> Self
        where
            I: IntoIterator<Item = A>,
            A: AsRef<std::ffi::OsStr>,
        {
            self.inner.args(args);
            self
        }

        pub async fn output(mut self) -> Result<std::process::Output, String> {
            // Build the future inside the runtime context, but don't hold the
            // non-Send EnterGuard across the await.
            let future = {
                let _guard = super::runtime().enter();
                let _ = &mut self.inner;
                self.inner
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
            };
            future.await.map_err(|e| e.to_string())
        }

        pub fn spawn(mut self) -> Result<(Receiver, CommandChild), String> {
            let _guard = super::runtime().enter();
            let mut child = self
                .inner
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?;
            let pid = child.id().unwrap_or(0);
            let (tx, rx) = mpsc::unbounded_channel();

            if let Some(stdout) = child.stdout.take() {
                let tx = tx.clone();
                tokio::spawn(async move {
                    use tokio::io::AsyncReadExt;
                    let mut buf = [0u8; 8192];
                    let mut reader = stdout;
                    loop {
                        match reader.read(&mut buf).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                if tx.send(CommandEvent::Stdout(buf[..n].to_vec())).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                });
            }
            if let Some(stderr) = child.stderr.take() {
                let tx = tx.clone();
                tokio::spawn(async move {
                    use tokio::io::AsyncReadExt;
                    let mut buf = [0u8; 8192];
                    let mut reader = stderr;
                    loop {
                        match reader.read(&mut buf).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                if tx.send(CommandEvent::Stderr(buf[..n].to_vec())).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                });
            }
            tokio::spawn(async move {
                let code = child.wait().await.ok().and_then(|s| s.code());
                let _ = tx.send(CommandEvent::Terminated(TerminatedPayload { code }));
            });

            Ok((Receiver(rx), CommandChild { pid }))
        }
    }
}

pub use process::{Command, CommandChild, CommandEvent, Receiver};

// ---------------------------------------------------------------------------
// Async runtime

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

pub mod async_runtime {
    use super::*;

    pub fn spawn<F>(future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        runtime().handle().spawn(future);
    }
}

/// Convenience for code that needs the runtime handle (updater, image loads).
pub fn handle() -> tokio::runtime::Handle {
    runtime().handle().clone()
}

/// Marker used by the UI to pass a no-op AppHandle to backend functions.
pub fn app_handle() -> AppHandle {
    AppHandle
}
