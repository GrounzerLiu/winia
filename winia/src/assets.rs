//! Runtime asset bundle and window icon loading.
//!
//! The bundle is initialized once at application startup: a desktop directory
//! (development root or executable directory) or Android's AssetManager.

use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct AssetEntry {
    pub id: String,
    pub path: String,
    pub packaged: String,
    pub bytes: u64,
    pub sha256: String,
    pub density: Option<f32>,
}

pub struct AssetManifest {
    pub entries: Vec<AssetEntry>,
    pub window_icon: Option<String>,
}

pub trait AssetBundle: Send + Sync {
    fn load(&self, logical: &str, density: f32) -> Option<Vec<u8>>;
    fn manifest(&self) -> Option<&AssetManifest>;
}

static BUNDLE: OnceLock<Box<dyn AssetBundle>> = OnceLock::new();
static ASSET_EPOCH: AtomicU64 = AtomicU64::new(0);
#[cfg(not(target_os = "android"))]
static WINDOW_ICON: OnceLock<Option<winit::icon::Icon>> = OnceLock::new();

#[cfg(not(target_os = "android"))]
pub fn init_desktop_default() {
    let root = std::env::var_os("WINIA_ASSET_ROOT")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| desktop_root_for_exe(&exe))
        });
    let Some(root) = root else {
        return;
    };
    let manifest = std::fs::read_to_string(root.join("assets.manifest.json"))
        .ok()
        .and_then(|text| parse_manifest(&text));
    let bundle = DesktopBundle { root, manifest };
    if BUNDLE.set(Box::new(bundle)).is_ok() {
        ASSET_EPOCH.fetch_add(1, Ordering::Release);
    }
}

#[cfg(target_os = "android")]
pub fn init_android(manager: ndk::asset::AssetManager) {
    let manifest = read_android_manifest(&manager);
    let bundle = AndroidBundle { manager, manifest };
    if BUNDLE.set(Box::new(bundle)).is_ok() {
        ASSET_EPOCH.fetch_add(1, Ordering::Release);
    }
}

pub fn load(logical: &str) -> Option<Vec<u8>> {
    load_at(logical, 1.0)
}

pub fn load_at(logical: &str, density: f32) -> Option<Vec<u8>> {
    BUNDLE.get()?.load(logical, density)
}

/// Resolve a user-supplied path to the manifest's canonical logical path.
pub fn canonical_path(logical: &str) -> String {
    let normalized = normalize(logical);
    if let Some(manifest) = manifest() {
        if let Some(entry) = manifest.find(&normalized) {
            // Preserve an explicit variant path so it is not re-resolved by window density.
            return if entry.density.is_some() {
                entry.path.clone()
            } else {
                entry.id.clone()
            };
        }
        if let Some(entry) = manifest.resolve(&normalized, 1.0) {
            return entry.id.clone();
        }
    }
    normalized
}

#[cfg(not(target_os = "android"))]
fn desktop_root_for_exe(exe: &Path) -> Option<PathBuf> {
    let parent = exe.parent()?;
    if let Some(resources) = macos_resources_root(exe) {
        if resources.join("assets.manifest.json").is_file() {
            return Some(resources);
        }
    }
    Some(parent.to_path_buf())
}

#[cfg(not(target_os = "android"))]
fn macos_resources_root(exe: &Path) -> Option<PathBuf> {
    let parent = exe.parent()?;
    if parent.file_name() == Some(OsStr::new("MacOS")) {
        Some(parent.parent()?.join("Resources"))
    } else {
        None
    }
}

pub fn manifest() -> Option<&'static AssetManifest> {
    BUNDLE.get()?.manifest()
}

pub fn generation() -> u64 {
    ASSET_EPOCH.load(Ordering::Acquire)
}

#[cfg(not(target_os = "android"))]
pub fn window_icon() -> Option<winit::icon::Icon> {
    WINDOW_ICON.get_or_init(load_window_icon).clone()
}

