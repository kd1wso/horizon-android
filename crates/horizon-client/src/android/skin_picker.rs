//! The Android launcher's offline skin and cape: PNGs the player picks in the system file
//! picker, kept in the launcher's `skins/` folder as `player.png` and `player.cape.png`
//! (`skins::files`), one look for every profile. The game the launcher starts wears them in
//! offline play (`horizon_client::skins::give_offline_look`); a Microsoft account wears its own.
use jni::{
    JNIEnv,
    objects::{JObject, JString},
};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::SystemTime,
};

/// The picker's request codes, which `HorizonActivity` hands back.
pub const SKIN_REQUEST: i32 = 4104;
pub const CAPE_REQUEST: i32 = 4105;
/// Whose files they are in `skins/`.
const NAME: &str = "player";

fn notice(text: impl Into<String>) {
    crate::documents::set_notice(text.into(), false);
}

/// The skin's file and the cape's.
pub fn files() -> (PathBuf, PathBuf) {
    crate::skins::files(&crate::skins::dir(), NAME)
}

/// The look of the next game played offline (`game_launch::LaunchRequest`): the kept skin,
/// with whether `model` makes its arms slim, and the kept cape.
pub fn look(model: crate::skins::Model) -> (Option<(PathBuf, bool)>, Option<PathBuf>) {
    let (skin, cape) = files();
    (
        skin.is_file()
            .then(|| (skin, model == crate::skins::Model::Slim)),
        cape.is_file().then_some(cape),
    )
}

/// Opens the system file picker for a skin PNG, or a cape's.
pub fn begin(cape: bool) -> Result<(), String> {
    crate::platform::hide();
    if !crate::native_ime::pick_image(if cape { CAPE_REQUEST } else { SKIN_REQUEST }) {
        return Err("Android file manager is unavailable".into());
    }
    notice(if cape {
        "Choose a cape PNG (64×32)"
    } else {
        "Choose a skin PNG (64×64 or the older 64×32)"
    });
    Ok(())
}

pub fn picker_failed(error: &str) {
    notice(error);
}

/// Drops the skin (or the cape): the default look shows again.
pub fn remove(cape: bool) -> Result<(), String> {
    let (skin, cape_file) = files();
    let file = if cape { cape_file } else { skin };
    match std::fs::remove_file(&file) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

/// The picker's answer (`documents::document_result`, on Android's main thread): the PNG is
/// read and checked on a worker, then kept.
pub fn result(
    mut env: JNIEnv<'_>,
    activity: JObject<'_>,
    request: i32,
    result: i32,
    uri: JString<'_>,
) {
    let cape = request == CAPE_REQUEST;
    let what = if cape { "Cape" } else { "Skin" };
    if result != -1 || uri.is_null() {
        notice(format!("{what} not changed"));
        return;
    }
    let setup = (|| -> jni::errors::Result<_> {
        Ok((
            env.get_java_vm()?,
            env.new_global_ref(activity)?,
            String::from(env.get_string(&uri)?),
        ))
    })();
    let Ok((vm, activity, uri)) = setup else {
        let _ = env.exception_clear();
        notice(format!(
            "{what} not changed: could not read the selected file"
        ));
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("Skin import".into())
        .spawn(move || {
            let answer = (|| -> Result<(), String> {
                let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
                let (file, _) =
                    crate::document_bridge::open(&mut env, activity.as_obj(), &uri, "skin.png")
                        .map_err(|e| e.to_string())?;
                let mut bytes = Vec::new();
                file.take(crate::skins::MAX_IMPORT_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                keep(&bytes, cape)
            })();
            match answer {
                Ok(()) => notice(format!(
                    "{what} saved. You wear it in offline play, from the next Play."
                )),
                Err(e) => notice(format!("{what} not changed: {e}")),
            }
        });
    if spawned.is_err() {
        notice(format!(
            "{what} not changed: could not start a worker thread"
        ));
    }
}

/// Keeps `bytes` as the skin (or the cape) if they are one, replacing the old file whole.
fn keep(bytes: &[u8], cape: bool) -> Result<(), String> {
    if cape {
        crate::skins::read_cape(bytes)?;
    } else {
        crate::skins::read_skin(bytes)?;
    }
    let dir = crate::skins::dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (skin, cape_file) = files();
    let to = if cape { cape_file } else { skin };
    let part = to.with_extension("partial");
    std::fs::write(&part, bytes).map_err(|e| format!("{}: {e}", part.display()))?;
    std::fs::rename(&part, &to).map_err(|e| format!("{}: {e}", to.display()))
}

/// What the Skins page draws: whether a skin and a cape are kept, the skin's front (16×32
/// texels, `skins::front`) and the cape's (10×16), `None` when there is none (or it no longer
/// reads).
pub struct Preview {
    pub kept: (bool, bool),
    pub skin: Option<Vec<[u8; 4]>>,
    pub cape: Option<Vec<[u8; 4]>>,
}

type Stamp = Option<(SystemTime, u64)>;

/// The preview, decoded again only when a file or the model changes.
pub fn preview(model: crate::skins::Model) -> Arc<Preview> {
    static CACHE: Mutex<Option<((Stamp, Stamp, crate::skins::Model), Arc<Preview>)>> =
        Mutex::new(None);
    let (skin, cape) = files();
    let stamp = |path: &Path| -> Stamp {
        let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
        Some((meta.modified().ok()?, meta.len()))
    };
    let key = (stamp(&skin), stamp(&cape), model);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((cached, preview)) = cache.as_ref()
        && *cached == key
    {
        return preview.clone();
    }
    let preview = Arc::new(Preview {
        kept: (key.0.is_some(), key.1.is_some()),
        skin: key.0.and_then(|_| {
            let bytes = crate::skins::read_file(&skin).ok()?;
            let image = crate::skins::read_skin(&bytes).ok()?;
            Some(crate::skins::front(&image, model))
        }),
        cape: key.1.and_then(|_| {
            let bytes = crate::skins::read_file(&cape).ok()?;
            let image = crate::skins::read_cape(&bytes).ok()?;
            Some(crate::skins::cape_front(&image))
        }),
    });
    *cache = Some((key, preview.clone()));
    preview
}
