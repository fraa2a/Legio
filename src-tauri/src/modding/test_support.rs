//! Local HTTP server, zip fixtures, a fake package source and a database-backed profile for modding tests.

use std::{
    cell::Cell,
    collections::{BTreeMap, HashMap},
    fs,
    io::{Cursor, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use zip::{ZipWriter, write::SimpleFileOptions};

use super::dependencies::{InstallRequest, PackageSource, ResolveMode, ResolvedPackage};
use super::ecosystem::EcosystemSchema;
use super::installer::{
    InstallError, InstallOutcome, ModdingContext, ProgressSink, install_packages,
};
use super::package_id::{PackageName, PackageVersionId};
use super::paths::ModdingPaths;
use super::profiles::ProfileLocks;
use crate::database::{CreateGameInput, Database, ModProfile, ProfileMod};

const FIXTURE: &str = include_str!("../../test-fixtures/modding/ecosystem-schema.json");

pub(super) fn zip_bytes(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(content.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

pub(super) fn write_zip(path: &Path, entries: &[(&str, &str)]) {
    std::fs::write(path, zip_bytes(entries)).unwrap();
}

struct FakePackage {
    dependencies: Vec<PackageVersionId>,
    files: Vec<(String, String)>,
}

/// In-memory package repository: metadata for the source side and zip
/// archives for the fetch side.
#[derive(Default)]
pub(super) struct FakeRepo {
    packages: BTreeMap<PackageVersionId, FakePackage>,
    fetches: Cell<usize>,
}

impl FakeRepo {
    pub(super) fn add(&mut self, id: &str, dependencies: &[&str], files: &[(&str, &str)]) {
        self.packages.insert(
            id.parse().unwrap(),
            FakePackage {
                dependencies: dependencies
                    .iter()
                    .map(|value| value.parse().unwrap())
                    .collect(),
                files: files
                    .iter()
                    .map(|(name, content)| ((*name).to_owned(), (*content).to_owned()))
                    .collect(),
            },
        );
    }

    pub(super) fn fetch(
        &self,
        package: &ResolvedPackage,
        destination: &Path,
    ) -> Result<(), String> {
        self.fetches.set(self.fetches.get() + 1);
        let found = self
            .packages
            .get(&package.id)
            .ok_or_else(|| format!("{} is not in the fake repository", package.id))?;
        let entries: Vec<(&str, &str)> = found
            .files
            .iter()
            .map(|(name, content)| (name.as_str(), content.as_str()))
            .collect();
        write_zip(destination, &entries);
        Ok(())
    }

    pub(super) fn fetch_count(&self) -> usize {
        self.fetches.get()
    }

    fn resolved(&self, id: &PackageVersionId) -> ResolvedPackage {
        ResolvedPackage {
            id: id.clone(),
            dependencies: self.packages[id].dependencies.clone(),
            download_url: format!("https://example.invalid/{id}.zip"),
            file_size: 1,
        }
    }
}

impl PackageSource for FakeRepo {
    fn latest(&self, package: &PackageName) -> Result<Option<ResolvedPackage>, String> {
        Ok(self
            .packages
            .keys()
            .filter(|candidate| candidate.package == *package)
            .max_by_key(|candidate| candidate.version)
            .map(|found| self.resolved(found)))
    }

    fn exact(&self, id: &PackageVersionId) -> Result<Option<ResolvedPackage>, String> {
        Ok(self.packages.contains_key(id).then(|| self.resolved(id)))
    }
}

/// A game with one profile in a temporary data directory.
pub(super) struct Env {
    pub(super) database: Database,
    pub(super) paths: ModdingPaths,
    pub(super) locks: ProfileLocks,
    pub(super) schema: EcosystemSchema,
    pub(super) game_key: &'static str,
    pub(super) game_id: String,
    pub(super) profile: ModProfile,
    pub(super) directory: TempDir,
}

impl Env {
    pub(super) fn new() -> Self {
        Self::for_game("lethal-company")
    }

    pub(super) fn for_game(game_key: &'static str) -> Self {
        let directory = TempDir::new();
        let database = Database::open(directory.path()).unwrap();
        let game_id = database
            .create_game(CreateGameInput {
                name: "Game".to_owned(),
                steam_app_id: None,
            })
            .unwrap()
            .id;
        database.set_game_modding(&game_id, game_key).unwrap();
        let paths = ModdingPaths::new(directory.path());
        let profile =
            crate::modding::profiles::create_profile(&database, &paths, &game_id, "Main").unwrap();
        Self {
            database,
            paths,
            locks: ProfileLocks::default(),
            schema: serde_json::from_str(FIXTURE).unwrap(),
            game_key,
            game_id,
            profile,
            directory,
        }
    }

    pub(super) fn ctx(&self) -> ModdingContext<'_> {
        ModdingContext {
            database: &self.database,
            paths: &self.paths,
            locks: &self.locks,
            game: self.schema.games[self.game_key].game_entry().unwrap(),
            loaders: &self.schema.modloader_packages,
        }
    }

    pub(super) fn profile_dir(&self) -> PathBuf {
        self.paths
            .profile_dir(&self.game_id, &self.profile.id)
            .unwrap()
    }

    pub(super) fn install(
        &self,
        repo: &FakeRepo,
        requests: &[&str],
        mode: ResolveMode,
    ) -> Result<InstallOutcome, InstallError> {
        self.install_with(repo, requests, mode, &|_| {})
    }

    pub(super) fn install_with(
        &self,
        repo: &FakeRepo,
        requests: &[&str],
        mode: ResolveMode,
        progress: ProgressSink<'_>,
    ) -> Result<InstallOutcome, InstallError> {
        let requests: Vec<_> = requests.iter().map(|value| request(value)).collect();
        install_packages(
            &self.ctx(),
            &self.profile.id,
            &requests,
            repo,
            &|package, destination| repo.fetch(package, destination),
            mode,
            progress,
        )
    }

    pub(super) fn mods(&self) -> Vec<ProfileMod> {
        self.database.profile_mods(&self.profile.id).unwrap()
    }

    pub(super) fn files_of(&self, name: &str) -> Vec<String> {
        self.database
            .profile_mod_files(&self.profile.id, name)
            .unwrap()
    }

    pub(super) fn exists(&self, relative: &str) -> bool {
        self.profile_dir().join(relative).exists()
    }
}

/// Removes the directory when dropped. Declared after the database in
/// [`Env`] so the database file is closed first.
pub(crate) struct TempDir(pub(crate) PathBuf);

impl TempDir {
    pub(crate) fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("legio-modding-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl std::ops::Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) fn request(value: &str) -> InstallRequest {
    match value.parse::<PackageVersionId>() {
        Ok(id) => InstallRequest {
            package: id.package,
            version: Some(id.version),
        },
        Err(_) => InstallRequest {
            package: value.parse().unwrap(),
            version: None,
        },
    }
}

#[derive(Clone)]
pub(crate) struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    mode: Mode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Sized,
    Chunked,
    // Declares a longer body than it sends, then keeps the connection open.
    Stall,
}

impl Reply {
    pub(crate) fn ok(body: Vec<u8>) -> Self {
        Self::new(200, body, Mode::Sized)
    }

    pub(crate) fn chunked(body: Vec<u8>) -> Self {
        Self::new(200, body, Mode::Chunked)
    }

    pub(crate) fn stall_after(body: Vec<u8>) -> Self {
        Self::new(200, body, Mode::Stall)
    }

    pub(crate) fn status(status: u16) -> Self {
        Self::new(status, Vec::new(), Mode::Sized)
    }

    pub(crate) fn redirect(location: &str) -> Self {
        let mut reply = Self::new(302, Vec::new(), Mode::Sized);
        reply
            .headers
            .push(("Location".to_owned(), location.to_owned()));
        reply
    }

    fn new(status: u16, body: Vec<u8>, mode: Mode) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body,
            mode,
        }
    }
}

struct Received {
    path: String,
    head: String,
    body: Vec<u8>,
}

/// Serves registered paths over plain HTTP on 127.0.0.1 until dropped.
pub(crate) struct TestServer {
    address: String,
    routes: Arc<Mutex<HashMap<String, Reply>>>,
    requests: Arc<Mutex<Vec<Received>>>,
    stop: Arc<AtomicBool>,
}

impl TestServer {
    pub(crate) fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let routes = Arc::new(Mutex::new(HashMap::new()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let accept = (routes.clone(), requests.clone(), stop.clone());
        thread::spawn(move || {
            let (routes, requests, stop) = accept;
            for stream in listener.incoming() {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let (routes, requests, stop) = (routes.clone(), requests.clone(), stop.clone());
                thread::spawn(move || serve(stream, &routes, &requests, &stop));
            }
        });
        Self {
            address,
            routes,
            requests,
            stop,
        }
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }

    pub(crate) fn set(&self, path: &str, reply: Reply) {
        self.routes.lock().unwrap().insert(path.to_owned(), reply);
    }

    /// Paths requested so far, in order.
    pub(crate) fn requests(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|received| received.path.clone())
            .collect()
    }

    /// Request bodies received for `path`.
    pub(crate) fn bodies(&self, path: &str) -> Vec<Vec<u8>> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|received| received.path == path)
            .map(|received| received.body.clone())
            .collect()
    }

    /// Raw request heads (lowercased) received for `path`.
    pub(crate) fn headers(&self, path: &str) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|received| received.path == path)
            .map(|received| received.head.to_ascii_lowercase())
            .collect()
    }

    pub(crate) fn clear_requests(&self) {
        self.requests.lock().unwrap().clear();
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Wakes the accept loop so it can exit.
        let _ = TcpStream::connect(&self.address);
    }
}

