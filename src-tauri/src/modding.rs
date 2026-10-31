//! Thunderstore modding: shared domain types and pure helpers.

pub(crate) mod archive_cache;
pub(crate) mod catalog_commands;
pub(crate) mod config_files;
pub(crate) mod dependencies;
pub(crate) mod ecosystem;
pub(crate) mod ecosystem_store;
pub(crate) mod exchange_commands;
pub(crate) mod install_commands;
pub(crate) mod install_rules;
pub(crate) mod installer;
pub(crate) mod launch;
pub(crate) mod loader_install;
pub(crate) mod local_import;
pub(crate) mod package_cache;
pub(crate) mod package_id;
pub(crate) mod package_manifest;
pub(crate) mod paths;
pub(crate) mod profile_codes;
pub(crate) mod profile_commands;
pub(crate) mod profile_exchange;
pub(crate) mod profiles;
#[cfg(test)]
mod test_support;
pub(crate) mod thunderstore_http;
mod wine_registry;
