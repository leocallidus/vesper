use serde::Deserialize;
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use directories::ProjectDirs;
use crate::config::Config;

#[derive(Clone, Debug)]
pub struct Shaderpack {
    pub id: String,
    pub name: String,
    pub description: String,
    pub dir: PathBuf,
    pub logo_path: PathBuf,
    pub shaders: Vec<ShaderEntry>,
}

#[derive(Clone, Debug)]
pub struct ShaderEntry {
    pub id: String,
    pub name: String,
    #[allow(dead_code)]
    pub description: Option<String>,
    pub dir: PathBuf,
    pub detected: DetectedShadertoyFiles,
    pub preview_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Default)]
pub struct DetectedShadertoyFiles {
    pub common: Option<PathBuf>,
    pub image: Option<PathBuf>,
    pub buffers: [Option<PathBuf>; 4],
    pub sound: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct ShaderpackManifest {
    #[serde(default)]
    schema_version: Option<u32>,
    pack: PackSection,
    #[serde(default)]
    shader: Vec<ShaderSection>,
}

#[derive(Debug, Deserialize)]
struct PackSection {
    #[serde(default)]
    id: Option<String>,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    author: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    homepage: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    license: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ShaderSection {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    source_url: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    license: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    author: Option<String>,
}

pub fn load_shaderpack(pack_dir: &Path) -> Result<Shaderpack, String> {
    if !pack_dir.is_dir() {
        return Err(format!("shaderpack path is not a directory: {}", pack_dir.display()));
    }

    let pack_dir_canon = pack_dir
        .canonicalize()
        .unwrap_or_else(|_| pack_dir.to_path_buf());

    let manifest_path = pack_dir.join("shaderpack.toml");
    if !manifest_path.is_file() {
        return Err(format!(
            "shaderpack.toml not found in {}",
            pack_dir.display()
        ));
    }

    let logo_path = pack_dir.join("shaderpacklogo.png");
    if !logo_path.is_file() {
        return Err(format!(
            "shaderpacklogo.png not found in {}",
            pack_dir.display()
        ));
    }

    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("read {}: {e}", manifest_path.display()))?;
    let manifest: ShaderpackManifest = toml::from_str(&manifest_text)
        .map_err(|e| format!("parse {}: {e}", manifest_path.display()))?;

    if let Some(ver) = manifest.schema_version {
        if ver != 1 {
            return Err(format!(
                "unsupported shaderpack schema_version={ver} (expected 1)"
            ));
        }
    }

    if manifest.pack.name.trim().is_empty() {
        return Err("pack.name is required".to_string());
    }

    let pack_id = manifest
        .pack
        .id
        .as_deref()
        .and_then(sanitize_id)
        .unwrap_or_else(|| generate_id_from_text("pack", &manifest.pack.name));

    let shaders_dir = pack_dir.join("shaders");
    let shader_entries: Vec<ShaderEntry> = if !manifest.shader.is_empty() {
        let mut entries = load_manifest_shaders(pack_dir, &pack_id, &manifest.shader)?;

        // Even if the manifest lists some shaders explicitly, auto-discover any other valid shader
        // directories inside `shaders/` so users don't have to keep the manifest in sync.
        let mut used_ids: HashSet<String> = entries.iter().map(|s| s.id.clone()).collect();
        let existing_dirs: HashSet<PathBuf> = entries
            .iter()
            .map(|s| s.dir.canonicalize().unwrap_or_else(|_| s.dir.clone()))
            .collect();

        let extras = scan_shader_dirs_extra(
            &pack_dir_canon,
            &pack_id,
            &shaders_dir,
            &mut used_ids,
            &existing_dirs,
        )?;
        entries.extend(extras);
        entries
    } else {
        scan_shader_dirs(pack_dir, &pack_id, &shaders_dir)?
    };

    if shader_entries.is_empty() {
        return Err("shaderpack contains no shaders".to_string());
    }

