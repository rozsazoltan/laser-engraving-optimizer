use crate::{calibration::GridCalibrationSpecification, processing::ProcessingSettings};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const SCHEMA_VERSION: u32 = 2;
const MIGRATED_LASER_NAME: &str = "Migrated laser";
const MAX_LASER_NAME_CHARS: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Laser {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Preset {
    pub id: String,
    pub laser_id: String,
    pub name: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub calibration: GridCalibrationSpecification,
    #[serde(default)]
    pub processing: ProcessingSettings,
    #[serde(default)]
    pub calibration_photo: Option<String>,
}

impl Preset {
    pub fn new(name: impl Into<String>, laser_id: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            laser_id: laser_id.into(),
            name: name.into(),
            note: String::new(),
            calibration: GridCalibrationSpecification::default(),
            processing: ProcessingSettings::default(),
            calibration_photo: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Store {
    schema: u32,
    #[serde(default)]
    lasers: Vec<Laser>,
    #[serde(default)]
    presets: Vec<Preset>,
    #[serde(default)]
    selected_laser: Option<String>,
    #[serde(default)]
    selected_preset: Option<String>,
}

/// Schema 1 layout, kept only to migrate an existing store once.
#[derive(Debug, Deserialize)]
struct LegacyStore {
    #[serde(default)]
    selected: Option<String>,
    #[serde(default)]
    selected_id: Option<String>,
    #[serde(default)]
    presets: Vec<LegacyPreset>,
}

#[derive(Debug, Deserialize)]
struct LegacyPreset {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    note: String,
    #[serde(default)]
    calibration: GridCalibrationSpecification,
    #[serde(default)]
    processing: ProcessingSettings,
    #[serde(default)]
    calibration_photo: Option<String>,
}

impl LegacyPreset {
    fn into_preset(self, laser_id: &str) -> Preset {
        Preset {
            id: self.id,
            laser_id: laser_id.to_string(),
            name: self.name,
            note: self.note,
            calibration: self.calibration,
            processing: self.processing,
            calibration_photo: self.calibration_photo,
        }
    }
}

#[derive(Debug)]
pub enum PresetError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Invalid(String),
    UnsupportedSchema(u32),
    MissingStore,
}

impl std::fmt::Display for PresetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "Preset storage error: {e}"),
            Self::Json(e) => write!(f, "Preset data is invalid: {e}"),
            Self::Invalid(e) => write!(f, "Invalid preset: {e}"),
            Self::UnsupportedSchema(v) => write!(f, "Unsupported preset schema version: {v}"),
            Self::MissingStore => write!(f, "Preset store does not exist"),
        }
    }
}
impl std::error::Error for PresetError {}
impl From<std::io::Error> for PresetError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<serde_json::Error> for PresetError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

pub type Result<T> = std::result::Result<T, PresetError>;

pub struct PresetStore {
    root: PathBuf,
    store: Store,
}

