use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use roxmltree::{Document, Node};

use crate::app::RenderableKind;
use crate::sprite_keys::validate_sprite_key;
use crate::AppPaths;

use super::database::{DefDatabase, EntityArchetype, EntityDefId};
use super::discovery::discover_mod_sources;
use super::types::{ContentPlanError, ContentPlanRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentErrorCode {
    Discovery,
    ReadFile,
    XmlMalformed,
    InvalidRoot,
    UnknownDefType,
    UnknownField,
    DuplicateField,
    MissingField,
    InvalidValue,
    DuplicateDefInMod,
    MissingOverrideTarget,
}

#[derive(Debug, Clone)]
pub struct ContentCompileError {
    pub code: ContentErrorCode,
    pub message: String,
    pub mod_id: String,
    pub def_name: Option<String>,
    pub field_name: Option<String>,
    pub file_path: PathBuf,
    pub location: Option<SourceLocation>,
}

impl fmt::Display for ContentCompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.location {
            Some(loc) => write!(
                f,
                "{:?}: {} (mod={}, def={}, field={}, file={}, line={}, column={})",
                self.code,
                self.message,
                self.mod_id,
                self.def_name.as_deref().unwrap_or("-"),
                self.field_name.as_deref().unwrap_or("-"),
                self.file_path.display(),
                loc.line,
                loc.column
            ),
            None => write!(
                f,
                "{:?}: {} (mod={}, def={}, field={}, file={})",
                self.code,
                self.message,
                self.mod_id,
                self.def_name.as_deref().unwrap_or("-"),
                self.field_name.as_deref().unwrap_or("-"),
                self.file_path.display()
            ),
        }
    }
}

impl std::error::Error for ContentCompileError {}

#[derive(Debug, Clone)]
pub struct CompiledEntityDef {
    pub def_name: String,
    pub label: Option<String>,
    pub renderable: Option<RenderableKind>,
    pub move_speed: Option<f32>,
    pub health_max: Option<u32>,
    pub base_damage: Option<u32>,
    pub aggro_radius: Option<f32>,
    pub attack_range: Option<f32>,
    pub attack_cooldown_seconds: Option<f32>,
    pub tags: Option<Vec<String>>,
    pub source_mod_id: String,
    pub source_file_path: PathBuf,
    pub source_location: Option<SourceLocation>,
}

#[derive(Debug, Clone, Default)]
struct MergedEntityDef {
    label: Option<String>,
    renderable: Option<RenderableKind>,
    move_speed: Option<f32>,
    health_max: Option<u32>,
    base_damage: Option<u32>,
    aggro_radius: Option<f32>,
    attack_range: Option<f32>,
    attack_cooldown_seconds: Option<f32>,
    tags: Option<Vec<String>>,
}

pub fn compile_mod_entity_defs(
    source_dir: &Path,
    mod_id: &str,
) -> Result<Vec<CompiledEntityDef>, ContentCompileError> {
    let xml_files = collect_xml_files_sorted(source_dir)
        .map_err(|error| read_error(mod_id, error.path, error.source))?;
    let mut defs = Vec::<CompiledEntityDef>::new();
    let mut seen_defs = HashSet::<String>::new();

    for xml_file in xml_files {
        let raw = fs::read_to_string(&xml_file)
            .map_err(|source| read_error(mod_id, xml_file.clone(), source))?;
        let parsed = parse_defs_document(mod_id, &xml_file, &raw)?;
        for def in parsed {
            if !seen_defs.insert(def.def_name.clone()) {
                return Err(ContentCompileError {
                    code: ContentErrorCode::DuplicateDefInMod,
                    message: format!(
                        "duplicate EntityDef '{}' in mod '{}'; each mod may define a defName only once",
                        def.def_name, mod_id
                    ),
                    mod_id: mod_id.to_string(),
                    def_name: Some(def.def_name.clone()),
                    field_name: None,
                    file_path: xml_file.clone(),
                    location: None,
                });
            }
            defs.push(def);
        }
    }

    Ok(defs)
}

pub fn compile_def_database(
    app_paths: &AppPaths,
    request: &ContentPlanRequest,
) -> Result<DefDatabase, ContentCompileError> {
    let sources = discover_mod_sources(app_paths, request)
        .map_err(|error| map_discovery_error(error, &app_paths.root))?;
    let mut defs = Vec::<CompiledEntityDef>::new();
    for source in sources {
        defs.extend(compile_mod_entity_defs(&source.source_dir, &source.mod_id)?);
    }
    def_database_from_compiled_defs(defs)
}

pub(crate) fn def_database_from_compiled_defs(
    defs: Vec<CompiledEntityDef>,
) -> Result<DefDatabase, ContentCompileError> {
    let merged = merge_compiled_entity_defs(defs)?;
    Ok(materialize_database(merged))
}

fn merge_compiled_entity_defs(
    defs: Vec<CompiledEntityDef>,
) -> Result<BTreeMap<String, MergedEntityDef>, ContentCompileError> {
    let mut merged = BTreeMap::<String, MergedEntityDef>::new();
    for def in defs {
        match merged.get_mut(&def.def_name) {
            Some(existing) => apply_patch(existing, &def),
            None => {
                if def.label.is_none() || def.renderable.is_none() {
                    return Err(missing_override_target_error(&def));
                }
                let mut initial = MergedEntityDef::default();
                apply_patch(&mut initial, &def);
                merged.insert(def.def_name.clone(), initial);
            }
        }
    }
    Ok(merged)
}