    Ok(Shaderpack {
        id: pack_id,
        name: manifest.pack.name,
        description: manifest.pack.description.unwrap_or_default(),
        dir: pack_dir.to_path_buf(),
        logo_path,
        shaders: shader_entries,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportConflictPolicy {
    Abort,
    Replace,
    Rename,
}

#[derive(Debug)]
pub enum ImportShaderpackError {
    SourceInvalid(String),
    Conflict { existing_id: String, dest_dir: PathBuf },
    Io(String),
}

impl fmt::Display for ImportShaderpackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceInvalid(msg) => write!(f, "{msg}"),
            Self::Conflict {
                existing_id,
                dest_dir,
            } => write!(
                f,
                "shaderpack conflict: id={existing_id} already exists at {}",
                dest_dir.display()
            ),
            Self::Io(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for ImportShaderpackError {}

pub fn shaderpacks_root_dir() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("", "", "vesper") {
        proj_dirs.data_dir().join("shaderpacks")
    } else {
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
            .join(".local")
            .join("share")
            .join("vesper")
            .join("shaderpacks")
    }
}

pub fn discover_installed_shaderpacks() -> Result<Vec<Shaderpack>, String> {
    let root = shaderpacks_root_dir();
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let rd = fs::read_dir(&root).map_err(|e| format!("read_dir({}): {e}", root.display()))?;
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !path.join("shaderpack.toml").is_file() {
            continue;
        }
        match load_shaderpack(&path) {
            Ok(mut pack) => {
                if let Some(id) = path.file_name().and_then(|s| s.to_str()).map(|s| s.trim()) {
                    if !id.is_empty() {
                        pack.id = id.to_string();
                    }
                }
                out.push(pack)
            }
            Err(err) => {
                eprintln!("Shaderpack load failed ({}): {err}", path.display());
            }
        }
    }
    Ok(out)
}

#[allow(dead_code)]
pub fn discover_shaderpacks(root: &Path) -> Result<Vec<Shaderpack>, String> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let rd = fs::read_dir(root).map_err(|e| format!("read_dir({}): {e}", root.display()))?;
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !path.join("shaderpack.toml").is_file() {
            continue;
        }
        match load_shaderpack(&path) {
            Ok(pack) => out.push(pack),
            Err(err) => {
                eprintln!("Shaderpack load failed ({}): {err}", path.display());
            }
        }
    }
    Ok(out)
}

pub fn import_shaderpack(
    source_dir: &Path,
    policy: ImportConflictPolicy,
) -> Result<Shaderpack, ImportShaderpackError> {
    if !source_dir.is_dir() {
        return Err(ImportShaderpackError::SourceInvalid(format!(
            "import source is not a directory: {}",
            source_dir.display()
        )));
    }

    let root = shaderpacks_root_dir();
    fs::create_dir_all(&root)
        .map_err(|e| ImportShaderpackError::Io(format!("create_dir_all({}): {e}", root.display())))?;

    let source_canon = source_dir
        .canonicalize()
        .map_err(|e| ImportShaderpackError::SourceInvalid(format!("canonicalize({}): {e}", source_dir.display())))?;
    let root_canon = root
        .canonicalize()
        .unwrap_or_else(|_| root.clone());

    // If user points at an already-installed pack dir, just load it.
    if source_canon.starts_with(&root_canon) {
        let mut pack = load_shaderpack(&source_canon)
            .map_err(ImportShaderpackError::SourceInvalid)?;
        if let Some(id) = source_canon
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.trim())
        {
            if !id.is_empty() {
                pack.id = id.to_string();
            }
        }
        return Ok(pack);
    }

    // Validate early (also ensures required files exist).
    let source_pack = load_shaderpack(source_dir).map_err(ImportShaderpackError::SourceInvalid)?;

    let storage_id = sanitize_id(&source_pack.id).unwrap_or_else(|| generate_id_from_text("pack", &source_pack.name));
    let (dest_id, dest_dir) = resolve_import_destination(&root, &storage_id, policy)?;

    if let Err(err) = copy_dir_all_no_symlinks(&source_canon, &dest_dir) {
        let _ = fs::remove_dir_all(&dest_dir);
        return Err(ImportShaderpackError::Io(format!(
            "import copy failed: {err}"
        )));
    }

    let mut installed = match load_shaderpack(&dest_dir) {
        Ok(v) => v,
        Err(err) => {
            let _ = fs::remove_dir_all(&dest_dir);
            return Err(ImportShaderpackError::Io(format!(
                "import validation failed: {err}"
            )));
        }
    };
    installed.id = dest_id;
    Ok(installed)
}

pub fn import_shaderpack_from_path(
    source_path: &Path,
    policy: ImportConflictPolicy,
) -> Result<Shaderpack, ImportShaderpackError> {
    if source_path.is_dir() {
        return import_shaderpack(source_path, policy);
    }
    if source_path.is_file() && is_supported_archive(source_path) {
        return import_shaderpack_archive(source_path, policy);
    }
    Err(ImportShaderpackError::SourceInvalid(format!(
        "unsupported import source: {}",
        source_path.display()
    )))
}

