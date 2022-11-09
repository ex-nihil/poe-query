use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Error;
use apollo_parser::ast::AstNode;
use log::{debug, error, warn};

use poe_bundle::reader::{BundleReader, BundleReaderRead};
use crate::dat::specification::EnumSpec;
use crate::FieldSpec;

use super::file::DatFile;
use super::file::DatFileRead;
use super::specification::FileSpec;
use super::traverse::TraversalContext;

pub struct DatContainer {
    bundle_reader: BundleReader,
    files: HashMap<String, DatFile>,
    specs: HashMap<String, FileSpec>,
    enums: HashMap<String, EnumSpec>,
}

// TODO: lazy loading
impl DatContainer {

    pub fn from_install(path: &str, spec_path: &str) -> DatContainer {
        let enums = FileSpec::read_all_enum_specs(spec_path);
        let specs = FileSpec::read_all_specs(spec_path, &enums);

        let bundles = BundleReader::from_install(path);

        let dat_files: HashMap<String, DatFile> = specs
            .iter()
            .filter(|(path, spec)| {
                debug!("Reading {}", path);
                match bundles.size_of(path) {
                    None => {
                        error!("Unable to read {}", path);
                        false
                    }
                    Some(_) => true
                }
            })
            .map(|(path, spec)| {
                let size = bundles.size_of(path).expect(format!("bundle size_of {}", path).as_str());
                let file = match bundles.bytes(path) {
                    Ok(bytes) => DatFile::from_bytes(bytes),
                    Err(_) => panic!("unable to read {}", path)
                };
                file.valid(spec);
                (path.clone(), file)
            })
            .collect();

        DatContainer {
            bundle_reader: bundles,
            files: dat_files,
            specs,
            enums,
        }
    }
}

#[derive(Clone, Copy)]
pub struct DatStore<'a> {
    pub files: &'a HashMap<String, DatFile>,
    pub specs: &'a HashMap<String, FileSpec>,
    pub enums: &'a HashMap<String, EnumSpec>,
}

pub trait DatStoreImpl<'a> {
    fn file(&self, path: &str) -> Option<&'a DatFile>;
    fn spec(&self, path: &str) -> Option<&'a FileSpec>;
    fn spec_by_export(&self, export: &str) -> Option<&'a FileSpec>;
    fn exports(&self) -> HashSet<&str>;
    fn enum_name(&self, path: &str) -> Option<&'a EnumSpec>;
}

impl<'a> DatStoreImpl<'a> for DatStore<'a> {
    fn file(&self, path: &str) -> Option<&'a DatFile> {
        self.files.get(path)
    }

    fn enum_name(&self, path: &str) -> Option<&'a EnumSpec> {
        self.enums.get(path)
    }

    fn spec(&self, path: &str) -> Option<&'a FileSpec> {
        self.specs.get(path)
    }

    fn spec_by_export(&self, export: &str) -> Option<&'a FileSpec> {
        self.specs.values().find(|s| s.export == export)
    }

    fn exports(&self) -> HashSet<&str> {
        self.specs.iter().map(|(_, s)| s.export.as_str()).collect()
    }
}

pub trait DatContainerImpl {
    fn navigate(&self) -> TraversalContext;
}

impl DatContainerImpl for DatContainer {
    fn navigate(&self) -> TraversalContext {
        TraversalContext {
            store: DatStore {
                files: &self.files,
                specs: &self.specs,
                enums: &self.enums,
            },
            variables: HashMap::new(),
            current_field: None,
            current_file: None,
            identity: None,
        }
    }
}