impl PresetStore {
    pub fn open_default() -> Result<Self> {
        let base = env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| PresetError::Invalid("LOCALAPPDATA is not set".into()))?;
        Self::open(base.join("LaserEngravingOptimizer"))
    }

    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let path = root.join("presets.json");
        if !path.exists() {
            return Ok(Self {
                root,
                store: Store {
                    schema: SCHEMA_VERSION,
                    lasers: Vec::new(),
                    presets: Vec::new(),
                    selected_laser: None,
                    selected_preset: None,
                },
            });
        }
        let value: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
        let version = value
            .get("schema")
            .or_else(|| value.get("schema_version"))
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| PresetError::Invalid("store has no schema version".into()))?;
        if version == 1 {
            let legacy: LegacyStore = serde_json::from_value(value)?;
            let store = migrate_legacy(legacy);
            validate_store(&store)?;
            let opened = Self { root, store };
            opened.save()?;
            return Ok(opened);
        }
        if version != SCHEMA_VERSION as u64 {
            return Err(PresetError::UnsupportedSchema(
                u32::try_from(version).unwrap_or(u32::MAX),
            ));
        }
        let store: Store = serde_json::from_value(value)?;
        validate_store(&store)?;
        Ok(Self { root, store })
    }

    pub fn lasers(&self) -> &[Laser] {
        &self.store.lasers
    }

    pub fn presets(&self) -> &[Preset] {
        &self.store.presets
    }

    pub fn selected_laser(&self) -> Option<&Laser> {
        self.store
            .selected_laser
            .as_deref()
            .and_then(|id| self.store.lasers.iter().find(|laser| laser.id == id))
    }

    pub fn selected(&self) -> Option<&Preset> {
        self.store
            .selected_preset
            .as_deref()
            .and_then(|id| self.store.presets.iter().find(|p| p.id == id))
    }

    pub fn select_laser(&mut self, id: &str) -> Result<()> {
        if !self.store.lasers.iter().any(|laser| laser.id == id) {
            return Err(PresetError::Invalid("laser does not exist".into()));
        }
        let old = self.store.clone();
        self.store.selected_laser = Some(id.to_owned());
        self.store.selected_preset = self
            .store
            .presets
            .iter()
            .find(|preset| preset.laser_id == id)
            .map(|preset| preset.id.clone());
        if let Err(error) = self.save() {
            self.store = old;
            return Err(error);
        }
        Ok(())
    }

    pub fn add_laser(&mut self, name: &str) -> Result<&Laser> {
        let name = self.checked_laser_name(name, None)?;
        let old = self.store.clone();
        let laser = Laser { id: new_id(), name };
        self.store.lasers.push(laser);
        self.store.selected_laser = Some(self.store.lasers.last().unwrap().id.clone());
        self.store.selected_preset = None;
        if let Err(error) = self.save() {
            self.store = old;
            return Err(error);
        }
        Ok(self.store.lasers.last().unwrap())
    }

    pub fn rename_laser(&mut self, id: &str, name: &str) -> Result<()> {
        if !self.store.lasers.iter().any(|laser| laser.id == id) {
            return Err(PresetError::Invalid("laser does not exist".into()));
        }
        let name = self.checked_laser_name(name, Some(id))?;
        let old = self.store.clone();
        self.store
            .lasers
            .iter_mut()
            .find(|laser| laser.id == id)
            .unwrap()
            .name = name;
        if let Err(error) = self.save() {
            self.store = old;
            return Err(error);
        }
        Ok(())
    }

    pub fn remove_laser(&mut self, id: &str) -> Result<()> {
        if !self.store.lasers.iter().any(|laser| laser.id == id) {
            return Err(PresetError::Invalid("laser does not exist".into()));
        }
        if self
            .store
            .presets
            .iter()
            .any(|preset| preset.laser_id == id)
        {
            return Err(PresetError::Invalid(
                "laser still owns presets; remove them first".into(),
            ));
        }
        let old = self.store.clone();
        self.store.lasers.retain(|laser| laser.id != id);
        if self.store.selected_laser.as_deref() == Some(id) {
            self.store.selected_laser = self.store.lasers.first().map(|laser| laser.id.clone());
            self.store.selected_preset = self
                .store
                .selected_laser
                .as_deref()
                .and_then(|lid| {
                    self.store
                        .presets
                        .iter()
                        .find(|preset| preset.laser_id == lid)
                })
                .map(|preset| preset.id.clone());
        }
        if let Err(error) = self.save() {
            self.store = old;
            return Err(error);
        }
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<()> {
        let preset = self
            .store
            .presets
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| PresetError::Invalid("selected preset does not exist".into()))?
            .clone();
        let old = self.store.clone();
        self.store.selected_laser = Some(preset.laser_id.clone());
        self.store.selected_preset = Some(preset.id);
        if let Err(error) = self.save() {
            self.store = old;
            return Err(error);
        }
        Ok(())
    }

    pub fn add(&mut self, preset: Preset) -> Result<()> {
        if self.store.presets.iter().any(|p| p.id == preset.id) {
            return Err(PresetError::Invalid("preset id already exists".into()));
        }
        if !self
            .store
            .lasers
            .iter()
            .any(|laser| laser.id == preset.laser_id)
        {
            return Err(PresetError::Invalid(
                "preset must belong to an existing laser".into(),
            ));
        }
        validate_preset(&preset)?;
        self.store.presets.push(preset);
        if let Err(error) = self.save() {
            self.store.presets.pop();
            return Err(error);
        }
        Ok(())
    }

    pub fn update(&mut self, preset: Preset) -> Result<()> {
        validate_preset(&preset)?;
        if !self
            .store
            .lasers
            .iter()
            .any(|laser| laser.id == preset.laser_id)
        {
            return Err(PresetError::Invalid(
                "preset must belong to an existing laser".into(),
            ));
        }
        let slot = self
            .store
            .presets
            .iter_mut()
            .find(|p| p.id == preset.id)
            .ok_or_else(|| PresetError::Invalid("preset does not exist".into()))?;
        let old = slot.clone();
        let old_id = old.id.clone();
        *slot = preset;
        if let Err(error) = self.save() {
            *self
                .store
                .presets
                .iter_mut()
                .find(|p| p.id == old_id)
                .unwrap() = old;
            return Err(error);
        }
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> Result<()> {
        let old_photo = self
            .store
            .presets
            .iter()
            .find(|p| p.id == id)
            .and_then(|p| p.calibration_photo.clone());
        let old = self.store.clone();
        self.store.presets.retain(|p| p.id != id);
        if self.store.presets.len() == old.presets.len() {
            return Err(PresetError::Invalid("preset does not exist".into()));
        }
        if self.store.selected_preset.as_deref() == Some(id) {
            self.store.selected_preset = None;
        }
        if let Err(e) = self.save() {
            self.store = old;
            return Err(e);
        }
        self.remove_unreferenced(old_photo);
        Ok(())
    }

    pub fn import_calibration_photo(&mut self, id: &str, source: impl AsRef<Path>) -> Result<()> {
        let preset = self
            .store
            .presets
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| PresetError::Invalid("preset does not exist".into()))?;
        let expected = preset
            .calibration
            .dimensions()
            .map_err(PresetError::Invalid)?;
        let image = crate::processing::load_rgb(source.as_ref()).map_err(PresetError::Invalid)?;
        if image.dimensions() != expected {
            return Err(PresetError::Invalid(format!(
                "calibration photo dimensions must be {}x{}",
                expected.0, expected.1
            )));
        }
        let old_photo = preset.calibration_photo.clone();
        let photos = self.root.join("photos");
        fs::create_dir_all(&photos)?;
        let photo_name = format!("{}.png", new_id());
        let staging = photos.join(format!(".{}.tmp.png", new_id()));
        image
            .save_with_format(&staging, image::ImageFormat::Png)
            .map_err(|e| {
                PresetError::Invalid(format!("calibration photo could not be saved: {e}"))
            })?;
        let target = photos.join(&photo_name);
        fs::rename(&staging, &target)?;
        self.store
            .presets
            .iter_mut()
            .find(|p| p.id == id)
            .unwrap()
            .calibration_photo = Some(photo_name);
        if let Err(e) = self.save() {
            self.store
                .presets
                .iter_mut()
                .find(|p| p.id == id)
                .unwrap()
                .calibration_photo = old_photo;
            let _ = fs::remove_file(target);
            return Err(e);
        }
        self.remove_unreferenced(old_photo);
        Ok(())
    }

    pub fn calibration_photo_path(&self, preset: &Preset) -> Option<PathBuf> {
        let name = preset.calibration_photo.as_deref()?;
        safe_photo_name(name).then(|| self.root.join("photos").join(name))
    }

    fn checked_laser_name(&self, name: &str, skip: Option<&str>) -> Result<String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > MAX_LASER_NAME_CHARS {
            return Err(PresetError::Invalid(format!(
                "laser name must contain 1-{MAX_LASER_NAME_CHARS} characters"
            )));
        }
        if name.chars().any(char::is_control) {
            return Err(PresetError::Invalid(
                "laser name must not contain control characters".into(),
            ));
        }
        let lower = name.to_lowercase();
        if self
            .store
            .lasers
            .iter()
            .any(|laser| Some(laser.id.as_str()) != skip && laser.name.to_lowercase() == lower)
        {
            return Err(PresetError::Invalid(
                "laser name must be unique (case-insensitive)".into(),
            ));
        }
        Ok(name.to_string())
    }

    fn save(&self) -> Result<()> {
        validate_store(&self.store)?;
        fs::create_dir_all(&self.root)?;
        let bytes = serde_json::to_vec_pretty(&self.store)?;
        let temp = self.root.join(format!(".presets-{}.tmp", new_id()));
        fs::write(&temp, bytes)?;
        if let Err(error) = replace_file(&temp, &self.root.join("presets.json")) {
            let _ = fs::remove_file(temp);
            return Err(error.into());
        }
        Ok(())
    }

    fn remove_unreferenced(&self, name: Option<String>) {
        if let Some(name) = name.filter(|n| {
            safe_photo_name(n)
                && !self
                    .store
                    .presets
                    .iter()
                    .any(|p| p.calibration_photo.as_deref() == Some(n))
        }) {
            let _ = fs::remove_file(self.root.join("photos").join(name));
        }
    }
}

