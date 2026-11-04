//! Safe world ZIP import/export for the Android launcher.
use std::{fs::{self, File, OpenOptions}, io::{self, Read, Seek, Write}, path::{Component, Path, PathBuf}, sync::{Mutex, atomic::{AtomicBool, Ordering}}, time::{SystemTime, UNIX_EPOCH}};
use jni::{JNIEnv, objects::{JObject, JString}};

pub const EXPORT_REQUEST: i32 = 4102;
pub const IMPORT_REQUEST: i32 = 4103;
const MAX_ZIP: u64 = 512 << 20;
const MAX_EXPANDED: u64 = 2 << 30;
const MAX_ENTRIES: usize = 100_000;
static BUSY: AtomicBool = AtomicBool::new(false);
static REQUEST: Mutex<Option<Request>> = Mutex::new(None);
struct Request { home: PathBuf, cache: PathBuf, world: Option<String> }

fn safe_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 180 && !s.starts_with('.') && !s.chars().any(char::is_control)
        && !s.contains(['/', '\\', ':', '\0'])
        && matches!(Path::new(s).components().next(), Some(Component::Normal(_)))
        && Path::new(s).components().count() == 1
}
fn notice(text: impl Into<String>, changed: bool) {
    crate::documents::set_notice(text.into(), changed);
}
pub fn busy() -> bool { BUSY.load(Ordering::Acquire) }
pub fn picker_failed(error: &str) {
    REQUEST.lock().unwrap_or_else(|e| e.into_inner()).take();
    BUSY.store(false, Ordering::Release);
    notice(error, false);
}
pub fn begin_import(home: PathBuf) -> Result<(), String> {
    begin(Request { home, cache: crate::platform::cache_dir().ok_or("Android cache unavailable")?.join("world-imports"), world: None })
}
pub fn begin_export(home: PathBuf, world: String) -> Result<(), String> {
    if !safe_name(&world) { return Err("Invalid world folder name".into()); }
    begin(Request { home, cache: crate::platform::cache_dir().ok_or("Android cache unavailable")?.join("world-exports"), world: Some(world) })
}
fn begin(request: Request) -> Result<(), String> {
    if BUSY.swap(true, Ordering::AcqRel) { return Err("A world transfer is already in progress".into()); }
    // Exports are offered as "<world>.zip"; imports use the ZIP-only picker (IMPORT_REQUEST).
    let title = request.world.as_ref().map(|world| format!("{world}.zip"));
    *REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = Some(request);
    crate::platform::hide();
    let ok = match &title { None => crate::native_ime::pick_world_document(), Some(title) => crate::native_ime::create_document(title) };
    if !ok { BUSY.store(false, Ordering::Release); REQUEST.lock().unwrap_or_else(|e| e.into_inner()).take(); return Err("Android file manager is unavailable".into()); }
    notice(if title.is_none() { "Choose a world ZIP archive" } else { "Choose where to save the world ZIP" }, false);
    Ok(())
}
pub fn result(mut env: JNIEnv<'_>, activity: JObject<'_>, request: i32, result: i32, uri: JString<'_>) {
    if request != IMPORT_REQUEST && request != EXPORT_REQUEST { return; }
    let exporting = request == EXPORT_REQUEST;
    let pending = {
        let mut slot = REQUEST.lock().unwrap_or_else(|e| e.into_inner());
        // A stale result of the other kind must not consume the pending transfer.
        if slot.as_ref().is_some_and(|p| p.world.is_some() != exporting) { return; }
        slot.take()
    };
    let Some(pending) = pending else { BUSY.store(false, Ordering::Release); return; };
    let verb = if exporting { "export" } else { "import" };
    if result != -1 || uri.is_null() { BUSY.store(false, Ordering::Release); notice(format!("World {verb} cancelled"), false); return; }
    let setup = (|| -> jni::errors::Result<_> { Ok((env.get_java_vm()?, env.new_global_ref(activity)?, String::from(env.get_string(&uri)?))) })();
    let Ok((vm, activity, uri)) = setup else { let _ = env.exception_clear(); BUSY.store(false, Ordering::Release); notice(if exporting { "Could not open the selected destination" } else { "Could not read selected archive" }, false); return; };
    notice(if exporting { "Exporting world..." } else { "Importing world..." }, false);
    let spawned = std::thread::Builder::new().name(format!("World {verb}")).spawn(move || {
        let answer = (|| -> Result<String, String> {
            let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
            if exporting {
                let output = crate::document_bridge::create(&mut env, activity.as_obj(), &uri).map_err(|e| e.to_string())?;
                export_zip(&pending, output)
            } else {
                let (input, name) = crate::document_bridge::open(&mut env, activity.as_obj(), &uri, "world.zip").map_err(|e| e.to_string())?;
                import_zip(&pending, input, name)
            }
        })();
        BUSY.store(false, Ordering::Release);
        match answer {
            Ok(name) if exporting => notice(format!("Exported world {name}"), false),
            Ok(name) => notice(format!("Imported world {name} into saves/"), true),
            Err(e) => notice(format!("World {verb} failed: {e}"), false),
        }
    });
    if spawned.is_err() { BUSY.store(false, Ordering::Release); notice(format!("World {verb} failed: could not start a worker thread"), false); }
}
/// Zip `saves/<world>` as `<world>/...` (the layout `install_zip` accepts) and stream it to `output`.
/// The archive is staged in the cache first: document providers may hand out unseekable pipes.
fn export_zip(req: &Request, mut output: File) -> Result<String, String> {
    let name = req.world.clone().ok_or("No world selected")?;
    let world = req.home.join("saves").join(&name);
    let is_dir = fs::symlink_metadata(&world).map_err(|e| e.to_string())?.is_dir();
    if !is_dir || !world.join("level.dat").is_file() { return Err("World folder has no level.dat".into()); }
    fs::create_dir_all(&req.cache).map_err(|e| e.to_string())?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let staged = req.cache.join(format!("world-{stamp}.zip"));
    let file = OpenOptions::new().read(true).write(true).create_new(true).open(&staged).map_err(|e| e.to_string())?;
    let answer = write_zip(&world, &name, file).and_then(|mut file| {
        file.rewind().map_err(|e| e.to_string())?;
        io::copy(&mut file, &mut output).map_err(|e| e.to_string())?;
        output.flush().map_err(|e| e.to_string())?;
        // Providers backed by pipes cannot fsync; the copy above already reported write errors.
        let _ = output.sync_all();
        Ok(())
    });
    let _ = fs::remove_file(&staged);
    answer.map(|()| name)
}
fn write_zip(world: &Path, name: &str, file: File) -> Result<File, String> {
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let (mut entries, mut expanded, mut dirs) = (0usize, 0u64, vec![String::new()]);
    while let Some(dir) = dirs.pop() {
        let mut children = fs::read_dir(world.join(&dir)).and_then(|it| it.collect::<io::Result<Vec<_>>>()).map_err(|e| e.to_string())?;
        children.sort_by_key(fs::DirEntry::file_name);
        for child in children {
            let file_name = child.file_name();
            let Some(part) = file_name.to_str().filter(|p| !p.contains(['\\', ':', '\0']) && !p.chars().any(char::is_control)) else {
                return Err(format!("Unsupported file name in world: {}", file_name.to_string_lossy()));
            };
            // The running game's lock is not part of the world (vanilla backups skip it too).
            if dir.is_empty() && part == "session.lock" { continue; }
            let kind = child.file_type().map_err(|e| e.to_string())?;
            // Links and special files are never exported; import rejects links anyway.
            if !kind.is_dir() && !kind.is_file() { continue; }
            entries += 1;
            if entries > MAX_ENTRIES { return Err("World has too many files".into()); }
            let rel = if dir.is_empty() { part.to_owned() } else { format!("{dir}/{part}") };
            let entry = format!("{name}/{rel}");
            if entry.len() >= 2048 { return Err("World contains a path that is too long".into()); }
            if kind.is_dir() {
                zip.add_directory(entry, options).map_err(|e| e.to_string())?;
                dirs.push(rel);
                continue;
            }
            let mut source = File::open(child.path()).map_err(|e| e.to_string())?.take(MAX_EXPANDED - expanded + 1);
            zip.start_file(entry, options).map_err(|e| e.to_string())?;
            expanded += io::copy(&mut source, &mut zip).map_err(|e| e.to_string())?;
            if expanded > MAX_EXPANDED { return Err("World is larger than 2 GiB".into()); }
        }
    }
    zip.finish().map_err(|e| e.to_string())
}
fn import_zip(req: &Request, mut input: File, name: String) -> Result<String, String> {
    if !name.to_ascii_lowercase().ends_with(".zip") { return Err("Choose a .zip world archive".into()); }
    fs::create_dir_all(&req.cache).map_err(|e| e.to_string())?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let staged = req.cache.join(format!("world-{stamp}.zip"));
    let mut out = OpenOptions::new().write(true).create_new(true).open(&staged).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 65536]; let mut total = 0u64;
    loop { let n = input.read(&mut buf).map_err(|e| e.to_string())?; if n == 0 { break; } total += n as u64; if total > MAX_ZIP { let _=fs::remove_file(&staged); return Err("ZIP exceeds 512 MiB".into()); } out.write_all(&buf[..n]).map_err(|e| e.to_string())?; }
    out.sync_all().map_err(|e| e.to_string())?; drop(out);
    let answer = install_zip(&req.home, &staged); let _ = fs::remove_file(staged); answer
}