fn is_supported_archive(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(ext.as_str(), "zip")
}

fn import_shaderpack_archive(
    archive_path: &Path,
    policy: ImportConflictPolicy,
) -> Result<Shaderpack, ImportShaderpackError> {
    if !archive_path.is_file() {
        return Err(ImportShaderpackError::SourceInvalid(format!(
            "archive path is not a file: {}",
            archive_path.display()
        )));
    }

    let temp_dir = create_temp_extract_dir().map_err(ImportShaderpackError::Io)?;
    if let Err(err) = extract_archive_bsdtar(archive_path, &temp_dir) {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(ImportShaderpackError::Io(err));
    }

    let pack_dir = match find_shaderpack_dir(&temp_dir) {
        Ok(v) => v,
        Err(err) => {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(ImportShaderpackError::SourceInvalid(err));
        }
    };

    let result = import_shaderpack(&pack_dir, policy);
    let _ = fs::remove_dir_all(&temp_dir);
    result
}

fn create_temp_extract_dir() -> Result<PathBuf, String> {
    let base = std::env::temp_dir().join("vesper-shaderpack-import");
    fs::create_dir_all(&base).map_err(|e| format!("create_dir_all({}): {e}", base.display()))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let pid = std::process::id();
    let dir = base.join(format!("{pid}-{now}"));
    fs::create_dir_all(&dir).map_err(|e| format!("create_dir_all({}): {e}", dir.display()))?;
    Ok(dir)
}

fn extract_archive_bsdtar(archive_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let output = Command::new("bsdtar")
        .arg("-xf")
        .arg(archive_path)
        .arg("-C")
        .arg(dest_dir)
        .output()
        .map_err(|e| format!("failed to run bsdtar: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("bsdtar extract failed: {stderr}"));
    }
    Ok(())
}

fn find_shaderpack_dir(root: &Path) -> Result<PathBuf, String> {
    let mut found: Vec<PathBuf> = Vec::new();
    find_shaderpack_dirs_recursive(root, 0, 6, &mut found)?;
    match found.len() {
        0 => Err("shaderpack.toml not found in archive".to_string()),
        1 => Ok(found.remove(0)),
        _ => Err("multiple shaderpack.toml found in archive".to_string()),
    }
}

fn find_shaderpack_dirs_recursive(
    dir: &Path,
    depth: usize,
    max_depth: usize,
    found: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if depth > max_depth {
        return Ok(());
    }
    if dir.join("shaderpack.toml").is_file() {
        found.push(dir.to_path_buf());
        return Ok(());
    }
    let rd = fs::read_dir(dir).map_err(|e| format!("read_dir({}): {e}", dir.display()))?;
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            find_shaderpack_dirs_recursive(&path, depth + 1, max_depth, found)?;
            if found.len() > 1 {
                return Ok(());
            }
        }
    }
    Ok(())
}

#[allow(dead_code)]
pub fn delete_installed_shaderpack(pack_id: &str) -> Result<PathBuf, String> {
    let pack_id = pack_id.trim();
    if pack_id.is_empty() {
        return Err("pack_id is empty".to_string());
    }
    let dir = shaderpacks_root_dir().join(pack_id);
    if !dir.exists() {
        return Err(format!("shaderpack not found: {}", dir.display()));
    }
    fs::remove_dir_all(&dir).map_err(|e| format!("remove_dir_all({}): {e}", dir.display()))?;
    Ok(dir)
}

pub fn delete_installed_shaderpack_and_clean_config(
    pack_id: &str,
    config: &mut Config,
) -> Result<usize, String> {
    let pack_id = pack_id.trim();
    if pack_id.is_empty() {
        return Err("pack_id is empty".to_string());
    }
    let dir = shaderpacks_root_dir().join(pack_id);
    if !dir.exists() {
        return Err(format!("shaderpack not found: {}", dir.display()));
    }

    let dir_canon = dir.canonicalize().unwrap_or_else(|_| dir.clone());
    let cleared = config.clear_shadertoy_paths_under(&dir_canon);

    fs::remove_dir_all(&dir).map_err(|e| format!("remove_dir_all({}): {e}", dir.display()))?;
    Ok(cleared)
}

