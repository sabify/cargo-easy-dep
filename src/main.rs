#![doc = include_str!("../README.md")]

use cargo_metadata::{Dependency, Metadata, MetadataCommand, camino::Utf8PathBuf};
use clap::{ArgAction, Args, Parser};
use colored::Colorize;
use std::{
    collections::HashMap,
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};
use toml_edit::{self, DocumentMut};

// See also `clap_cargo::style::CLAP_STYLING`
const CLAP_STYLING: clap::builder::styling::Styles = clap::builder::styling::Styles::styled()
    .header(clap_cargo::style::HEADER)
    .usage(clap_cargo::style::USAGE)
    .literal(clap_cargo::style::LITERAL)
    .placeholder(clap_cargo::style::PLACEHOLDER)
    .error(clap_cargo::style::ERROR)
    .valid(clap_cargo::style::VALID)
    .invalid(clap_cargo::style::INVALID);

#[derive(Parser)]
#[command(name = "cargo")]
#[command(bin_name = "cargo")]
#[command(styles = CLAP_STYLING)]
enum CargoCli {
    EasyDep(Cli),
}

#[derive(Args)]
#[command(about, version)]
struct Cli {
    /// Minimum number of occurrences to consider a dependency common
    #[clap(
        short,
        long,
        default_value = "2",
        env = "CARGO_EASY_DEP_MIN_OCCURRENCES",
        value_parser = clap::value_parser!(u32).range(1..)
    )]
    min_occurrences: u32,

    /// Path to workspace root (defaults to current directory)
    #[clap(short, long, env = "CARGO_EASY_DEP_WORKSPACE_ROOT")]
    workspace_root: Option<PathBuf>,

    /// Suppress all output
    #[clap(
        short,
        long,
        action = ArgAction::SetTrue,
        env = "CARGO_EASY_DEP_QUIET"
    )]
    quiet: bool,
}

#[derive(Debug)]
enum AppError {
    Metadata(String),
    Io(std::io::Error, PathBuf),
    TomlParse(toml_edit::TomlError, PathBuf),
    WorkspaceUpdate(String),
    MemberUpdate(String, Utf8PathBuf),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Metadata(msg) => write!(f, "Failed to retrieve cargo metadata: {}", msg),
            AppError::Io(err, path) => write!(f, "IO error at '{}': {}", path.display(), err),
            AppError::TomlParse(err, path) => {
                write!(f, "TOML parse error in '{}': {}", path.display(), err)
            }
            AppError::WorkspaceUpdate(msg) => {
                write!(f, "Failed to update workspace Cargo.toml: {}", msg)
            }
            AppError::MemberUpdate(msg, path) => write!(
                f,
                "Failed to update member Cargo.toml at '{}': {}",
                path, msg
            ),
        }
    }
}

impl Error for AppError {}

impl From<cargo_metadata::Error> for AppError {
    fn from(err: cargo_metadata::Error) -> Self {
        AppError::Metadata(err.to_string())
    }
}

fn io_err(err: std::io::Error, path: impl Into<PathBuf>) -> AppError {
    AppError::Io(err, path.into())
}

fn toml_err(err: toml_edit::TomlError, path: impl Into<PathBuf>) -> AppError {
    AppError::TomlParse(err, path.into())
}

type AppResult<T> = Result<T, AppError>;

fn main() -> Result<(), Box<dyn Error>> {
    let CargoCli::EasyDep(cli) = CargoCli::parse();

    match run(&cli) {
        Ok(_) => {
            if !cli.quiet {
                println!(
                    "{}",
                    "Successfully updated all Cargo.toml files with workspace dependencies."
                        .green(),
                );
            }
            Ok(())
        }
        Err(e) => {
            if !cli.quiet {
                eprintln!("{}: {}", "Error".red(), e.to_string().red());
            }
            Err(e.into())
        }
    }
}

