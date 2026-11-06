//! Shader packs built into the APK from `shaders/android-native/` (docs/android-content.md):
//! the launcher puts each into the profile's `shaderpacks/`, where Installed and the game's
//! Shader Packs page list it. Each version of a pack is put there once, so one the player
//! deletes stays deleted until the APK brings a new version, and a file of the same name
//! that the player put there is never overwritten.
use std::{fs, io, path::Path, sync::OnceLock};

/// The record of what was put in a profile: `name<TAB>digest` lines.
pub const RECORD: &str = "android-bundled-shaders.txt";

pub struct Pack {
    /// Its file name in `shaderpacks/`.
    pub name: String,
    pub bytes: Vec<u8>,
}

static APK: OnceLock<Vec<Pack>> = OnceLock::new();

/// Keeps the packs the APK carries (`android_main` reads them at start).
pub fn remember(packs: Vec<Pack>) {
    let _ = APK.set(packs);
}

/// Whether `name` in `shaderpacks/` is one of the APK's packs.
pub fn is_bundled(name: &str) -> bool {
    APK.get()
        .is_some_and(|packs| packs.iter().any(|p| p.name == name))
}

/// `install` with the APK's packs.
pub fn install_remembered(game: &Path) -> io::Result<Vec<String>> {
    install(game, APK.get().map_or(&[], Vec::as_slice))
}

/// A file name with no folder in it (the APK's names are checked when it is built, too).
fn plain(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\', ':', '\0'])
        && !name.chars().any(char::is_control)
}

/// FNV-1a: stable across builds and toolchains, enough to tell versions of a file apart.
fn digest(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{hash:016x}-{}", bytes.len())
}

fn read_record(path: &Path) -> Vec<(String, String)> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(name, digest)| (name.to_owned(), digest.to_owned()))
        .collect()
}

/// Puts the packs that are new to `game` (a profile's folder) into its `shaderpacks/`;
/// returns the names written.
pub fn install(game: &Path, packs: &[Pack]) -> io::Result<Vec<String>> {
    install_to(game, packs, "shaderpacks", RECORD, "android-bundled-shader")
}

/// Shared installation logic for APK shader and resource packs.
pub fn install_to(
    game: &Path,
    packs: &[Pack],
    directory: &str,
    record_name: &str,
    partial_name: &str,
) -> io::Result<Vec<String>> {
    if packs.is_empty() {
        return Ok(Vec::new());
    }
    let record_path = game.join(record_name);
    let mut record = read_record(&record_path);
    let folder = game.join(directory);
    let (mut written, mut changed) = (Vec::new(), false);
    for pack in packs {
        if !plain(&pack.name) {
            continue;
        }
        let new = digest(&pack.bytes);
        let old = record.iter().position(|(name, _)| *name == pack.name);
        if old.is_some_and(|i| record[i].1 == new) {
            continue;
        }
        let target = folder.join(&pack.name);
        let write = match fs::read(&target) {
            Ok(current) => {
                let current = digest(&current);
                // Only an earlier version that this put there is replaced.
                current != new && old.is_some_and(|i| record[i].1 == current)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => true,
            Err(error) => return Err(error),
        };
        if write {
            fs::create_dir_all(&folder)?;
            let partial = game.join(format!("{partial_name}.partial"));
            fs::write(&partial, &pack.bytes)?;
            fs::rename(&partial, &target)?;
            written.push(pack.name.clone());
        }
        match old {
            Some(i) => record[i].1 = new,
            None => record.push((pack.name.clone(), new)),
        }
        changed = true;
    }
    if changed {
        let text: String = record
            .iter()
            .map(|(name, digest)| format!("{name}\t{digest}\n"))
            .collect();
        let partial = game.join(format!("{record_name}.partial"));
        fs::write(&partial, text)?;
        fs::rename(&partial, &record_path)?;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "horizon-bundled-shaders-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn pack(bytes: &[u8]) -> Vec<Pack> {
        vec![Pack {
            name: "Lux.zip".into(),
            bytes: bytes.to_vec(),
        }]
    }
    fn read(game: &Path) -> Option<Vec<u8>> {
        fs::read(game.join("shaderpacks/Lux.zip")).ok()
    }

    #[test]
    fn a_new_profile_gets_the_pack_once() {
        let game = game();
        assert_eq!(install(&game, &pack(b"one")).unwrap(), ["Lux.zip"]);
        assert_eq!(read(&game).as_deref(), Some(&b"one"[..]));
        assert!(install(&game, &pack(b"one")).unwrap().is_empty());
        assert!(!game.join("android-bundled-shader.partial").exists());
    }

    #[test]
    fn a_deleted_pack_stays_deleted_until_a_new_version() {
        let game = game();
        install(&game, &pack(b"one")).unwrap();
        fs::remove_file(game.join("shaderpacks/Lux.zip")).unwrap();
        assert!(install(&game, &pack(b"one")).unwrap().is_empty());
        assert_eq!(read(&game), None);
        assert_eq!(install(&game, &pack(b"two")).unwrap(), ["Lux.zip"]);
        assert_eq!(read(&game).as_deref(), Some(&b"two"[..]));
    }

    #[test]
    fn a_new_version_replaces_the_old_one() {
        let game = game();
        install(&game, &pack(b"one")).unwrap();
        assert_eq!(install(&game, &pack(b"two")).unwrap(), ["Lux.zip"]);
        assert_eq!(read(&game).as_deref(), Some(&b"two"[..]));
    }

    #[test]
    fn the_players_own_file_is_never_overwritten() {
        let game = game();
        fs::create_dir_all(game.join("shaderpacks")).unwrap();
        fs::write(game.join("shaderpacks/Lux.zip"), b"mine").unwrap();
        assert!(install(&game, &pack(b"one")).unwrap().is_empty());
        assert_eq!(read(&game).as_deref(), Some(&b"mine"[..]));
        install(&game, &pack(b"one")).unwrap();
        fs::write(game.join("shaderpacks/Lux.zip"), b"edited").unwrap();
        assert!(install(&game, &pack(b"two")).unwrap().is_empty());
        assert_eq!(read(&game).as_deref(), Some(&b"edited"[..]));
    }

    #[test]
    fn names_that_are_not_plain_are_skipped() {
        let game = game();
        let packs = [Pack {
            name: "../escape.zip".into(),
            bytes: b"x".to_vec(),
        }];
        assert!(install(&game, &packs).unwrap().is_empty());
        assert!(!game.join("escape.zip").exists());
    }
}
