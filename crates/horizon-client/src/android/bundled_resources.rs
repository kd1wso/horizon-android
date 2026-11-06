//! Resource packs supplied in the APK, installed once per version per profile.
use crate::bundled_shaders::{self, Pack};
use std::{io, path::Path, sync::OnceLock};
static APK: OnceLock<Vec<Pack>> = OnceLock::new();
pub fn remember(packs: Vec<Pack>) {
    let _ = APK.set(packs);
}
pub fn is_bundled(name: &str) -> bool {
    APK.get()
        .is_some_and(|packs| packs.iter().any(|p| p.name == name))
}
pub fn install_remembered(game: &Path) -> io::Result<Vec<String>> {
    install(game, APK.get().map_or(&[], Vec::as_slice))
}
fn install(game: &Path, packs: &[Pack]) -> io::Result<Vec<String>> {
    bundled_shaders::install_to(
        game,
        packs,
        "resourcepacks",
        "android-bundled-resources.txt",
        "android-bundled-resource",
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn built_resource_pack_passes_android_import_validation() {
        let Some(path) = std::env::var_os("HORIZON_ANDROID_TEST_RESOURCE_PACK") else {
            return;
        };
        let path = std::path::PathBuf::from(path);
        let name = path.file_name().unwrap().to_str().unwrap();
        crate::content_files::validate(&path, crate::content_files::Kind::Packs, name, || false)
            .unwrap();
    }

    #[test]
    fn installed_once_preserves_deletions_and_player_files() {
        let dir =
            std::env::temp_dir().join(format!("horizon-resource-bundle-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let pack = |bytes: &[u8]| {
            vec![Pack {
                name: "Test.zip".into(),
                bytes: bytes.into(),
            }]
        };
        let path = dir.join("resourcepacks/Test.zip");
        assert_eq!(install(&dir, &pack(b"one")).unwrap(), ["Test.zip"]);
        assert!(install(&dir, &pack(b"one")).unwrap().is_empty());
        fs::remove_file(&path).unwrap();
        assert!(install(&dir, &pack(b"one")).unwrap().is_empty());
        assert_eq!(install(&dir, &pack(b"two")).unwrap(), ["Test.zip"]);
        fs::write(&path, b"mine").unwrap();
        assert!(install(&dir, &pack(b"three")).unwrap().is_empty());
        assert_eq!(fs::read(path).unwrap(), b"mine");
        fs::remove_dir_all(dir).unwrap();
    }
}
