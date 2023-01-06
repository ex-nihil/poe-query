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
    specs: HashMap<String, FileSpec>,
    enums: HashMap<String, EnumSpec>,
}

impl<'a> DatReader<'a> {

    pub fn from_install(language: &'a str, bundles: &'a BundleReader, spec_path: &Path) -> DatReader<'a> {
        let enums = FileSpec::read_enum_specs(spec_path);
        let specs = FileSpec::read_file_specs(spec_path, &enums, &HashMap::new());
        let specs = FileSpec::read_file_specs(spec_path, &enums, &specs);

        DatReader {
            language,
            bundle_reader: bundles,
            specs,
            enums
        }
    }

    fn get_filepath(&self, filename: &str) -> String {
        if self.language == "English" {
            return format!("Data/{}.dat64", filename)
        }
        format!("Data/{}/{}.dat64", self.language, filename)
    }
}

pub trait DatStoreImpl<'a> {
    fn file_by_filename(&self, filename: &str) -> Option<DatFile>;
    fn spec(&self, path: &str) -> Option<&FileSpec>;
    fn spec_by_export(&self, export: &str) -> Option<&FileSpec>;
    fn exports(&self) -> HashSet<&str>;
    fn enum_name(&self, path: &str) -> Option<&EnumSpec>;
}

impl<'a> DatStoreImpl<'a> for DatReader<'a> {
    fn file_by_filename(&self, filename: &str) -> Option<DatFile> {
        let path = self.get_filepath(filename);
        let spec = self.spec(filename);
        info!("Unpacking {}", path);
        // TODO: remove unwrap() in poe_bundle and return an actual error
        let Ok(bytes) = self.bundle_reader.bytes(&path) else { return None };

        let dat_file = DatFile::from_bytes(path, bytes).ok();
        match (spec, dat_file) {
            (Some(file_specification), Some(dat_file)) => {
                dat_file.valid(file_specification);
                Some(dat_file)
            },
            (_, dat_file) => dat_file
        }
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