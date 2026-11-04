//! Runtime asset bundle and window icon loading.
//!
//! The bundle is initialized once at application startup: a desktop directory
//! (development root or executable directory) or Android's AssetManager.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub struct AssetEntry {
    pub path: String,
    pub packaged: String,
}

pub struct AssetManifest {
    pub entries: Vec<AssetEntry>,
    pub window_icon: Option<String>,
}

pub trait AssetBundle: Send + Sync {
    fn load(&self, logical: &str) -> Option<Vec<u8>>;
    fn manifest(&self) -> Option<&AssetManifest>;
}

static BUNDLE: OnceLock<Box<dyn AssetBundle>> = OnceLock::new();

pub fn init_desktop_default() {
    let root = std::env::var_os("WINIA_ASSET_ROOT")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok()?.parent().map(Path::to_path_buf));
    let Some(root) = root else {
        return;
    };
    let manifest = std::fs::read_to_string(root.join("assets.manifest.json"))
        .ok()
        .and_then(|text| parse_manifest(&text));
    let bundle = DesktopBundle { root, manifest };
    let _ = BUNDLE.set(Box::new(bundle));
}

#[cfg(target_os = "android")]
pub fn init_android(manager: ndk::asset::AssetManager) {
    let manifest = read_android_manifest(&manager);
    let bundle = AndroidBundle { manager, manifest };
    let _ = BUNDLE.set(Box::new(bundle));
}

pub fn load(logical: &str) -> Option<Vec<u8>> {
    BUNDLE.get()?.load(logical)
}

pub fn manifest() -> Option<&'static AssetManifest> {
    BUNDLE.get()?.manifest()
}

#[cfg(not(target_os = "android"))]
pub fn window_icon() -> Option<winit::icon::Icon> {
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
    fn load(&self, logical: &str) -> Option<Vec<u8>> {
        let entry = self.manifest.as_ref()?.find(logical)?;
        std::fs::read(self.root.join(&entry.packaged)).ok()
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
    fn load(&self, logical: &str) -> Option<Vec<u8>> {
        let entry = self.manifest.as_ref()?.find(logical)?;
        let name = std::ffi::CString::new(entry.packaged.clone()).ok()?;
        let mut asset = self.manager.open(&name)?;
        let mut bytes = Vec::new();
        use std::io::Read;
        asset.read_to_end(&mut bytes).ok()?;
        Some(bytes)
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
            Some(AssetEntry {
                path: file.get("path")?.as_str()?.to_owned(),
                packaged: file.get("packaged")?.as_str()?.to_owned(),
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
