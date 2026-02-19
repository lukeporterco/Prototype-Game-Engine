use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use roxmltree::{Document, Node};

use crate::app::RenderableKind;
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
}

#[derive(Debug, Clone)]
pub struct ContentCompileError {
    pub code: ContentErrorCode,
    pub message: String,
    pub mod_id: String,
    pub file_path: PathBuf,
    pub location: Option<SourceLocation>,
}

impl fmt::Display for ContentCompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.location {
            Some(loc) => write!(
                f,
                "{:?}: {} (mod={}, file={}, line={}, column={})",
                self.code,
                self.message,
                self.mod_id,
                self.file_path.display(),
                loc.line,
                loc.column
            ),
            None => write!(
                f,
                "{:?}: {} (mod={}, file={})",
                self.code,
                self.message,
                self.mod_id,
                self.file_path.display()
            ),
        }
    }
}

impl std::error::Error for ContentCompileError {}

#[derive(Debug, Clone)]
struct PendingEntityDef {
    def_name: String,
    label: String,
    renderable: RenderableKind,
    move_speed: f32,
}

pub fn compile_def_database(
    app_paths: &AppPaths,
    request: &ContentPlanRequest,
) -> Result<DefDatabase, ContentCompileError> {
    let sources = discover_mod_sources(app_paths, request)
        .map_err(|error| map_discovery_error(error, &app_paths.root))?;

    let mut merged = BTreeMap::<String, PendingEntityDef>::new();

    for source in sources {
        let xml_files = collect_xml_files_sorted(&source.source_dir)
            .map_err(|error| read_error(&source.mod_id, error.path, error.source))?;
        let mut seen_in_mod = HashSet::<String>::new();

        for xml_file in xml_files {
            let raw = fs::read_to_string(&xml_file)
                .map_err(|source_err| read_error(&source.mod_id, xml_file.clone(), source_err))?;
            let defs = parse_defs_document(&source.mod_id, &xml_file, &raw)?;
            for def in defs {
                if !seen_in_mod.insert(def.def_name.clone()) {
                    return Err(ContentCompileError {
                        code: ContentErrorCode::DuplicateDefInMod,
                        message: format!(
                            "duplicate EntityDef '{}' in mod '{}'; each mod may define a defName only once",
                            def.def_name, source.mod_id
                        ),
                        mod_id: source.mod_id.clone(),
                        file_path: xml_file.clone(),
                        location: None,
                    });
                }
                // Cross-mod duplicates are intentional override points (last mod wins).
                merged.insert(def.def_name.clone(), def);
            }
        }
    }

    let entity_defs = merged
        .into_values()
        .map(|def| EntityArchetype {
            id: EntityDefId(0),
            def_name: def.def_name,
            label: def.label,
            renderable: def.renderable,
            move_speed: def.move_speed,
        })
        .collect::<Vec<_>>();

    Ok(DefDatabase::from_entity_defs(entity_defs))
}