fn load_manifest_shaders(
    pack_dir: &Path,
    pack_id: &str,
    shaders: &[ShaderSection],
) -> Result<Vec<ShaderEntry>, String> {
    let pack_dir_canon = pack_dir
        .canonicalize()
        .map_err(|e| format!("canonicalize({}): {e}", pack_dir.display()))?;

    let mut used_ids: HashSet<String> = HashSet::new();
    let mut out = Vec::new();

    for shader in shaders {
        let shader_name = shader
            .name
            .as_deref()
            .or(shader.path.as_deref().and_then(|p| Path::new(p).file_name()?.to_str()))
            .unwrap_or("")
            .trim();
        if shader_name.is_empty() {
            return Err("shader.name (or shader.path) is required".to_string());
        }

        let shader_id = shader
            .id
            .as_deref()
            .and_then(sanitize_id)
            .unwrap_or_else(|| generate_id_from_text(pack_id, shader_name));
        if !used_ids.insert(shader_id.clone()) {
            return Err(format!("duplicate shader id: {shader_id}"));
        }

        let rel = shader
            .path
            .as_deref()
            .ok_or_else(|| format!("shader.path is required for {shader_id}"))?;
        let dir = pack_dir.join(rel);
        if !dir.is_dir() {
            return Err(format!(
                "shader directory not found for {shader_id}: {}",
                dir.display()
            ));
        }

        let dir_canon = dir
            .canonicalize()
            .map_err(|e| format!("canonicalize({}): {e}", dir.display()))?;
        if !dir_canon.starts_with(&pack_dir_canon) {
            return Err(format!("shader.path escapes pack dir: {rel}"));
        }

        let detected = detect_shadertoy_files_in_dir(&dir);
        if detected.image.is_none() {
            return Err(format!(
                "shader {shader_id} has no Image shader (Image.glsl/.frag/.fs)"
            ));
        }
        let preview_path = optional_preview_png(&dir);

        out.push(ShaderEntry {
            id: shader_id,
            name: shader_name.to_string(),
            description: shader.description.clone(),
            dir,
            detected,
            preview_path,
        });
    }

    Ok(out)
}

fn scan_shader_dirs(
    pack_dir: &Path,
    pack_id: &str,
    shaders_dir: &Path,
) -> Result<Vec<ShaderEntry>, String> {
    if !shaders_dir.is_dir() {
        return Err(format!(
            "shaderpack has no shaders/ directory: {}",
            shaders_dir.display()
        ));
    }

    let pack_dir_canon = pack_dir
        .canonicalize()
        .unwrap_or_else(|_| pack_dir.to_path_buf());
    let mut used_ids: HashSet<String> = HashSet::new();
    let existing_dirs: HashSet<PathBuf> = HashSet::new();
    scan_shader_dirs_extra(
        &pack_dir_canon,
        pack_id,
        shaders_dir,
        &mut used_ids,
        &existing_dirs,
    )
}

fn scan_shader_dirs_extra(
    pack_dir_canon: &Path,
    pack_id: &str,
    shaders_dir: &Path,
    used_ids: &mut HashSet<String>,
    existing_dirs: &HashSet<PathBuf>,
) -> Result<Vec<ShaderEntry>, String> {
    if !shaders_dir.is_dir() {
        return Ok(Vec::new());
    }

    let rd = fs::read_dir(shaders_dir)
        .map_err(|e| format!("read_dir({}): {e}", shaders_dir.display()))?;

    let mut out = Vec::new();
    for entry in rd.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }

        let dir_canon = match dir.canonicalize() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if !dir_canon.starts_with(pack_dir_canon) {
            // Ignore potential symlink escapes instead of failing the whole pack load.
            continue;
        }
        if existing_dirs.contains(&dir_canon) {
            continue;
        }

        let Some(name) = dir.file_name().and_then(|s| s.to_str()).map(|s| s.trim()) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }

        let detected = detect_shadertoy_files_in_dir(&dir);
        if detected.image.is_none() {
            continue;
        }

        let shader_id = generate_unique_id(pack_id, name, used_ids);
        let preview_path = optional_preview_png(&dir);
        out.push(ShaderEntry {
            id: shader_id,
            name: name.to_string(),
            description: None,
            dir,
            detected,
            preview_path,
        });
    }

    out.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
    Ok(out)
}

