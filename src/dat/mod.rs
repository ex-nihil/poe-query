use std::collections::{HashMap, HashSet};
use std::path::Path;

use log::{info, warn};
use poe_bundle::{BundleReader, BundleReaderRead};

use crate::dat::file::DatFile;
use crate::dat::specification::{EnumSpec, FileSpec};
use crate::error::QueryError;
use crate::translate::StatDescriptions;

pub mod util;
pub mod specification;
pub mod file;


/// Load the table and enum specifications from a dat-schema directory.
/// File specs are resolved in two passes so @ref columns can look up the
/// field types of the tables they point at.
pub fn load_schema(spec_path: &Path) -> (HashMap<String, FileSpec>, HashMap<String, EnumSpec>) {
    let definitions = FileSpec::parse_definitions(spec_path);
    let enums = FileSpec::read_enum_specs(&definitions);
    let specs = FileSpec::read_file_specs(&definitions, &enums, &HashMap::new());
    let specs = FileSpec::read_file_specs(&definitions, &enums, &specs);
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

    pub fn specs(&self) -> &HashMap<String, FileSpec> {
        &self.specs
    }

    pub fn language(&self) -> &str {
        self.language
    }

    /// Read a UTF-16 text file (e.g. stat descriptions) from the bundles.
    pub fn read_text_file(&self, path: &str) -> Result<String, QueryError> {
        // bytes() panics on paths missing from the index, so gate the read
        if self.bundle_reader.size_of(path).is_none() {
            return Err(QueryError::MissingDataFile { table: path.to_string() });
        }
        let bytes = self.bundle_reader.bytes(path)
            .map_err(|error| QueryError::internal(format!("failed to read '{}': {}", path, error)))?;
        Ok(decode_utf16(&bytes))
    }

    /// Load and parse a stat description file, following include directives.
    /// `file` is the base name, default "stat_descriptions".
    pub fn stat_descriptions(&self, file: Option<&str>) -> Result<StatDescriptions, QueryError> {
        let name = file.unwrap_or("stat_descriptions");
        let name = name.strip_suffix(".txt").or(name.strip_suffix(".csd")).unwrap_or(name);

        let mut path = format!("metadata/statdescriptions/{}.csd", name.to_lowercase());
        if self.bundle_reader.size_of(&path).is_none() {
            path = format!("metadata/statdescriptions/{}.txt", name.to_lowercase());
        }

        let text = self.read_text_file(&path)?;
        StatDescriptions::parse_with(&text, &mut |include: &str| {
            self.read_text_file(&include.to_lowercase())
        })
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
        // bytes() panics on paths missing from the index, so gate the read
        if self.bundle_reader.size_of(&path).is_none() {
            return Err(QueryError::MissingDataFile { table: filename.to_string() });
        }
        let bytes = self.bundle_reader.bytes(&path)
            .map_err(|_| QueryError::MissingDataFile { table: filename.to_string() })?;

        let mut dat_file = DatFile::from_bytes(path, bytes)?;
        if let Some(file_specification) = spec {
            dat_file.warnings = dat_file.validate(file_specification)?;
            dat_file.warnings.iter().for_each(|warning| warn!("{}", warning));
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

/// Decode game text files: UTF-16LE with BOM, BOM-less UTF-16LE heuristic,
/// or plain UTF-8 fallback.
fn decode_utf16(bytes: &[u8]) -> String {
    let has_bom = bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE;
    let looks_utf16 = bytes.len() >= 2 && bytes[1] == 0;
    if has_bom || looks_utf16 {
        let start = if has_bom { 2 } else { 0 };
        let units: Vec<u16> = bytes[start..].chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).to_string()
    }
}