#[cfg(not(target_os = "android"))]
fn load_window_icon() -> Option<winit::icon::Icon> {
    let manifest = manifest()?;
    let logical = manifest.window_icon.as_deref()?;
    let bytes = load(logical)?;
    let image = skia_safe::Image::from_encoded(skia_safe::Data::new_copy(&bytes))?;
    let dimensions = image.dimensions();
    let width = dimensions.width;
    let height = dimensions.height;
    let info = skia_safe::ImageInfo::new(
        (width, height),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Unpremul,
        None,
    );
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    if !image.read_pixels(
        &info,
        &mut pixels,
        width as usize * 4,
        (0, 0),
        skia_safe::image::CachingHint::Allow,
    ) {
        return None;
    }
    winit::icon::RgbaIcon::new(pixels, width as u32, height as u32)
        .ok()
        .map(winit::icon::Icon::from)
}

#[cfg(target_os = "android")]
pub fn window_icon() -> Option<winit::icon::Icon> {
    None
}

struct DesktopBundle {
    root: PathBuf,
    manifest: Option<AssetManifest>,
}

impl AssetBundle for DesktopBundle {
    fn load(&self, logical: &str, density: f32) -> Option<Vec<u8>> {
        let entry = self.manifest.as_ref()?.resolve(logical, density)?;
        let bytes = std::fs::read(self.root.join(&entry.packaged)).ok()?;
        entry.verify(&bytes).then_some(bytes)
    }

    fn manifest(&self) -> Option<&AssetManifest> {
        self.manifest.as_ref()
    }
}

#[cfg(target_os = "android")]
struct AndroidBundle {
    manager: ndk::asset::AssetManager,
    manifest: Option<AssetManifest>,
}

#[cfg(target_os = "android")]
impl AssetBundle for AndroidBundle {
    fn load(&self, logical: &str, density: f32) -> Option<Vec<u8>> {
        let entry = self.manifest.as_ref()?.resolve(logical, density)?;
        let name = std::ffi::CString::new(entry.packaged.clone()).ok()?;
        let mut asset = self.manager.open(&name)?;
        let mut bytes = Vec::new();
        use std::io::Read;
        asset.read_to_end(&mut bytes).ok()?;
        entry.verify(&bytes).then_some(bytes)
    }

    fn manifest(&self) -> Option<&AssetManifest> {
        self.manifest.as_ref()
    }
}

impl AssetManifest {
    fn find(&self, logical: &str) -> Option<&AssetEntry> {
        let logical = normalize(logical);
        self.entries
            .iter()
            .find(|entry| entry.path == logical || entry.packaged == logical)
    }

    fn resolve(&self, logical: &str, density: f32) -> Option<&AssetEntry> {
        let logical = normalize(logical);
        if let Some(exact) = self.find(&logical) {
            // A base entry (id == path, no density) is a candidate group, not an exact
            // density choice; only an explicit variant path short-circuits the resolver.
            if exact.density.is_some() || exact.path != exact.id {
                return Some(exact);
            }
        }
        let prefixed = format!("assets/{logical}");
        let mut candidates: Vec<&AssetEntry> = self
            .entries
            .iter()
            .filter(|entry| {
                entry.id == logical
                    || entry.id == prefixed
                    || entry.path == prefixed
                    || entry.packaged == prefixed
            })
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let target = if density.is_finite() && density > 0.0 {
            density
        } else {
            1.0
        };
        let value = |entry: &&AssetEntry| entry.density.unwrap_or(1.0);
        if let Some(exact) = candidates
            .iter()
            .find(|entry| (value(entry) - target).abs() < 0.01)
        {
            return Some(*exact);
        }
        let above = candidates
            .iter()
            .filter(|entry| value(entry) >= target)
            .min_by(|a, b| value(a).total_cmp(&value(b)));
        if let Some(entry) = above {
            return Some(*entry);
        }
        candidates.sort_by(|a, b| value(a).total_cmp(&value(b)));
        candidates.pop()
    }
}

pub fn resolved_density(logical: &str, density: f32) -> Option<f32> {
    manifest()?
        .resolve(logical, density)
        .map(|entry| entry.density.unwrap_or(1.0))
}

impl AssetEntry {
    fn verify(&self, bytes: &[u8]) -> bool {
        if self.bytes != 0 && self.bytes != bytes.len() as u64 {
            return false;
        }
        if !self.sha256.is_empty() {
            let digest = format!("{:x}", Sha256::digest(bytes));
            if digest != self.sha256.to_ascii_lowercase() {
                return false;
            }
        }
        true
    }
}

