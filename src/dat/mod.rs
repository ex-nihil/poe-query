use std::collections::{HashMap, HashSet};
use std::path::Path;

use log::info;
use poe_bundle::{BundleReader, BundleReaderRead};

use crate::dat::file::DatFile;
use crate::dat::specification::{EnumSpec, FileSpec};
use crate::error::QueryError;

pub mod util;
pub mod specification;
pub mod file;


/// Load the table and enum specifications from a dat-schema directory.
/// File specs are resolved in two passes so @ref columns can look up the
/// field types of the tables they point at.
pub fn load_schema(spec_path: &Path) -> (HashMap<String, FileSpec>, HashMap<String, EnumSpec>) {
    let enums = FileSpec::read_enum_specs(spec_path);
    let specs = FileSpec::read_file_specs(spec_path, &enums, &HashMap::new());
    let specs = FileSpec::read_file_specs(spec_path, &enums, &specs);
    (specs, enums)
}

pub struct DatReader<'a> {
    language: &'a str,
    bundle_reader: &'a BundleReader,
    specs: HashMap<String, FileSpec>,
    enums: HashMap<String, EnumSpec>,
}

impl<'a> DatReader<'a> {

    pub fn from_install(language: &'a str, bundles: &'a BundleReader, spec_path: &Path) -> DatReader<'a> {
        let (specs, enums) = load_schema(spec_path);

        DatReader {
            language,
            bundle_reader: bundles,
            specs,
            enums
        }
    }

    fn get_filepath(&self, filename: &str) -> String {
        let name = filename.to_lowercase();
        if self.language == "English" {
            return format!("data/{}.datc64", name)
        }
        format!("data/{}/{}.datc64", self.language.to_lowercase(), name)
    }
}

pub trait DatStoreImpl<'a> {
    fn file_by_filename(&self, filename: &str) -> Result<DatFile, QueryError>;
    fn spec(&self, path: &str) -> Option<&FileSpec>;
    fn spec_by_export(&self, export: &str) -> Option<&FileSpec>;
    fn exports(&self) -> HashSet<&str>;
    fn enum_name(&self, path: &str) -> Option<&EnumSpec>;
}

impl<'a> DatStoreImpl<'a> for DatReader<'a> {
    fn file_by_filename(&self, filename: &str) -> Result<DatFile, QueryError> {
        let path = self.get_filepath(filename);
        let spec = self.spec(filename);
        info!("Unpacking {}", path);
        let bytes = self.bundle_reader.bytes(&path)
            .map_err(|_| QueryError::MissingDataFile { table: filename.to_string() })?;

        let dat_file = DatFile::from_bytes(path, bytes)?;
        if let Some(file_specification) = spec {
            dat_file.valid(file_specification);
        }
        Ok(dat_file)
    }

    fn spec(&self, path: &str) -> Option<&FileSpec> {
        self.specs.get(path)
    }

    fn spec_by_export(&self, export: &str) -> Option<&FileSpec> {
        self.specs.values().find(|s| s.file_name == export)
    }

    fn exports(&self) -> HashSet<&str> {
        self.specs.iter().map(|(_, s)| s.file_name.as_str()).collect()
    }

    fn enum_name(&self, path: &str) -> Option<&EnumSpec> {
        self.enums.get(path)
    }
}