fn parse_defs_document(
    mod_id: &str,
    file_path: &Path,
    raw: &str,
) -> Result<Vec<PendingEntityDef>, ContentCompileError> {
    let doc = Document::parse(raw).map_err(|error| ContentCompileError {
        code: ContentErrorCode::XmlMalformed,
        message: format!("malformed XML: {error}"),
        mod_id: mod_id.to_string(),
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

    let mut defs = Vec::<PendingEntityDef>::new();
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
) -> Result<PendingEntityDef, ContentCompileError> {
    let mut seen_fields = HashSet::<String>::new();
    let mut def_name: Option<String> = None;
    let mut label: Option<String> = None;
    let mut renderable: Option<RenderableKind> = None;
    let mut move_speed: Option<f32> = None;

    for field in node.children().filter(|child| child.is_element()) {
        let field_name = field.tag_name().name().to_string();
        if !seen_fields.insert(field_name.clone()) {
            return Err(error_at_node(
                ContentErrorCode::DuplicateField,
                format!("duplicate field <{}> in <EntityDef>", field_name),
                mod_id,
                file_path,
                doc,
                field,
            ));
        }

        match field_name.as_str() {
            "defName" => {
                def_name = Some(required_text(mod_id, file_path, doc, field, "defName")?);
            }
            "label" => {
                label = Some(required_text(mod_id, file_path, doc, field, "label")?);
            }
            "renderable" => {
                let value = required_text(mod_id, file_path, doc, field, "renderable")?;
                let parsed = match value.as_str() {
                    "Placeholder" => RenderableKind::Placeholder,
                    _ => {
                        return Err(error_at_node(
                            ContentErrorCode::InvalidValue,
                            format!(
                                "invalid renderable '{}'; allowed values: Placeholder",
                                value
                            ),
                            mod_id,
                            file_path,
                            doc,
                            field,
                        ))
                    }
                };
                renderable = Some(parsed);
            }
            "moveSpeed" => {
                let value = required_text(mod_id, file_path, doc, field, "moveSpeed")?;
                let parsed = value.parse::<f32>().map_err(|_| {
                    error_at_node(
                        ContentErrorCode::InvalidValue,
                        format!("moveSpeed '{}' is not a valid number", value),
                        mod_id,
                        file_path,
                        doc,
                        field,
                    )
                })?;
                if !parsed.is_finite() || parsed < 0.0 {
                    return Err(error_at_node(
                        ContentErrorCode::InvalidValue,
                        "moveSpeed must be finite and >= 0".to_string(),
                        mod_id,
                        file_path,
                        doc,
                        field,
                    ));
                }
                move_speed = Some(parsed);
            }
            // Accepted for compatibility with existing Ticket 6 fixtures.
            "tags" => {}
            _ => {
                return Err(error_at_node(
                    ContentErrorCode::UnknownField,
                    format!("unknown field <{}> in <EntityDef>", field_name),
                    mod_id,
                    file_path,
                    doc,
                    field,
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
    let Some(label) = label else {
        return Err(error_at_node(
            ContentErrorCode::MissingField,
            "missing required field <label> in <EntityDef>".to_string(),
            mod_id,
            file_path,
            doc,
            node,
        ));
    };
    let Some(renderable) = renderable else {
        return Err(error_at_node(
            ContentErrorCode::MissingField,
            "missing required field <renderable> in <EntityDef>".to_string(),
            mod_id,
            file_path,
            doc,
            node,
        ));
    };

    Ok(PendingEntityDef {
        def_name,
        label,
        renderable,
        move_speed: move_speed.unwrap_or(5.0),
    })
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

fn error_at_node(
    code: ContentErrorCode,
    message: String,
    mod_id: &str,
    file_path: &Path,
    doc: &Document<'_>,
    node: Node<'_, '_>,
) -> ContentCompileError {
    let pos = doc.text_pos_at(node.range().start);
    ContentCompileError {
        code,
        message,
        mod_id: mod_id.to_string(),
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
            file_path: expected_dir,
            location: None,
        },
        other => ContentCompileError {
            code: ContentErrorCode::Discovery,
            message: other.to_string(),
            mod_id: "<discovery>".to_string(),
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
    fn cross_mod_duplicate_is_last_mod_wins() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        fs::create_dir_all(app.mods_dir.join("moda")).expect("mkdir");
        write_file(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>proto.player</defName><label>Base</label><renderable>Placeholder</renderable><moveSpeed>1.0</moveSpeed></EntityDef></Defs>"#,
        );
        write_file(
            &app.mods_dir.join("moda").join("defs.xml"),
            r#"<Defs><EntityDef><defName>proto.player</defName><label>Mod</label><renderable>Placeholder</renderable><moveSpeed>7.0</moveSpeed></EntityDef></Defs>"#,
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
    fn fixture_valid_case_compiles() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        copy_dir_recursive(
            &fixture_root("pass_01_base_only").join("base"),
            &app.base_content_dir,
        );
        let db = compile_def_database(&app, &ContentPlanRequest::default()).expect("compile");
        assert!(db.entity_def_id_by_name("proto.player").is_some());
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
}
