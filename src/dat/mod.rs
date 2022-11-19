use std::collections::{HashMap, HashSet};
use std::path::Path;
use log::info;
use poe_bundle::{BundleReader, BundleReaderRead};
use crate::dat::file::DatFile;
use crate::dat::specification::{EnumSpec, FileSpec};

pub mod util;
pub mod specification;
pub mod file;


pub struct DatReader<'a> {
    language: &'a str,
    bundle_reader: &'a BundleReader,
    pub specs: HashMap<String, FileSpec>,
    enums: HashMap<String, EnumSpec>,
}

impl<'a> DatReader<'a> {

    pub fn from_install(language: &'a str, bundles: &'a BundleReader, spec_path: &Path) -> DatReader<'a> {
        let enums = FileSpec::read_all_enum_specs(spec_path);
        let specs = FileSpec::read_all_specs(spec_path, &enums);

        DatReader {
            language,
            bundle_reader: bundles,
            specs,
            enums
        }
    }

    fn get_filepath(&self, filename: &str) -> String {
        if self.language == "English" {
            return format!("Data/{}.dat", filename)
        }
        format!("Data/{}/{}.dat", self.language, filename)
    }
}

pub trait DatStoreImpl<'a> {
    fn file(&self, spec: &FileSpec) -> Option<DatFile>;
    fn file_by_filename(&self, filename: &str) -> Option<DatFile>;
    fn spec(&self, path: &str) -> Option<&FileSpec>;
    fn spec_by_export(&self, export: &str) -> Option<&FileSpec>;
    fn exports(&self) -> HashSet<&str>;
    fn enum_name(&self, path: &str) -> Option<&EnumSpec>;
}

impl<'a> DatStoreImpl<'a> for DatReader<'a> {
    fn file(&self, spec: &FileSpec) -> Option<DatFile> {
        let path = self.get_filepath(&spec.filename);
        info!("Unpacking {}", path);
        let Ok(bytes) = self.bundle_reader.bytes(&path) else { return None };

        let file = DatFile::from_bytes(path, bytes);
        file.valid(spec);

        Some(file)
    }

    fn file_by_filename(&self, filename: &str) -> Option<DatFile> {
        let path = self.get_filepath(filename);
        info!("Unpacking {}", path);
        //let Some(spec) = self.spec(&path) else { return None };
        let Ok(bytes) = self.bundle_reader.bytes(&path) else { return None };

        let file = DatFile::from_bytes(path, bytes);
        Some(file)
    }

    fn spec(&self, path: &str) -> Option<&FileSpec> {
        self.specs.get(path)
    }

    fn spec_by_export(&self, export: &str) -> Option<&FileSpec> {
        self.specs.values().find(|s| s.filename == export)
    }

    fn exports(&self) -> HashSet<&str> {
        self.specs.iter().map(|(_, s)| s.filename.as_str()).collect()
    }

    fn enum_name(&self, path: &str) -> Option<&EnumSpec> {
        self.enums.get(path)
    }
}