fn run(cli: &Cli) -> AppResult<()> {
    let workspace_path = cli
        .workspace_root
        .as_deref()
        .unwrap_or_else(|| Path::new("."));

    // Get cargo metadata
    if !cli.quiet {
        println!("{}", "Analyzing workspace...".yellow());
    }
    let metadata = MetadataCommand::new()
        .current_dir(workspace_path)
        .no_deps()
        .exec()
        .map_err(|e| AppError::Metadata(format!("Failed to get metadata: {}", e)))?;

    if !cli.quiet {
        println!(
            "{} {} {}",
            "Detecting common dependencies across".yellow(),
            metadata.workspace_members.len().to_string().yellow().bold(),
            "workspace members...".yellow(),
        );
    }

    // Collect dependencies used more than the minimum occurrences
    let common_deps = find_common_dependencies(&metadata, cli.min_occurrences, cli.quiet)?;
    if common_deps.is_empty() {
        if !cli.quiet {
            println!(
                "{}",
                "No common dependencies found across workspace members.".yellow()
            );
        }
        return Ok(());
    }

    // Update the root Cargo.toml
    if !cli.quiet {
        println!("{}", "Updating root Cargo.toml...".yellow());
    }
    update_root_cargo_toml(&metadata, &common_deps, cli.quiet)?;

    // Update all member Cargo.toml files
    if !cli.quiet {
        println!("{}", "Updating member Cargo.toml files...".yellow());
    }
    let mut updated_count = 0;
    for package in metadata.workspace_members.iter() {
        let pkg = metadata
            .packages
            .iter()
            .find(|p| p.id == *package)
            .ok_or_else(|| AppError::Metadata(format!("Package not found for ID: {}", package)))?;

        let modified = update_member_cargo_toml(&pkg.manifest_path, &common_deps, cli.quiet)?;
        if modified {
            updated_count += 1;
        }
    }

    if !cli.quiet && updated_count > 0 {
        println!(
            "{} {} {}",
            "Updated".green(),
            updated_count.to_string().green().bold(),
            "member Cargo.toml files".green()
        );
    }
    Ok(())
}

fn find_common_dependencies(
    metadata: &Metadata,
    min_occurrences: u32,
    quiet: bool,
) -> AppResult<HashMap<String, Dependency>> {
    let mut dep_count: HashMap<String, usize> = HashMap::new();
    let mut dep_info: HashMap<String, Dependency> = HashMap::new();

    // Count occurrences of each dependency and collect their info
    for package_id in &metadata.workspace_members {
        let package = metadata
            .packages
            .iter()
            .find(|p| p.id == *package_id)
            .ok_or_else(|| {
                AppError::Metadata(format!("Package not found for ID: {}", package_id))
            })?;

        for dep in package.dependencies.iter() {
            if dep.path.is_some() {
                continue;
            }
            let count = dep_count.entry(dep.name.clone()).or_insert(0);
            *count += 1;
            if *count >= min_occurrences as usize {
                // The first version occurrence will be used.
                dep_info
                    .entry(dep.name.clone())
                    .or_insert_with(|| dep.clone());
            }
        }
    }

    if !quiet && !dep_info.is_empty() {
        println!("Found {} common dependencies:", dep_info.len());
        for (name, info) in &dep_info {
            println!("  - {} = {}", name, info.req);
        }
    }

    Ok(dep_info)
}