fn migrate_legacy(legacy: LegacyStore) -> Store {
    let mut store = Store {
        schema: SCHEMA_VERSION,
        lasers: Vec::new(),
        presets: Vec::new(),
        selected_laser: None,
        selected_preset: None,
    };
    let selected = legacy.selected.or(legacy.selected_id);
    if legacy.presets.is_empty() {
        return store;
    }
    let laser = Laser {
        id: new_id(),
        name: MIGRATED_LASER_NAME.to_string(),
    };
    let laser_id = laser.id.clone();
    store.lasers.push(laser);
    store.presets = legacy
        .presets
        .into_iter()
        .map(|preset| preset.into_preset(&laser_id))
        .collect();
    store.selected_laser = Some(laser_id);
    store.selected_preset =
        selected.filter(|id| store.presets.iter().any(|preset| &preset.id == id));
    store
}

fn replace_file(temp: &Path, target: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
        }
        let from: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        let ok = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0x1 | 0x8) };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        fs::rename(temp, target)
    }
}

fn safe_photo_name(name: &str) -> bool {
    !name.is_empty()
        && name
            == Path::new(name)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
        && name.ends_with(".png")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
}

fn validate_id(id: &str, what: &str) -> Result<()> {
    if id.is_empty() || id.len() > 64 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(PresetError::Invalid(format!("unsafe {what} id")));
    }
    Ok(())
}