fn apply_patch(target: &mut MergedEntityDef, patch: &CompiledEntityDef) {
    if let Some(label) = &patch.label {
        target.label = Some(label.clone());
    }
    if let Some(renderable) = patch.renderable.clone() {
        target.renderable = Some(renderable);
    }
    if let Some(move_speed) = patch.move_speed {
        target.move_speed = Some(move_speed);
    }
    if let Some(health_max) = patch.health_max {
        target.health_max = Some(health_max);
    }
    if let Some(base_damage) = patch.base_damage {
        target.base_damage = Some(base_damage);
    }
    if let Some(aggro_radius) = patch.aggro_radius {
        target.aggro_radius = Some(aggro_radius);
    }
    if let Some(attack_range) = patch.attack_range {
        target.attack_range = Some(attack_range);
    }
    if let Some(attack_cooldown_seconds) = patch.attack_cooldown_seconds {
        target.attack_cooldown_seconds = Some(attack_cooldown_seconds);
    }
    if let Some(tags) = &patch.tags {
        target.tags = Some(tags.clone());
    }
}

fn materialize_database(merged: BTreeMap<String, MergedEntityDef>) -> DefDatabase {
    let defs = merged
        .into_iter()
        .map(|(def_name, merged)| EntityArchetype {
            id: EntityDefId(0),
            def_name,
            label: merged
                .label
                .expect("merged defs always include label after validation"),
            renderable: merged
                .renderable
                .expect("merged defs always include renderable after validation"),
            move_speed: merged.move_speed.unwrap_or(5.0),
            health_max: merged.health_max,
            base_damage: merged.base_damage,
            aggro_radius: merged.aggro_radius,
            attack_range: merged.attack_range,
            attack_cooldown_seconds: merged.attack_cooldown_seconds,
            tags: merged.tags.unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    DefDatabase::from_entity_defs(defs)
}

fn missing_override_target_error(def: &CompiledEntityDef) -> ContentCompileError {
    ContentCompileError {
        code: ContentErrorCode::MissingOverrideTarget,
        message: format!(
            "EntityDef '{}' is a partial override but no earlier definition exists; define it in base or an earlier mod, or provide full fields (label, renderable)",
            def.def_name
        ),
        mod_id: def.source_mod_id.clone(),
        def_name: Some(def.def_name.clone()),
        field_name: None,
        file_path: def.source_file_path.clone(),
        location: def.source_location,
    }
}

fn parse_defs_document(
    mod_id: &str,
    file_path: &Path,
    raw: &str,
) -> Result<Vec<CompiledEntityDef>, ContentCompileError> {
    let doc = Document::parse(raw).map_err(|error| ContentCompileError {
        code: ContentErrorCode::XmlMalformed,
        message: format!("malformed XML: {error}"),
        mod_id: mod_id.to_string(),
        def_name: None,
        field_name: None,
        file_path: file_path.to_path_buf(),
        location: Some(SourceLocation {
            line: error.pos().row as usize,
            column: error.pos().col as usize,
        }),
    })?;

    let root = doc.root_element();
    if root.tag_name().name() != "Defs" {
        return Err(error_at_node(
            ContentErrorCode::InvalidRoot,
            "root element must be <Defs>".to_string(),
            mod_id,
            file_path,
            &doc,
            root,
        ));
    }

    let mut defs = Vec::<CompiledEntityDef>::new();
    for child in root.children().filter(|node| node.is_element()) {
        if child.tag_name().name() != "EntityDef" {
            return Err(error_at_node(
                ContentErrorCode::UnknownDefType,
                format!(
                    "unsupported def type <{}>; MVP supports only <EntityDef>",
                    child.tag_name().name()
                ),
                mod_id,
                file_path,
                &doc,
                child,
            ));
        }
        defs.push(parse_entity_def(mod_id, file_path, &doc, child)?);
    }

    Ok(defs)
}

fn parse_entity_def(
    mod_id: &str,
    file_path: &Path,
    doc: &Document<'_>,
    node: Node<'_, '_>,
) -> Result<CompiledEntityDef, ContentCompileError> {
    let def_name_hint = def_name_hint_from_node(node);
    let mut seen_fields = HashSet::<String>::new();
    let mut def_name = None::<String>;
    let mut label = None::<String>;
    let mut renderable = None::<RenderableKind>;
    let mut move_speed = None::<f32>;
    let mut health_max = None::<u32>;
    let mut base_damage = None::<u32>;
    let mut aggro_radius = None::<f32>;
    let mut attack_range = None::<f32>;
    let mut attack_cooldown_seconds = None::<f32>;
    let mut tags = None::<Vec<String>>;

    for field in node.children().filter(|child| child.is_element()) {
        let field_name = field.tag_name().name().to_string();
        if !seen_fields.insert(field_name.clone()) {
            return Err(error_at_node_with_context(
                ContentErrorCode::DuplicateField,
                format!("duplicate field <{}> in <EntityDef>", field_name),
                mod_id,
                file_path,
                doc,
                field,
                def_name_hint.as_deref(),
                Some(field_name.as_str()),
            ));
        }

        match field_name.as_str() {
            "defName" => def_name = Some(required_text(mod_id, file_path, doc, field, "defName")?),
            "label" => label = Some(required_text(mod_id, file_path, doc, field, "label")?),
            "renderable" => {
                let parsed = parse_renderable(mod_id, file_path, doc, field)?;
                renderable = Some(parsed);
            }
            "moveSpeed" => {
                let value = required_text(mod_id, file_path, doc, field, "moveSpeed")?;
                let parsed = value.parse::<f32>().map_err(|_| {
                    error_at_node_with_context(
                        ContentErrorCode::InvalidValue,
                        format!("moveSpeed '{}' is not a valid number", value),
                        mod_id,
                        file_path,
                        doc,
                        field,
                        def_name_hint.as_deref(),
                        Some("moveSpeed"),
                    )
                })?;
                if !parsed.is_finite() || parsed < 0.0 {
                    return Err(error_at_node_with_context(
                        ContentErrorCode::InvalidValue,
                        "moveSpeed must be finite and >= 0".to_string(),
                        mod_id,
                        file_path,
                        doc,
                        field,
                        def_name_hint.as_deref(),
                        Some("moveSpeed"),
                    ));
                }
                move_speed = Some(parsed);
            }
            "health_max" => {
                let parsed = parse_u32_field(
                    mod_id,
                    file_path,
                    doc,
                    field,
                    def_name_hint.as_deref(),
                    "health_max",
                    true,
                )?;
                health_max = Some(parsed);
            }
            "base_damage" => {
                let parsed = parse_u32_field(
                    mod_id,
                    file_path,
                    doc,
                    field,
                    def_name_hint.as_deref(),
                    "base_damage",
                    false,
                )?;
                base_damage = Some(parsed);
            }
            "aggro_radius" => {
                let parsed = parse_non_negative_f32_field(
                    mod_id,
                    file_path,
                    doc,
                    field,
                    def_name_hint.as_deref(),
                    "aggro_radius",
                )?;
                aggro_radius = Some(parsed);
            }
            "attack_range" => {
                let parsed = parse_non_negative_f32_field(
                    mod_id,
                    file_path,
                    doc,
                    field,
                    def_name_hint.as_deref(),
                    "attack_range",
                )?;
                attack_range = Some(parsed);
            }
            "attack_cooldown_seconds" => {
                let parsed = parse_non_negative_f32_field(
                    mod_id,
                    file_path,
                    doc,
                    field,
                    def_name_hint.as_deref(),
                    "attack_cooldown_seconds",
                )?;
                attack_cooldown_seconds = Some(parsed);
            }
            "tags" => tags = Some(parse_tags(mod_id, file_path, doc, field)?),
            _ => {
                return Err(error_at_node_with_context(
                    ContentErrorCode::UnknownField,
                    format!("unknown field <{}> in <EntityDef>", field_name),
                    mod_id,
                    file_path,
                    doc,
                    field,
                    def_name_hint.as_deref(),
                    Some(field_name.as_str()),
                ))
            }
        }
    }

    let Some(def_name) = def_name else {
        return Err(error_at_node(
            ContentErrorCode::MissingField,
            "missing required field <defName> in <EntityDef>".to_string(),
            mod_id,
            file_path,
            doc,
            node,
        ));
    };
    let pos = doc.text_pos_at(node.range().start);
    Ok(CompiledEntityDef {
        def_name,
        label,
        renderable,
        move_speed,
        health_max,
        base_damage,
        aggro_radius,
        attack_range,
        attack_cooldown_seconds,
        tags,
        source_mod_id: mod_id.to_string(),
        source_file_path: file_path.to_path_buf(),
        source_location: Some(SourceLocation {
            line: pos.row as usize,
            column: pos.col as usize,
        }),
    })
}

fn def_name_hint_from_node(node: Node<'_, '_>) -> Option<String> {
    node.children()
        .filter(|child| child.is_element() && child.tag_name().name() == "defName")
        .find_map(|def_name| {
            let value = def_name.text().map(str::trim).unwrap_or_default();
            if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            }
        })
}

fn parse_tags(
    mod_id: &str,
    file_path: &Path,
    doc: &Document<'_>,
    tags_node: Node<'_, '_>,
) -> Result<Vec<String>, ContentCompileError> {
    let mut tags = Vec::<String>::new();
    for child in tags_node.children().filter(|child| child.is_element()) {
        if child.tag_name().name() != "li" {
            return Err(error_at_node(
                ContentErrorCode::UnknownField,
                format!(
                    "unknown field <{}> inside <tags>; expected <li>",
                    child.tag_name().name()
                ),
                mod_id,
                file_path,
                doc,
                child,
            ));
        }
        tags.push(required_text(mod_id, file_path, doc, child, "li")?);
    }
    Ok(tags)
}

fn parse_renderable(
    mod_id: &str,
    file_path: &Path,
    doc: &Document<'_>,
    node: Node<'_, '_>,
) -> Result<RenderableKind, ContentCompileError> {
    for attr in node.attributes() {
        if attr.name() != "kind" && attr.name() != "spriteKey" {
            return Err(error_at_node(
                ContentErrorCode::UnknownField,
                format!(
                    "unknown attribute '{}' on <renderable>; allowed attributes: kind, spriteKey",
                    attr.name()
                ),
                mod_id,
                file_path,
                doc,
                node,
            ));
        }
    }

    let kind_attr = node.attribute("kind");
    let sprite_key_attr = node.attribute("spriteKey");
    let text_value = node.text().map(str::trim).unwrap_or_default();

    if let Some(kind) = kind_attr {
        if !text_value.is_empty() {
            return Err(error_at_node(
                ContentErrorCode::InvalidValue,
                "renderable with 'kind' attribute must not also include non-whitespace text"
                    .to_string(),
                mod_id,
                file_path,
                doc,
                node,
            ));
        }

        return match kind {
            "Placeholder" => {
                if sprite_key_attr.is_some() {
                    Err(error_at_node(
                        ContentErrorCode::InvalidValue,
                        "renderable kind='Placeholder' must not include spriteKey".to_string(),
                        mod_id,
                        file_path,
                        doc,
                        node,
                    ))
                } else {
                    Ok(RenderableKind::Placeholder)
                }
            }
            "Sprite" => {
                let Some(key) = sprite_key_attr else {
                    return Err(error_at_node(
                        ContentErrorCode::InvalidValue,
                        "renderable kind='Sprite' requires spriteKey".to_string(),
                        mod_id,
                        file_path,
                        doc,
                        node,
                    ));
                };
                validate_sprite_key(key).map_err(|error| {
                    error_at_node(
                        ContentErrorCode::InvalidValue,
                        format!("invalid sprite key '{key}': {error}"),
                        mod_id,
                        file_path,
                        doc,
                        node,
                    )
                })?;
                Ok(RenderableKind::Sprite(key.to_string()))
            }
            _ => Err(error_at_node(
                ContentErrorCode::InvalidValue,
                format!(
                    "invalid renderable kind '{}'; allowed values: Placeholder or Sprite",
                    kind
                ),
                mod_id,
                file_path,
                doc,
                node,
            )),
        };
    }

    if sprite_key_attr.is_some() {
        return Err(error_at_node(
            ContentErrorCode::InvalidValue,
            "renderable attribute spriteKey requires kind".to_string(),
            mod_id,
            file_path,
            doc,
            node,
        ));
    }

    let value = required_text(mod_id, file_path, doc, node, "renderable")?;
    match value.as_str() {
        "Placeholder" => Ok(RenderableKind::Placeholder),
        _ if value.starts_with("Sprite:") => {
            let key = value.trim_start_matches("Sprite:");
            validate_sprite_key(key).map_err(|error| {
                error_at_node(
                    ContentErrorCode::InvalidValue,
                    format!("invalid sprite key '{key}': {error}"),
                    mod_id,
                    file_path,
                    doc,
                    node,
                )
            })?;
            Ok(RenderableKind::Sprite(key.to_string()))
        }
        _ => Err(error_at_node(
            ContentErrorCode::InvalidValue,
            format!(
                "invalid renderable '{}'; allowed values: Placeholder, Sprite:<key>, or <renderable kind=\"...\" .../>",
                value
            ),
            mod_id,
            file_path,
            doc,
            node,
        )),
    }
}

fn required_text(
    mod_id: &str,
    file_path: &Path,
    doc: &Document<'_>,
    node: Node<'_, '_>,
    field_name: &str,
) -> Result<String, ContentCompileError> {
    let value = node.text().map(str::trim).unwrap_or_default().to_string();
    if value.is_empty() {
        return Err(error_at_node(
            ContentErrorCode::MissingField,
            format!("field <{}> must not be empty", field_name),
            mod_id,
            file_path,
            doc,
            node,
        ));
    }
    Ok(value)
}

fn parse_u32_field(
    mod_id: &str,
    file_path: &Path,
    doc: &Document<'_>,
    node: Node<'_, '_>,
    def_name: Option<&str>,
    field_name: &str,
    strictly_positive: bool,
) -> Result<u32, ContentCompileError> {
    let value = required_text(mod_id, file_path, doc, node, field_name)?;
    let parsed = value.parse::<u32>().map_err(|_| {
        error_at_node_with_context(
            ContentErrorCode::InvalidValue,
            format!("{field_name} '{value}' is not a valid u32"),
            mod_id,
            file_path,
            doc,
            node,
            def_name,
            Some(field_name),
        )
    })?;
    if strictly_positive && parsed == 0 {
        return Err(error_at_node_with_context(
            ContentErrorCode::InvalidValue,
            format!("{field_name} must be > 0"),
            mod_id,
            file_path,
            doc,
            node,
            def_name,
            Some(field_name),
        ));
    }
    Ok(parsed)
}

fn parse_non_negative_f32_field(
    mod_id: &str,
    file_path: &Path,
    doc: &Document<'_>,
    node: Node<'_, '_>,
    def_name: Option<&str>,
    field_name: &str,
) -> Result<f32, ContentCompileError> {
    let value = required_text(mod_id, file_path, doc, node, field_name)?;
    let parsed = value.parse::<f32>().map_err(|_| {
        error_at_node_with_context(
            ContentErrorCode::InvalidValue,
            format!("{field_name} '{value}' is not a valid number"),
            mod_id,
            file_path,
            doc,
            node,
            def_name,
            Some(field_name),
        )
    })?;
    if !parsed.is_finite() || parsed < 0.0 {
        return Err(error_at_node_with_context(
            ContentErrorCode::InvalidValue,
            format!("{field_name} must be finite and >= 0"),
            mod_id,
            file_path,
            doc,
            node,
            def_name,
            Some(field_name),
        ));
    }
    Ok(parsed)
}

fn error_at_node(
    code: ContentErrorCode,
    message: String,
    mod_id: &str,
    file_path: &Path,
    doc: &Document<'_>,
    node: Node<'_, '_>,
) -> ContentCompileError {
    error_at_node_with_context(code, message, mod_id, file_path, doc, node, None, None)
}

fn error_at_node_with_context(
    code: ContentErrorCode,
    message: String,
    mod_id: &str,
    file_path: &Path,
    doc: &Document<'_>,
    node: Node<'_, '_>,
    def_name: Option<&str>,
    field_name: Option<&str>,
) -> ContentCompileError {
    let pos = doc.text_pos_at(node.range().start);
    ContentCompileError {
        code,
        message,
        mod_id: mod_id.to_string(),
        def_name: def_name.map(ToString::to_string),
        field_name: field_name.map(ToString::to_string),
        file_path: file_path.to_path_buf(),
        location: Some(SourceLocation {
            line: pos.row as usize,
            column: pos.col as usize,
        }),
    }
}

struct ReadError {
    path: PathBuf,
    source: std::io::Error,
}

fn collect_xml_files_sorted(root: &Path) -> Result<Vec<PathBuf>, ReadError> {
    let mut files = Vec::<PathBuf>::new();
    collect_recursive(root, &mut files)?;
    files.sort_by(|a, b| {
        normalize_rel_path(a.strip_prefix(root).expect("under root")).cmp(&normalize_rel_path(
            b.strip_prefix(root).expect("under root"),
        ))
    });
    Ok(files)
}

fn collect_recursive(current: &Path, files: &mut Vec<PathBuf>) -> Result<(), ReadError> {
    let entries = fs::read_dir(current).map_err(|source| ReadError {
        path: current.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| ReadError {
            path: current.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_recursive(&path, files)?;
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn normalize_rel_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn read_error(mod_id: &str, path: PathBuf, source: std::io::Error) -> ContentCompileError {
    ContentCompileError {
        code: ContentErrorCode::ReadFile,
        message: format!("failed to read XML file: {source}"),
        mod_id: mod_id.to_string(),
        def_name: None,
        field_name: None,
        file_path: path,
        location: None,
    }
}

fn map_discovery_error(error: ContentPlanError, root: &Path) -> ContentCompileError {
    match error {
        ContentPlanError::EnabledModMissing {
            mod_id,
            expected_dir,
        } => ContentCompileError {
            code: ContentErrorCode::Discovery,
            message: format!(
                "enabled mod '{}' not found at {}; check enabled mod list",
                mod_id,
                expected_dir.display()
            ),
            mod_id,
            def_name: None,
            field_name: None,
            file_path: expected_dir,
            location: None,
        },
        other => ContentCompileError {
            code: ContentErrorCode::Discovery,
            message: other.to_string(),
            mod_id: "<discovery>".to_string(),
            def_name: None,
            field_name: None,
            file_path: root.to_path_buf(),
            location: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;

    fn setup_app_paths(root: &Path) -> AppPaths {
        let base = root.join("assets").join("base");
        let mods = root.join("mods");
        let cache = root.join("cache");
        fs::create_dir_all(&base).expect("base");
        fs::create_dir_all(&mods).expect("mods");
        fs::create_dir_all(&cache).expect("cache");
        AppPaths {
            root: root.to_path_buf(),
            base_content_dir: base,
            mods_dir: mods,
            cache_dir: cache,
        }
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(path, content).expect("write");
    }

    fn fixture_root(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("docs")
            .join("fixtures")
            .join("content_pipeline_v1")
            .join(name)
    }

    fn copy_dir_recursive(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).expect("mkdir dst");
        let entries = fs::read_dir(src).expect("read src");
        for entry in entries {
            let entry = entry.expect("entry");
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if src_path.is_dir() {
                copy_dir_recursive(&src_path, &dst_path);
            } else {
                fs::copy(&src_path, &dst_path).expect("copy");
            }
        }
    }

    #[test]
    fn valid_compile_assigns_stable_ids_by_def_name() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs>
                <EntityDef><defName>zeta</defName><label>Zeta</label><renderable>Placeholder</renderable></EntityDef>
                <EntityDef><defName>alpha</defName><label>Alpha</label><renderable>Placeholder</renderable></EntityDef>
            </Defs>"#,
        );
        let db = compile_def_database(&app, &ContentPlanRequest::default()).expect("compile");
        let alpha = db.entity_def_id_by_name("alpha").expect("alpha");
        let zeta = db.entity_def_id_by_name("zeta").expect("zeta");
        assert!(alpha.0 < zeta.0);
    }

    #[test]
    fn missing_label_for_full_definition_fails() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>x</defName><renderable>Placeholder</renderable></EntityDef></Defs>"#,
        );
        let err = compile_def_database(&app, &ContentPlanRequest::default()).expect_err("err");
        assert_eq!(err.code, ContentErrorCode::MissingOverrideTarget);
        assert_eq!(err.mod_id, "base");
    }

    #[test]
    fn missing_def_name_reports_mod_file_and_location() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><label>X</label><renderable>Placeholder</renderable></EntityDef></Defs>"#,
        );
        let err = compile_def_database(&app, &ContentPlanRequest::default()).expect_err("err");
        assert_eq!(err.code, ContentErrorCode::MissingField);
        assert_eq!(err.mod_id, "base");
        assert!(err
            .file_path
            .ends_with(Path::new("assets").join("base").join("defs.xml")));
        assert!(err.location.is_some());
    }

    #[test]
    fn unknown_field_errors() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>a</defName><label>A</label><renderable>Placeholder</renderable><mood>Happy</mood></EntityDef></Defs>"#,
        );
        let err = compile_def_database(&app, &ContentPlanRequest::default()).expect_err("err");
        assert_eq!(err.code, ContentErrorCode::UnknownField);
    }

    #[test]
    fn invalid_renderable_errors() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>a</defName><label>A</label><renderable>Sprite</renderable></EntityDef></Defs>"#,
        );
        let err = compile_def_database(&app, &ContentPlanRequest::default()).expect_err("err");
        assert_eq!(err.code, ContentErrorCode::InvalidValue);
    }

    #[test]
    fn sprite_renderable_parses_when_key_is_valid() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>a</defName><label>A</label><renderable>Sprite:player</renderable></EntityDef></Defs>"#,
        );
        let db = compile_def_database(&app, &ContentPlanRequest::default()).expect("compile");
        let id = db.entity_def_id_by_name("a").expect("id");
        let def = db.entity_def(id).expect("def");
        assert_eq!(def.renderable, RenderableKind::Sprite("player".to_string()));
    }

    #[test]
    fn sprite_renderable_rejects_invalid_key() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>a</defName><label>A</label><renderable>Sprite:a.b</renderable></EntityDef></Defs>"#,
        );
        let err = compile_def_database(&app, &ContentPlanRequest::default()).expect_err("err");
        assert_eq!(err.code, ContentErrorCode::InvalidValue);
    }

    #[test]
    fn renderable_kind_attribute_parses_sprite() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>a</defName><label>A</label><renderable kind="Sprite" spriteKey="player"/></EntityDef></Defs>"#,
        );
        let db = compile_def_database(&app, &ContentPlanRequest::default()).expect("compile");
        let id = db.entity_def_id_by_name("a").expect("id");
        let def = db.entity_def(id).expect("def");
        assert_eq!(def.renderable, RenderableKind::Sprite("player".to_string()));
    }

    #[test]
    fn renderable_kind_attribute_parses_placeholder() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>a</defName><label>A</label><renderable kind="Placeholder"/></EntityDef></Defs>"#,
        );
        let db = compile_def_database(&app, &ContentPlanRequest::default()).expect("compile");
        let id = db.entity_def_id_by_name("a").expect("id");
        let def = db.entity_def(id).expect("def");
        assert_eq!(def.renderable, RenderableKind::Placeholder);
    }

    #[test]
    fn renderable_kind_and_non_whitespace_text_is_error() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>a</defName><label>A</label><renderable kind="Sprite" spriteKey="player">Sprite:player</renderable></EntityDef></Defs>"#,
        );
        let err = compile_def_database(&app, &ContentPlanRequest::default()).expect_err("err");
        assert_eq!(err.code, ContentErrorCode::InvalidValue);
        assert!(err
            .message
            .contains("must not also include non-whitespace text"));
    }

    #[test]
    fn renderable_unknown_attribute_is_error() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>a</defName><label>A</label><renderable kind="Sprite" spriteKey="player" foo="bar"/></EntityDef></Defs>"#,
        );
        let err = compile_def_database(&app, &ContentPlanRequest::default()).expect_err("err");
        assert_eq!(err.code, ContentErrorCode::UnknownField);
        assert!(err.message.contains("unknown attribute"));
    }

    #[test]
    fn malformed_xml_reports_location() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>a</defName></Defs>"#,
        );
        let err = compile_def_database(&app, &ContentPlanRequest::default()).expect_err("err");
        assert_eq!(err.code, ContentErrorCode::XmlMalformed);
        assert!(err.location.is_some());
    }

    #[test]
    fn same_mod_duplicate_def_errors() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs>
                <EntityDef><defName>a</defName><label>A</label><renderable>Placeholder</renderable></EntityDef>
                <EntityDef><defName>a</defName><label>B</label><renderable>Placeholder</renderable></EntityDef>
            </Defs>"#,
        );
        let err = compile_def_database(&app, &ContentPlanRequest::default()).expect_err("err");
        assert_eq!(err.code, ContentErrorCode::DuplicateDefInMod);
    }

    #[test]
    fn cross_mod_duplicate_is_last_mod_wins_and_tags_preserve_when_omitted() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        fs::create_dir_all(app.mods_dir.join("moda")).expect("mkdir");
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>proto.player</defName><label>Base</label><renderable>Placeholder</renderable><moveSpeed>1.0</moveSpeed><tags><li>human</li></tags></EntityDef></Defs>"#,
        );
        write_file(
            &app.mods_dir.join("moda").join("defs.xml"),
            r#"<Defs><EntityDef><defName>proto.player</defName><label>Mod</label><moveSpeed>7.0</moveSpeed></EntityDef></Defs>"#,
        );
        let db = compile_def_database(
            &app,
            &ContentPlanRequest {
                enabled_mods: vec!["moda".to_string()],
                compiler_version: "dev".to_string(),
                game_version: "dev".to_string(),
            },
        )
        .expect("compile");
        let id = db.entity_def_id_by_name("proto.player").expect("id");
        let def = db.entity_def(id).expect("def");
        assert_eq!(def.label, "Mod");
        assert!((def.move_speed - 7.0).abs() < f32::EPSILON);
        assert_eq!(def.tags, vec!["human".to_string()]);
    }

    #[test]
    fn list_replace_overrides_previous_list() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        fs::create_dir_all(app.mods_dir.join("moda")).expect("mkdir");
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>proto.player</defName><label>Base</label><renderable>Placeholder</renderable><tags><li>one</li><li>two</li></tags></EntityDef></Defs>"#,
        );
        write_file(
            &app.mods_dir.join("moda").join("defs.xml"),
            r#"<Defs><EntityDef><defName>proto.player</defName><tags><li>replaced</li></tags></EntityDef></Defs>"#,
        );
        let db = compile_def_database(
            &app,
            &ContentPlanRequest {
                enabled_mods: vec!["moda".to_string()],
                compiler_version: "dev".to_string(),
                game_version: "dev".to_string(),
            },
        )
        .expect("compile");
        let id = db.entity_def_id_by_name("proto.player").expect("id");
        let def = db.entity_def(id).expect("def");
        assert_eq!(def.tags, vec!["replaced".to_string()]);
    }

    #[test]
    fn partial_override_without_target_fails_clearly() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>proto.ghost</defName><moveSpeed>3.0</moveSpeed></EntityDef></Defs>"#,
        );
        let err = compile_def_database(&app, &ContentPlanRequest::default()).expect_err("err");
        assert_eq!(err.code, ContentErrorCode::MissingOverrideTarget);
        assert!(err.message.contains("partial override"));
    }

    #[test]
    fn move_speed_defaults_to_five() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>a</defName><label>A</label><renderable>Placeholder</renderable></EntityDef></Defs>"#,
        );
        let db = compile_def_database(&app, &ContentPlanRequest::default()).expect("compile");
        let id = db.entity_def_id_by_name("a").expect("id");
        let def = db.entity_def(id).expect("def");
        assert!((def.move_speed - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn gameplay_fields_parse_as_optional_values_when_present() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>a</defName><label>A</label><renderable>Placeholder</renderable><health_max>200</health_max><base_damage>40</base_damage><aggro_radius>10.0</aggro_radius><attack_range>1.5</attack_range><attack_cooldown_seconds>0.25</attack_cooldown_seconds></EntityDef></Defs>"#,
        );
        let db = compile_def_database(&app, &ContentPlanRequest::default()).expect("compile");
        let id = db.entity_def_id_by_name("a").expect("id");
        let def = db.entity_def(id).expect("def");
        assert_eq!(def.health_max, Some(200));
        assert_eq!(def.base_damage, Some(40));
        assert_eq!(def.aggro_radius, Some(10.0));
        assert_eq!(def.attack_range, Some(1.5));
        assert_eq!(def.attack_cooldown_seconds, Some(0.25));
    }

    #[test]
    fn gameplay_fields_are_none_when_omitted() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>a</defName><label>A</label><renderable>Placeholder</renderable></EntityDef></Defs>"#,
        );
        let db = compile_def_database(&app, &ContentPlanRequest::default()).expect("compile");
        let id = db.entity_def_id_by_name("a").expect("id");
        let def = db.entity_def(id).expect("def");
        assert_eq!(def.health_max, None);
        assert_eq!(def.base_damage, None);
        assert_eq!(def.aggro_radius, None);
        assert_eq!(def.attack_range, None);
        assert_eq!(def.attack_cooldown_seconds, None);
    }

    #[test]
    fn gameplay_fields_validate_and_override_last_writer_wins() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        fs::create_dir_all(app.mods_dir.join("moda")).expect("mkdir");
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>proto.player</defName><label>Base</label><renderable>Placeholder</renderable><health_max>100</health_max><base_damage>25</base_damage><aggro_radius>6.0</aggro_radius><attack_range>0.9</attack_range><attack_cooldown_seconds>1.0</attack_cooldown_seconds></EntityDef></Defs>"#,
        );
        write_file(
            &app.mods_dir.join("moda").join("defs.xml"),
            r#"<Defs><EntityDef><defName>proto.player</defName><health_max>300</health_max><base_damage>60</base_damage><aggro_radius>12.0</aggro_radius><attack_range>2.5</attack_range><attack_cooldown_seconds>0.2</attack_cooldown_seconds></EntityDef></Defs>"#,
        );
        let db = compile_def_database(
            &app,
            &ContentPlanRequest {
                enabled_mods: vec!["moda".to_string()],
                compiler_version: "dev".to_string(),
                game_version: "dev".to_string(),
            },
        )
        .expect("compile");
        let id = db.entity_def_id_by_name("proto.player").expect("id");
        let def = db.entity_def(id).expect("def");
        assert_eq!(def.health_max, Some(300));
        assert_eq!(def.base_damage, Some(60));
        assert_eq!(def.aggro_radius, Some(12.0));
        assert_eq!(def.attack_range, Some(2.5));
        assert_eq!(def.attack_cooldown_seconds, Some(0.2));

        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>a</defName><label>A</label><renderable>Placeholder</renderable><health_max>0</health_max></EntityDef></Defs>"#,
        );
        let err = compile_def_database(&app, &ContentPlanRequest::default()).expect_err("error");
        assert_eq!(err.code, ContentErrorCode::InvalidValue);
        assert_eq!(err.mod_id, "base");
        assert_eq!(err.def_name.as_deref(), Some("a"));
        assert_eq!(err.field_name.as_deref(), Some("health_max"));
    }

    #[test]
    fn gameplay_non_negative_fields_reject_negative_values_with_context() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());

        for (field_name, value) in [
            ("aggro_radius", "-1.0"),
            ("attack_range", "-0.5"),
            ("attack_cooldown_seconds", "-0.25"),
        ] {
            write_file(
                &app.base_content_dir.join("defs.xml"),
                &format!(
                    "<Defs><EntityDef><defName>proto.bad</defName><label>Bad</label><renderable>Placeholder</renderable><{}>{}</{}></EntityDef></Defs>",
                    field_name, value, field_name
                ),
            );
            let err = compile_def_database(&app, &ContentPlanRequest::default()).expect_err("err");
            assert_eq!(err.code, ContentErrorCode::InvalidValue);
            assert_eq!(err.mod_id, "base");
            assert_eq!(err.def_name.as_deref(), Some("proto.bad"));
            assert_eq!(err.field_name.as_deref(), Some(field_name));
        }
    }

    #[test]
    fn gameplay_fields_allow_zero_when_runtime_logic_supports_it() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>proto.zero_ok</defName><label>ZeroOk</label><renderable>Placeholder</renderable><base_damage>0</base_damage><attack_cooldown_seconds>0</attack_cooldown_seconds></EntityDef></Defs>"#,
        );
        let db = compile_def_database(&app, &ContentPlanRequest::default()).expect("compile");
        let id = db.entity_def_id_by_name("proto.zero_ok").expect("id");
        let def = db.entity_def(id).expect("def");
        assert_eq!(def.base_damage, Some(0));
        assert_eq!(def.attack_cooldown_seconds, Some(0.0));
    }

    #[test]
    fn interactable_tags_compile_and_preserve_values() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs>
                <EntityDef><defName>proto.player</defName><label>Player</label><renderable>Placeholder</renderable></EntityDef>
                <EntityDef><defName>proto.resource_pile</defName><label>ResourcePile</label><renderable>Placeholder</renderable><tags><li>interactable</li><li>resource_pile</li></tags></EntityDef>
            </Defs>"#,
        );
        let db = compile_def_database(&app, &ContentPlanRequest::default()).expect("compile");
        let id = db.entity_def_id_by_name("proto.resource_pile").expect("id");
        let def = db.entity_def(id).expect("def");
        assert_eq!(
            def.tags,
            vec!["interactable".to_string(), "resource_pile".to_string()]
        );
    }

    #[test]
    fn fixture_valid_case_compiles() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        copy_dir_recursive(
            &fixture_root("pass_01_base_only").join("base"),
            &app.base_content_dir,
        );
        let db = compile_def_database(&app, &ContentPlanRequest::default()).expect("compile");
        let id = db.entity_def_id_by_name("proto.player").expect("id");
        let def = db.entity_def(id).expect("def");
        assert_eq!(
            def.tags,
            vec!["colonist".to_string(), "starter".to_string()]
        );
    }

    #[test]
    fn fixture_missing_defname_fails() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        fs::create_dir_all(app.mods_dir.join("missingdefname")).expect("mkdir");
        copy_dir_recursive(
            &fixture_root("fail_01_missing_defname").join("missingdefname"),
            &app.mods_dir.join("missingdefname"),
        );
        let err = compile_def_database(
            &app,
            &ContentPlanRequest {
                enabled_mods: vec!["missingdefname".to_string()],
                compiler_version: "dev".to_string(),
                game_version: "dev".to_string(),
            },
        )
        .expect_err("error");
        assert_eq!(err.code, ContentErrorCode::MissingField);
        assert_eq!(err.mod_id, "missingdefname");
    }

    #[test]
    fn fixture_unknown_field_fails() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        fs::create_dir_all(app.mods_dir.join("unknownfield")).expect("mkdir");
        copy_dir_recursive(
            &fixture_root("fail_02_unknown_field").join("unknownfield"),
            &app.mods_dir.join("unknownfield"),
        );
        let err = compile_def_database(
            &app,
            &ContentPlanRequest {
                enabled_mods: vec!["unknownfield".to_string()],
                compiler_version: "dev".to_string(),
                game_version: "dev".to_string(),
            },
        )
        .expect_err("error");
        assert_eq!(err.code, ContentErrorCode::UnknownField);
        assert_eq!(err.mod_id, "unknownfield");
    }

    #[test]
    fn fixture_renderable_attr_sprite_compiles() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        copy_dir_recursive(
            &fixture_root("pass_04_renderable_attr").join("base"),
            &app.base_content_dir,
        );
        let db = compile_def_database(&app, &ContentPlanRequest::default()).expect("compile");
        let id = db.entity_def_id_by_name("proto.worker_attr").expect("id");
        let def = db.entity_def(id).expect("def");
        assert_eq!(def.renderable, RenderableKind::Sprite("player".to_string()));
    }

    #[test]
    fn fixture_renderable_attr_bad_key_fails() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        fs::create_dir_all(app.mods_dir.join("badattr")).expect("mkdir");
        copy_dir_recursive(
            &fixture_root("fail_07_renderable_attr_bad_key").join("badattr"),
            &app.mods_dir.join("badattr"),
        );
        let err = compile_def_database(
            &app,
            &ContentPlanRequest {
                enabled_mods: vec!["badattr".to_string()],
                compiler_version: "dev".to_string(),
                game_version: "dev".to_string(),
            },
        )
        .expect_err("error");
        assert_eq!(err.code, ContentErrorCode::InvalidValue);
        assert_eq!(err.mod_id, "badattr");
    }
}