fn serve(
    mut stream: TcpStream,
    routes: &Mutex<HashMap<String, Reply>>,
    requests: &Mutex<Vec<Received>>,
    stop: &AtomicBool,
) {
    if stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .is_err()
    {
        return;
    }
    let mut head = Vec::new();
    let mut byte = [0];
    while !head.ends_with(b"\r\n\r\n") {
        if stream.read_exact(&mut byte).is_err() {
            return;
        }
        head.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&head).into_owned();
    let path = head
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    let length = head
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .and_then(|value| value.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    if stream.read_exact(&mut body).is_err() {
        return;
    }
    requests.lock().unwrap().push(Received {
        path: path.clone(),
        head,
        body,
    });
    let reply = routes
        .lock()
        .unwrap()
        .get(&path)
        .cloned()
        .unwrap_or_else(|| Reply::status(404));
    let mut response = format!("HTTP/1.1 {} Test\r\nConnection: close\r\n", reply.status);
    for (name, value) in &reply.headers {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    // Clients may hang up early on oversized bodies, so write errors are expected.
    match reply.mode {
        Mode::Sized => {
            response.push_str(&format!("Content-Length: {}\r\n\r\n", reply.body.len()));
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(&reply.body);
        }
        Mode::Chunked => {
            response.push_str("Transfer-Encoding: chunked\r\n\r\n");
            let _ = stream.write_all(response.as_bytes());
            for part in reply.body.chunks(512) {
                if stream
                    .write_all(format!("{:x}\r\n", part.len()).as_bytes())
                    .and_then(|()| stream.write_all(part))
                    .and_then(|()| stream.write_all(b"\r\n"))
                    .is_err()
                {
                    return;
                }
            }
            let _ = stream.write_all(b"0\r\n\r\n");
        }
        Mode::Stall => {
            response.push_str(&format!(
                "Content-Length: {}\r\n\r\n",
                reply.body.len() + 1000
            ));
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(&reply.body);
            for _ in 0..50 {
                if stop.load(Ordering::Acquire) {
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}
