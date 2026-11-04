//! Safe world ZIP import/export for the Android launcher.
use std::{fs::{self, File, OpenOptions}, io::{self, Read, Write}, path::{Component, Path, PathBuf}, sync::{Mutex, atomic::{AtomicBool, Ordering}}, time::{SystemTime, UNIX_EPOCH}};
use jni::{JNIEnv, objects::{JObject, JString}};

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
pub fn begin_import(home: PathBuf) -> Result<(), String> {
    begin(Request { home, cache: crate::platform::cache_dir().ok_or("Android cache unavailable")?.join("world-imports"), world: None })
}
pub fn begin_export(home: PathBuf, world: String) -> Result<(), String> {
    if !safe_name(&world) { return Err("Invalid world folder name".into()); }
    begin(Request { home, cache: crate::platform::cache_dir().ok_or("Android cache unavailable")?.join("world-exports"), world: Some(world) })
}
fn begin(request: Request) -> Result<(), String> {
    if BUSY.swap(true, Ordering::AcqRel) { return Err("A world transfer is already in progress".into()); }
    let importing = request.world.is_none();
    *REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = Some(request);
    crate::platform::hide();
    let ok = if importing { crate::native_ime::pick_document() } else { crate::native_ime::create_document("world.zip") };
    if !ok { BUSY.store(false, Ordering::Release); REQUEST.lock().unwrap_or_else(|e| e.into_inner()).take(); return Err("Android file manager is unavailable".into()); }
    notice(if importing { "Choose a world ZIP archive" } else { "Choose where to save the world ZIP" }, false);
    Ok(())
}
pub fn result(mut env: JNIEnv<'_>, activity: JObject<'_>, request: i32, result: i32, uri: JString<'_>) {
    if request != IMPORT_REQUEST { return; }
    let pending = REQUEST.lock().unwrap_or_else(|e| e.into_inner()).take();
    let Some(pending) = pending else { BUSY.store(false, Ordering::Release); return; };
    if result != -1 || uri.is_null() { BUSY.store(false, Ordering::Release); notice("World import cancelled", false); return; }
    let setup = (|| -> jni::errors::Result<_> { Ok((env.get_java_vm()?, env.new_global_ref(activity)?, String::from(env.get_string(&uri)?))) })();
    let Ok((vm, activity, uri)) = setup else { let _ = env.exception_clear(); BUSY.store(false, Ordering::Release); notice("Could not read selected archive", false); return; };
    let _ = std::thread::Builder::new().name("World import".into()).spawn(move || {
        let answer = (|| -> Result<String, String> {
            let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
            let (input, name) = crate::document_bridge::open(&mut env, activity.as_obj(), &uri, "world.zip").map_err(|e| e.to_string())?;
            import_zip(&pending, input, name)
        })();
        BUSY.store(false, Ordering::Release);
        match answer { Ok(name) => notice(format!("Imported world {name} into saves/"), true), Err(e) => notice(format!("World import failed: {e}"), false) }
    });
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