fn update_root_cargo_toml(
    metadata: &Metadata,
    common_deps: &HashMap<String, Dependency>,
    quiet: bool,
) -> AppResult<bool> {
    let root_manifest_path = metadata.workspace_root.join("Cargo.toml");
    let content =
        fs::read_to_string(&root_manifest_path).map_err(|e| io_err(e, &root_manifest_path))?;

    let mut doc = content
        .parse::<DocumentMut>()
        .map_err(|e| toml_err(e, &root_manifest_path))?;

    // Ensure the workspace section exists
    if !doc.contains_key("workspace") {
        doc["workspace"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    // Create or get the workspace.dependencies table
    if !doc["workspace"]
        .as_table()
        .ok_or_else(|| AppError::WorkspaceUpdate("'workspace' is not a table".to_string()))?
        .contains_key("dependencies")
    {
        doc["workspace"]["dependencies"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    let mut modified = false;

    // Add each common dependency to workspace.dependencies
    for (name, info) in common_deps {
        let deps_table = doc["workspace"]["dependencies"]
            .as_table_mut()
            .ok_or_else(|| {
                AppError::WorkspaceUpdate("'workspace.dependencies' is not a table".to_string())
            })?;

        // Simple version string
        deps_table.entry(name).or_insert_with(|| {
            modified = true;
            toml_edit::value(info.req.to_string())
        });
    }

    fs::write(&root_manifest_path, doc.to_string()).map_err(|e| io_err(e, &root_manifest_path))?;

    if !quiet {
        if modified {
            println!(
                "{} {} {}",
                "Updated root Cargo.toml with".green(),
                common_deps.len().to_string().green().bold(),
                "common dependencies".green(),
            );
        } else {
            println!("{}", "No changes needed for root Cargo.toml".green());
        }
    }

    Ok(modified)
}

fn update_member_cargo_toml(
    manifest_path: &Utf8PathBuf,
    common_deps: &HashMap<String, Dependency>,
    quiet: bool,
) -> AppResult<bool> {
    let content = fs::read_to_string(manifest_path).map_err(|e| io_err(e, manifest_path))?;

    let mut doc = content
        .parse::<DocumentMut>()
        .map_err(|e| toml_err(e, manifest_path))?;

    let mut modified = false;

    // Update regular dependencies
    if let Some(deps) = doc.get_mut("dependencies") {
        if let Some(deps_table) = deps.as_table_mut() {
            modified |= update_dependencies_table(deps_table, common_deps)?;
        } else {
            return Err(AppError::MemberUpdate(
                "'dependencies' is not a table".to_string(),
                manifest_path.to_path_buf(),
            ));
        }
    }

    // Update dev-dependencies
    if let Some(deps) = doc.get_mut("dev-dependencies") {
        if let Some(deps_table) = deps.as_table_mut() {
            modified |= update_dependencies_table(deps_table, common_deps)?;
        } else {
            return Err(AppError::MemberUpdate(
                "'dev-dependencies' is not a table".to_string(),
                manifest_path.to_path_buf(),
            ));
        }
    }

    // Update build-dependencies
    if let Some(deps) = doc.get_mut("build-dependencies") {
        if let Some(deps_table) = deps.as_table_mut() {
            modified |= update_dependencies_table(deps_table, common_deps)?;
        } else {
            return Err(AppError::MemberUpdate(
                "'build-dependencies' is not a table".to_string(),
                manifest_path.to_path_buf(),
            ));
        }
    }

    if modified {
        fs::write(manifest_path, doc.to_string()).map_err(|e| io_err(e, manifest_path))?;
        if !quiet {
            println!("  - Updated member at: {}", manifest_path);
        }
    } else if !quiet {
        println!("  - No changes needed for: {}", manifest_path);
    }

    Ok(modified)
}

fn update_dependencies_table(
    deps_table: &mut toml_edit::Table,
    common_deps: &HashMap<String, Dependency>,
) -> AppResult<bool> {
    let mut modified = false;

    for name in common_deps.keys() {
        if deps_table.contains_key(name) {
            match &mut deps_table[name] {
                toml_edit::Item::Value(toml_edit::Value::String(_)) => {
                    // Replace with workspace = true
                    let mut dep_table = toml_edit::Table::new();
                    dep_table.set_implicit(true);
                    dep_table["workspace"] = toml_edit::value(true);
                    deps_table[name] = dep_table.into_inline_table().into();
                    modified = true;
                }
                toml_edit::Item::Value(toml_edit::Value::InlineTable(table)) => {
                    // Keep existing configuration but add workspace = true
                    // Remove the version field if it exists
                    if table.contains_key("version") {
                        table.remove("version");
                    }
                    // Add workspace = true
                    let entry = table.entry("workspace").or_insert_with(|| {
                        modified = true;
                        toml_edit::Value::Boolean(toml_edit::Formatted::new(true))
                    });

                    if let Some(is_workspace) = entry.as_bool() {
                        if !is_workspace {
                            *entry = toml_edit::Value::Boolean(toml_edit::Formatted::new(true));
                            modified = true;
                        }
                    }
                }
                toml_edit::Item::Table(table) => {
                    // Keep existing configuration but add workspace = true
                    // Remove the version field if it exists
                    if table.contains_key("version") {
                        table.remove("version");
                    }
                    // Add workspace = true
                    let entry = table.entry("workspace").or_insert_with(|| {
                        modified = true;
                        toml_edit::value(true)
                    });

                    if let Some(is_workspace) = entry.as_bool() {
                        if !is_workspace {
                            *entry = toml_edit::value(true);
                            modified = true;
                        }
                    }
                }

                toml_edit::Item::ArrayOfTables(tables) => {
                    for table in tables.iter_mut() {
                        modified |= update_dependencies_table(table, common_deps)?;
                    }
                }
                _ => {}
            }
        }
    }

    Ok(modified)
}