fn validate_preset(p: &Preset) -> Result<()> {
    validate_id(&p.id, "preset")?;
    validate_id(&p.laser_id, "laser")?;
    if p.name.trim().is_empty() || p.name.len() > 256 {
        return Err(PresetError::Invalid("name must contain 1-256 bytes".into()));
    }
    p.calibration.validate().map_err(PresetError::Invalid)?;
    p.processing.validate().map_err(PresetError::Invalid)?;
    if p.calibration_photo
        .as_deref()
        .is_some_and(|n| !safe_photo_name(n))
    {
        return Err(PresetError::Invalid(
            "unsafe calibration photo reference".into(),
        ));
    }
    Ok(())
}

fn validate_laser(laser: &Laser) -> Result<()> {
    validate_id(&laser.id, "laser")?;
    let name = laser.name.trim();
    if name.is_empty() || name.chars().count() > MAX_LASER_NAME_CHARS {
        return Err(PresetError::Invalid(format!(
            "laser name must contain 1-{MAX_LASER_NAME_CHARS} characters"
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(PresetError::Invalid(
            "laser name must not contain control characters".into(),
        ));
    }
    if name != laser.name {
        return Err(PresetError::Invalid(
            "laser name must not have surrounding whitespace".into(),
        ));
    }
    Ok(())
}

fn validate_store(store: &Store) -> Result<()> {
    if store.schema != SCHEMA_VERSION {
        return Err(PresetError::UnsupportedSchema(store.schema));
    }
    let mut laser_ids = HashSet::new();
    let mut laser_names = HashSet::new();
    for laser in &store.lasers {
        validate_laser(laser)?;
        if !laser_ids.insert(laser.id.as_str()) {
            return Err(PresetError::Invalid("duplicate laser id".into()));
        }
        if !laser_names.insert(laser.name.to_lowercase()) {
            return Err(PresetError::Invalid("duplicate laser name".into()));
        }
    }
    let mut preset_ids = HashSet::new();
    for preset in &store.presets {
        validate_preset(preset)?;
        if !preset_ids.insert(preset.id.as_str()) {
            return Err(PresetError::Invalid("duplicate preset id".into()));
        }
        if !laser_ids.contains(preset.laser_id.as_str()) {
            return Err(PresetError::Invalid(
                "preset references an unknown laser".into(),
            ));
        }
    }
    if let Some(id) = store.selected_laser.as_deref() {
        if !laser_ids.contains(id) {
            return Err(PresetError::Invalid("selected laser does not exist".into()));
        }
    }
    if let Some(id) = store.selected_preset.as_deref() {
        let preset = store
            .presets
            .iter()
            .find(|preset| preset.id == id)
            .ok_or_else(|| PresetError::Invalid("selected preset does not exist".into()))?;
        if store.selected_laser.as_deref() != Some(preset.laser_id.as_str()) {
            return Err(PresetError::Invalid(
                "selected preset does not belong to the selected laser".into(),
            ));
        }
    }
    Ok(())
}

fn new_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}-{:x}", nanos, std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let path = env::temp_dir().join(format!("preset-test-{}", new_id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn force_save_failure(root: &Path) -> (PathBuf, PathBuf) {
        let store = root.join("presets.json");
        let backup = root.join("presets.json.backup");
        fs::rename(&store, &backup).unwrap();
        fs::create_dir(&store).unwrap();
        (store, backup)
    }

    fn restore_save(root: &Path, store: PathBuf, backup: PathBuf) {
        fs::remove_dir(store).unwrap();
        fs::rename(backup, root.join("presets.json")).unwrap();
    }

    fn valid_preset(name: &str, laser_id: &str) -> Preset {
        let mut preset = Preset::new(name, laser_id);
        preset.calibration.block_size = 32;
        preset.calibration.rows = 2;
        preset.calibration.columns = 2;
        preset
    }

    #[test]
    fn round_trip_and_managed_photo_lifecycle() {
        let root = temp();
        let source = root.join("source.png");
        image::RgbImage::from_pixel(64, 64, image::Rgb([4, 5, 6]))
            .save(&source)
            .unwrap();
        let mut db = PresetStore::open(&root).unwrap();
        let laser_id = db.add_laser("Engraver A").unwrap().id.clone();
        let mut first = valid_preset("Default", &laser_id);
        first.note = "engraver A".into();
        let id = first.id.clone();
        db.add(first.clone()).unwrap();
        db.import_calibration_photo(&id, &source).unwrap();
        let managed = db
            .calibration_photo_path(db.presets().first().unwrap())
            .unwrap();
        fs::remove_file(source).unwrap();
        assert!(image::open(&managed).is_ok());
        first = db.presets()[0].clone();
        first.name = "Updated".into();
        first.note = "400 6 6".into();
        first.processing.gamma = 1.4;
        db.update(first.clone()).unwrap();
        let second = valid_preset("Other", &laser_id);
        let second_id = second.id.clone();
        db.add(second).unwrap();
        db.select(&second_id).unwrap();
        let reopened = PresetStore::open(&root).unwrap();
        assert_eq!(reopened.selected().unwrap().id, second_id);
        assert_eq!(reopened.selected_laser().unwrap().id, laser_id);
        assert_eq!(reopened.presets()[0], first);
        assert!(managed.exists());
        assert_eq!(
            reopened
                .calibration_photo_path(&reopened.presets()[0])
                .unwrap(),
            managed
        );
        let replacement = root.join("replacement.png");
        image::RgbImage::from_pixel(64, 64, image::Rgb([7, 8, 9]))
            .save(&replacement)
            .unwrap();
        let old_photo = managed.clone();
        let id = reopened.presets()[0].id.clone();
        let mut reopened = reopened;
        reopened
            .import_calibration_photo(&id, &replacement)
            .unwrap();
        assert!(!old_photo.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_bad_store_without_rewriting_it() {
        let root = temp();
        let path = root.join("presets.json");
        let malformed = br#"{"schema_version":99,"selected_id":null,"presets":[]}"#;
        fs::write(&path, malformed).unwrap();
        assert!(matches!(
            PresetStore::open(&root),
            Err(PresetError::UnsupportedSchema(99))
        ));
        assert_eq!(fs::read(&path).unwrap(), malformed);
        let malformed = br#"{"schema":3,"lasers":[],"presets":[]}"#;
        fs::write(&path, malformed).unwrap();
        assert!(matches!(
            PresetStore::open(&root),
            Err(PresetError::UnsupportedSchema(3))
        ));
        assert_eq!(fs::read(&path).unwrap(), malformed);
        let malformed = b"{ definitely not json";
        fs::write(&path, malformed).unwrap();
        assert!(PresetStore::open(&root).is_err());
        assert_eq!(fs::read(&path).unwrap(), malformed);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migrates_schema_one_once() {
        let root = temp();
        let path = root.join("presets.json");
        let legacy = serde_json::json!({
            "schema_version": 1,
            "selected_id": "abc-1",
            "presets": [
                {
                    "id": "abc-1",
                    "name": "Old one",
                    "note": "n",
                    "calibration": { "block_size": 400, "rows": 6, "columns": 6, "exponent": 1.8 },
                    "processing": { "gamma": 1.0, "clahe_clip": 4.0, "color_method": "decolorize" },
                    "calibration_photo": null
                },
                {
                    "id": "abc-2",
                    "name": "Old two",
                    "calibration": { "block_size": 400, "rows": 6, "columns": 6, "exponent": 1.8 }
                }
            ]
        });
        fs::write(&path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();
        let db = PresetStore::open(&root).unwrap();
        assert_eq!(db.lasers().len(), 1);
        assert_eq!(db.lasers()[0].name, "Migrated laser");
        let laser_id = db.lasers()[0].id.clone();
        assert_eq!(db.presets().len(), 2);
        assert!(db
            .presets()
            .iter()
            .all(|preset| preset.laser_id == laser_id));
        assert_eq!(db.selected_laser().unwrap().id, laser_id);
        assert_eq!(db.selected().unwrap().id, "abc-1");

        let written: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(written["schema"], 2);
        assert_eq!(written["lasers"][0]["name"], "Migrated laser");
        assert_eq!(written["selected_laser"], laser_id);
        assert_eq!(written["selected_preset"], "abc-1");

        let reopened = PresetStore::open(&root).unwrap();
        assert_eq!(reopened.lasers().len(), 1);
        assert_eq!(reopened.presets().len(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn laser_validation_and_ownership_rules() {
        let root = temp();
        let mut db = PresetStore::open(&root).unwrap();
        assert!(db.add_laser("   ").is_err());
        assert!(db.add_laser("bad\nname").is_err());
        assert!(db.add_laser(&"x".repeat(MAX_LASER_NAME_CHARS + 1)).is_err());
        let id = db.add_laser("  Diode  ").unwrap().id.clone();
        assert_eq!(db.lasers()[0].name, "Diode");
        assert_eq!(db.selected_laser().unwrap().id, id);
        assert!(db.add_laser("diode").is_err());

        db.rename_laser(&id, "Fiber").unwrap();
        assert_eq!(db.lasers()[0].name, "Fiber");
        assert_eq!(db.selected_laser().unwrap().id, id);

        let other = db.add_laser("Co2").unwrap().id.clone();
        assert!(db.rename_laser(&id, "co2").is_err());
        assert_eq!(db.lasers()[0].name, "Fiber");
        assert_eq!(db.selected_laser().unwrap().id, other);

        let orphan = valid_preset("Orphan", "missing-laser");
        assert!(db.add(orphan).is_err());

        let preset = valid_preset("Attached", &id);
        let preset_id = preset.id.clone();
        db.add(preset).unwrap();
        assert!(db.remove_laser(&id).is_err());

        db.rename_laser(&id, "Fiber one").unwrap();
        db.remove(&preset_id).unwrap();
        db.remove_laser(&id).unwrap();
        assert_eq!(db.lasers().len(), 1);
        assert!(db.selected_laser().is_some());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validation_rejects_unsafe_photo_and_duplicate_ids() {
        let root = temp();
        let mut db = PresetStore::open(&root).unwrap();
        let laser_id = db.add_laser("Laser").unwrap().id.clone();
        let mut p = valid_preset("x", &laser_id);
        p.calibration_photo = Some("../outside.png".into());
        assert!(db.add(p).is_err());
        let p = valid_preset("x", &laser_id);
        db.add(p.clone()).unwrap();
        assert!(db.add(p).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn remove_deletes_owned_photo_after_store_commit() {
        let root = temp();
        let source = root.join("source.png");
        image::RgbImage::new(64, 64).save(&source).unwrap();
        let mut db = PresetStore::open(&root).unwrap();
        let laser_id = db.add_laser("Laser").unwrap().id.clone();
        let p = valid_preset("x", &laser_id);
        let id = p.id.clone();
        db.add(p).unwrap();
        db.import_calibration_photo(&id, &source).unwrap();
        let photo = db.calibration_photo_path(&db.presets()[0]).unwrap();
        db.remove(&id).unwrap();
        assert!(!photo.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_mutations_restore_memory_and_keep_json() {
        let root = temp();
        let mut db = PresetStore::open(&root).unwrap();
        let laser_id = db.add_laser("Laser").unwrap().id.clone();
        let original = valid_preset("original", &laser_id);
        let id = original.id.clone();
        db.add(original).unwrap();
        db.select(&id).unwrap();
        let original_photo = root.join("original.png");
        image::RgbImage::from_pixel(64, 64, image::Rgb([1, 2, 3]))
            .save(&original_photo)
            .unwrap();
        db.import_calibration_photo(&id, &original_photo).unwrap();
        let existing_photo = db.presets()[0].calibration_photo.clone();
        let expected = db.presets().to_vec();

        let (store, backup) = force_save_failure(&root);
        let mut added = valid_preset("added", &laser_id);
        assert!(db.add(added.clone()).is_err());
        assert_eq!(db.presets(), expected);
        restore_save(&root, store, backup);

        let (store, backup) = force_save_failure(&root);
        added.id = id.clone();
        added.name = "changed".into();
        assert!(db.update(added).is_err());
        assert_eq!(db.presets(), expected);
        restore_save(&root, store, backup);

        let other = valid_preset("other", &laser_id);
        let other_id = other.id.clone();
        db.add(other).unwrap();
        let before_select = db.store.clone();
        let (store, backup) = force_save_failure(&root);
        assert!(db.select(&other_id).is_err());
        assert_eq!(db.store, before_select);
        restore_save(&root, store, backup);

        let before_remove = db.store.clone();
        let (store, backup) = force_save_failure(&root);
        assert!(db.remove(&id).is_err());
        assert_eq!(db.store, before_remove);
        restore_save(&root, store, backup);

        let source = root.join("source.png");
        image::RgbImage::from_pixel(64, 64, image::Rgb([0, 0, 0]))
            .save(&source)
            .unwrap();
        let old_reference = db.presets()[0].calibration_photo.clone();
        let before_import = db.store.clone();
        let old_photo_path = db
            .calibration_photo_path(&db.presets()[0])
            .expect("existing photo reference");
        let old_photo_bytes = fs::read(&old_photo_path).unwrap();
        let json_bytes = fs::read(root.join("presets.json")).unwrap();
        let (store, backup) = force_save_failure(&root);
        assert!(db.import_calibration_photo(&id, &source).is_err());
        assert_eq!(db.store, before_import);
        assert_eq!(db.presets()[0].calibration_photo, old_reference);
        restore_save(&root, store, backup);
        assert_eq!(fs::read(root.join("presets.json")).unwrap(), json_bytes);
        assert_eq!(fs::read(&old_photo_path).unwrap(), old_photo_bytes);
        let photo_dir = root.join("photos");
        assert_eq!(fs::read_dir(photo_dir).unwrap().count(), 1);
        assert_eq!(db.presets()[0].calibration_photo, existing_photo);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_laser_mutations_roll_back() {
        let root = temp();
        let mut db = PresetStore::open(&root).unwrap();
        db.add_laser("First").unwrap();
        let before = db.store.clone();
        let (store, backup) = force_save_failure(&root);
        assert!(db.add_laser("Second").is_err());
        assert_eq!(db.store, before);
        restore_save(&root, store, backup);
        let id = db.lasers()[0].id.clone();
        let (store, backup) = force_save_failure(&root);
        assert!(db.rename_laser(&id, "Renamed").is_err());
        assert_eq!(db.store, before);
        restore_save(&root, store, backup);
        fs::remove_dir_all(root).unwrap();
    }
}