fn normalize(value: &str) -> String {
    value
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_owned()
}

fn parse_manifest(text: &str) -> Option<AssetManifest> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let entries = value
        .get("files")?
        .as_array()?
        .iter()
        .filter_map(|file| {
            let path = normalize(file.get("path")?.as_str()?);
            Some(AssetEntry {
                id: normalize(
                    file.get("id")
                        .and_then(|value| value.as_str())
                        .unwrap_or(&path),
                ),
                path,
                packaged: normalize(file.get("packaged")?.as_str()?),
                bytes: file
                    .get("bytes")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0),
                sha256: file
                    .get("sha256")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_owned(),
                density: file
                    .get("density")
                    .and_then(|value| value.as_f64())
                    .map(|value| value as f32),
            })
        })
        .collect();
    Some(AssetManifest {
        entries,
        window_icon: value
            .get("window_icon")
            .and_then(|value| value.as_str())
            .map(str::to_owned),
    })
}

#[cfg(target_os = "android")]
fn read_android_manifest(manager: &ndk::asset::AssetManager) -> Option<AssetManifest> {
    let name = std::ffi::CString::new("winia-assets/manifest.json").ok()?;
    let mut asset = manager.open(&name)?;
    let mut bytes = Vec::new();
    use std::io::Read;
    asset.read_to_end(&mut bytes).ok()?;
    parse_manifest(std::str::from_utf8(&bytes).ok()?)
}

#[cfg(all(test, not(target_os = "android")))]
mod tests {
    use super::*;

    fn entry(id: &str, density: Option<f32>) -> AssetEntry {
        let path = match density {
            Some(value) => format!("{id}@{value}x.png"),
            None => id.to_owned(),
        };
        AssetEntry {
            id: id.to_owned(),
            path: path.clone(),
            packaged: path,
            bytes: 0,
            sha256: String::new(),
            density,
        }
    }

    #[test]
    fn resolves_asset_density_variants() {
        let manifest = AssetManifest {
            entries: vec![
                entry("assets/icons/logo.png", None),
                entry("assets/icons/logo.png", Some(2.0)),
                entry("assets/icons/logo.png", Some(3.0)),
            ],
            window_icon: None,
        };
        assert_eq!(
            manifest
                .resolve("assets/icons/logo.png", 1.0)
                .unwrap()
                .density,
            None
        );
        assert_eq!(
            manifest
                .resolve("assets/icons/logo.png", 2.0)
                .unwrap()
                .density,
            Some(2.0)
        );
        assert_eq!(
            manifest
                .resolve("assets/icons/logo.png", 4.0)
                .unwrap()
                .density,
            Some(3.0)
        );
        assert_eq!(
            manifest.resolve("icons/logo.png", 2.0).unwrap().density,
            Some(2.0)
        );
    }

    #[test]
    fn parses_density_manifest_entries() {
        let manifest = parse_manifest(
            r#"{"files":[{"id":"assets/logo.png","path":"assets/logo@2x.png","packaged":"assets/logo@2x.png","bytes":1,"sha256":"00","density":2.0}]}"#,
        )
        .unwrap();
        assert_eq!(manifest.entries[0].id, "assets/logo.png");
        assert_eq!(manifest.entries[0].density, Some(2.0));
    }

    #[test]
    fn verifies_asset_bytes_and_hash() {
        let bytes = b"asset";
        let digest = format!("{:x}", Sha256::digest(bytes));
        let entry = AssetEntry {
            id: "assets/a.txt".to_owned(),
            path: "assets/a.txt".to_owned(),
            packaged: "assets/a.txt".to_owned(),
            bytes: bytes.len() as u64,
            sha256: digest,
            density: None,
        };
        assert!(entry.verify(bytes));
        assert!(!entry.verify(b"other"));
    }

    #[test]
    fn maps_macos_executable_to_resources() {
        let root = Path::new("Demo.app/Contents");
        let exe = root.join("MacOS/demo");
        assert_eq!(
            macos_resources_root(&exe).as_deref(),
            Some(root.join("Resources").as_path())
        );
        assert_eq!(macos_resources_root(Path::new("other/demo")), None);
    }
}
