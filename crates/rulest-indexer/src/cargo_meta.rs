use std::collections::HashSet;
use std::path::Path;

use cargo_metadata::MetadataCommand;
use rulest_core::models::Crate;

/// Information extracted from `cargo metadata`.
pub struct WorkspaceInfo {
    pub crates: Vec<Crate>,
    pub modules: Vec<(String, Vec<ModuleInfo>)>, // (crate_name, modules)
    /// Cross-crate dependencies within the workspace: `(from_crate_name, to_crate_name)` pairs.
    pub dependencies: Vec<(String, String)>,
}

pub struct ModuleInfo {
    pub path: String,
    pub name: String,
}

/// Run `cargo metadata` and extract workspace members.
pub fn extract_workspace(workspace_root: &Path) -> Result<WorkspaceInfo, String> {
    let manifest_path = if workspace_root.is_file() {
        workspace_root.to_path_buf()
    } else {
        workspace_root.join("Cargo.toml")
    };

    let metadata = MetadataCommand::new()
        .manifest_path(&manifest_path)
        .exec()
        .map_err(|e| format!("Failed to run cargo metadata: {}", e))?;

    let workspace_members: HashSet<_> = metadata.workspace_members.iter().collect();

    // Collect workspace member package names for dependency filtering
    let workspace_package_names: HashSet<String> = metadata
        .packages
        .iter()
        .filter(|p| workspace_members.contains(&p.id))
        .map(|p| p.name.clone())
        .collect();

    let mut crates = Vec::new();
    let mut modules = Vec::new();
    let mut dependencies = Vec::new();

    for package in &metadata.packages {
        if !workspace_members.contains(&package.id) {
            continue;
        }

        let pkg_path = package
            .manifest_path
            .parent()
            .map(|p| p.as_std_path().to_path_buf())
            .unwrap_or_default();

        let relative_path = pathdiff_from(&pkg_path, workspace_root.parent().unwrap_or(workspace_root));

        crates.push(Crate {
            id: None,
            name: package.name.clone(),
            path: relative_path.clone(),
            description: package.description.clone(),
            bounded_context: None,
        });

        // Find all .rs files in the crate's src directory
        let src_dir = pkg_path.join("src");
        let mut module_infos = Vec::new();

        if src_dir.exists() {
            collect_rs_files(&src_dir, &relative_path, &mut module_infos);
        }

        modules.push((package.name.clone(), module_infos));

        // Extract workspace-internal dependencies
        for dep in &package.dependencies {
            if workspace_package_names.contains(&dep.name) {
                dependencies.push((package.name.clone(), dep.name.clone()));
            }
        }
    }

    Ok(WorkspaceInfo {
        crates,
        modules,
        dependencies,
    })
}

fn collect_rs_files(dir: &Path, base_path: &str, modules: &mut Vec<ModuleInfo>) {
    let walker = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok());

    for entry in walker {
        if entry.file_type().is_file()
            && entry.path().extension().is_some_and(|ext| ext == "rs")
        {
            // Derive module name from file path
            let name = entry
                .path()
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            // Store the path relative to workspace root
            let relative = format!(
                "{}/src/{}",
                base_path,
                entry
                    .path()
                    .strip_prefix(dir)
                    .unwrap_or(entry.path())
                    .to_string_lossy()
            );

            modules.push(ModuleInfo {
                path: relative,
                name,
            });
        }
    }
}

fn pathdiff_from(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}