fn detect_shadertoy_files_in_dir(dir: &Path) -> DetectedShadertoyFiles {
    let mut image: Option<PathBuf> = None;
    let mut common: Option<PathBuf> = None;
    let mut buffers: [Option<PathBuf>; 4] = [None, None, None, None];
    let mut sound: Option<PathBuf> = None;

    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if !is_shader_extension(&path) {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let key = normalize_name(stem);
            match key.as_str() {
                "image" | "mainimage" => {
                    image.get_or_insert(path);
                }
                "common" => {
                    common.get_or_insert(path);
                }
                "buffera" => {
                    buffers[0].get_or_insert(path);
                }
                "bufferb" => {
                    buffers[1].get_or_insert(path);
                }
                "bufferc" => {
                    buffers[2].get_or_insert(path);
                }
                "bufferd" => {
                    buffers[3].get_or_insert(path);
                }
                "sound" => {
                    sound.get_or_insert(path);
                }
                _ => {}
            };
        }
    }

    DetectedShadertoyFiles {
        common,
        image,
        buffers,
        sound,
    }
}

fn optional_preview_png(dir: &Path) -> Option<PathBuf> {
    let path = dir.join("preview.png");
    if path.is_file() { Some(path) } else { None }
}

fn is_shader_extension(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(ext.as_str(), "glsl" | "frag" | "fs")
}

fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn sanitize_id(id: &str) -> Option<String> {
    let id = id.trim();
    if id.is_empty() {
        return None;
    }
    let mut out = String::new();
    for c in id.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c.to_ascii_lowercase());
        } else if c.is_whitespace() {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn generate_unique_id(prefix: &str, name: &str, used: &mut HashSet<String>) -> String {
    let base = generate_id_from_text(prefix, name);
    if used.insert(base.clone()) {
        return base;
    }
    for i in 2..=9999u32 {
        let candidate = format!("{base}-{i}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    base
}

fn generate_id_from_text(prefix: &str, text: &str) -> String {
    let slug = slugify(text);
    if slug.is_empty() {
        stable_hash_id(prefix, text)
    } else {
        slug
    }
}

fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for c in text.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
            continue;
        }
        if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn stable_hash_id(prefix: &str, text: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in text.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{prefix}-{hash:016x}")
}

fn resolve_import_destination(
    root: &Path,
    desired_id: &str,
    policy: ImportConflictPolicy,
) -> Result<(String, PathBuf), ImportShaderpackError> {
    let desired_id = desired_id.trim();
    if desired_id.is_empty() {
        return Err(ImportShaderpackError::SourceInvalid(
            "cannot import shaderpack: empty id".to_string(),
        ));
    }

    let desired_dir = root.join(desired_id);
    if !desired_dir.exists() {
        return Ok((desired_id.to_string(), desired_dir));
    }

    match policy {
        ImportConflictPolicy::Abort => Err(ImportShaderpackError::Conflict {
            existing_id: desired_id.to_string(),
            dest_dir: desired_dir,
        }),
        ImportConflictPolicy::Replace => {
            fs::remove_dir_all(&desired_dir).map_err(|e| {
                ImportShaderpackError::Io(format!(
                    "remove_dir_all({}): {e}",
                    desired_dir.display()
                ))
            })?;
            Ok((desired_id.to_string(), desired_dir))
        }
        ImportConflictPolicy::Rename => {
            for i in 2..=9999u32 {
                let candidate = format!("{desired_id}-{i}");
                let dir = root.join(&candidate);
                if !dir.exists() {
                    return Ok((candidate, dir));
                }
            }
            Err(ImportShaderpackError::Io(
                "unable to find free shaderpack id".to_string(),
            ))
        }
    }
}

fn copy_dir_all_no_symlinks(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.is_dir() {
        return Err(format!("copy source is not a dir: {}", src.display()));
    }
    if dst.exists() {
        return Err(format!("copy destination already exists: {}", dst.display()));
    }

    fs::create_dir_all(dst).map_err(|e| format!("create_dir_all({}): {e}", dst.display()))?;

    let rd = fs::read_dir(src).map_err(|e| format!("read_dir({}): {e}", src.display()))?;
    for entry in rd.flatten() {
        let src_path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(file_name);

        let meta = fs::symlink_metadata(&src_path)
            .map_err(|e| format!("metadata({}): {e}", src_path.display()))?;
        if meta.file_type().is_symlink() {
            return Err(format!(
                "symlinks are not allowed in shaderpacks: {}",
                src_path.display()
            ));
        }

        if meta.is_dir() {
            copy_dir_all_no_symlinks(&src_path, &dst_path)?;
        } else if meta.is_file() {
            fs::copy(&src_path, &dst_path).map_err(|e| {
                format!(
                    "copy file {} -> {}: {e}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        }
    }

    Ok(())
}