fn install_zip(home: &Path, archive: &Path) -> Result<String, String> {
    let file = File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("Invalid ZIP: {e}"))?;
    if zip.is_empty() || zip.len() > MAX_ENTRIES { return Err("Archive has no files or too many entries".into()); }
    let mut root: Option<String> = None; let mut expanded = 0u64; let mut level = false;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let raw = entry.name().to_owned();
        if raw.len() > 2048 || raw.starts_with('/') || raw.contains(['\\', ':', '\0']) || raw.chars().any(char::is_control) || raw.split('/').any(|p| p == "." || p == "..") || entry.is_symlink() { return Err("ZIP contains an unsafe path or link".into()); }
        let parts: Vec<_> = raw.split('/').filter(|p| !p.is_empty()).collect();
        if parts.len() < 2 || !safe_name(parts[0]) { return Err("ZIP must contain one top-level world folder".into()); }
        if root.as_deref().is_some_and(|r| r != parts[0]) { return Err("ZIP contains multiple world folders".into()); }
        root = Some(parts[0].to_owned()); level |= parts.len() == 2 && parts[1] == "level.dat" && !entry.is_dir();
        expanded = expanded.checked_add(entry.size()).ok_or("Expanded size overflow")?;
        if expanded > MAX_EXPANDED { return Err("World expands beyond 2 GiB".into()); }
    }
    if !level { return Err("Archive does not contain world/level.dat".into()); }
    let name = root.ok_or("Missing world folder")?; let saves = home.join("saves"); fs::create_dir_all(&saves).map_err(|e| e.to_string())?;
    if fs::symlink_metadata(&saves).map_err(|e| e.to_string())?.file_type().is_symlink() { return Err("Unsafe saves directory".into()); }
    let mut target_name = name.clone(); let mut target = saves.join(&target_name);
    for i in 1..=10_000 { if !target.exists() { break; } target_name = format!("{name} ({i})"); target = saves.join(&target_name); if i == 10_000 { return Err("Too many worlds with this name".into()); } }
    let stage = saves.join(format!(".world-import-{}-{}", std::process::id(), target_name));
    if stage.exists() { fs::remove_dir_all(&stage).map_err(|e| e.to_string())?; } fs::create_dir(&stage).map_err(|e| e.to_string())?;
    let result = (|| -> Result<(), String> { for i in 0..zip.len() { let mut entry = zip.by_index(i).map_err(|e| e.to_string())?; let raw = entry.name().to_owned(); let rel = raw.split_once('/').map(|(_, p)| p).unwrap_or(""); if rel.is_empty() { continue; } let out = stage.join(rel); if entry.is_dir() { fs::create_dir_all(&out).map_err(|e| e.to_string())?; } else { if let Some(parent) = out.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; } let mut dest = OpenOptions::new().write(true).create_new(true).open(&out).map_err(|e| e.to_string())?; io::copy(&mut entry, &mut dest).map_err(|e| e.to_string())?; dest.sync_all().map_err(|e| e.to_string())?; } } Ok(()) })();
    if let Err(e) = result { let _ = fs::remove_dir_all(&stage); return Err(e); }
    fs::rename(&stage, &target).map_err(|e| { let _ = fs::remove_dir_all(&stage); e.to_string() })?; Ok(target_name)
}
