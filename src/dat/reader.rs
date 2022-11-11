use std::collections::{HashMap, HashSet};

use log::info;
use poe_bundle::reader::{BundleReader, BundleReaderRead};

use crate::dat::specification::EnumSpec;

use super::file::DatFile;
use super::specification::FileSpec;

pub struct DatContainer<'a> {
    bundle_reader: &'a BundleReader,
    specs: HashMap<String, FileSpec>,
    enums: HashMap<String, EnumSpec>,
}

impl<'a> DatContainer<'a> {

    pub fn from_install(bundles: &'a BundleReader, spec_path: &str) -> DatContainer<'a> {
        let enums = FileSpec::read_all_enum_specs(spec_path);
        let specs = FileSpec::read_all_specs(spec_path, &enums);

        DatContainer {
            bundle_reader: bundles,
            specs,
            enums
        }
    }
}

pub trait DatStoreImpl<'a> {
    fn file(&self, path: &str) -> Option<DatFile>;
    fn spec(&self, path: &str) -> Option<&FileSpec>;
    fn spec_by_export(&self, export: &str) -> Option<&FileSpec>;
    fn exports(&self) -> HashSet<&str>;
    fn enum_name(&self, path: &str) -> Option<&EnumSpec>;
}

impl<'a> DatStoreImpl<'a> for DatContainer<'a> {
    fn file(&self, path: &str) -> Option<DatFile> {
        info!("Unpacking {}", path);
        match self.bundle_reader.bytes(path) {
            Ok(bytes) => Some(DatFile::from_bytes(path.to_string(), bytes)),
            Err(_) => None
        }
    }

    fn spec(&self, path: &str) -> Option<&FileSpec> {
        self.specs.get(path)
    }

    fn spec_by_export(&self, export: &str) -> Option<&FileSpec> {
        self.specs.values().find(|s| s.export == export)
    }

    fn exports(&self) -> HashSet<&str> {
        self.specs.iter().map(|(_, s)| s.export.as_str()).collect()
    }

    fn enum_name(&self, path: &str) -> Option<&EnumSpec> {
        self.enums.get(path)
    }
}